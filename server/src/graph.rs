//! Origem de email própria da app: Microsoft Graph (Outlook/Microsoft 365 do ISEP) com início de sessão por código
//! de dispositivo (OAuth2). Não precisa do Thunderbird: o utilizador autoriza uma vez e a app guarda o token de
//! renovação cifrado com o DPAPI. Fala HTTPS pela WinHTTP do sistema (sem bibliotecas TLS próprias).
use crate::dpapi;
use crate::http::{encode_component, HttpClient};
use crate::model::Email;
use crate::util::{str_of, truncate_chars, Json};
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

/// Cliente público e inquilino por omissão (os mesmos da versão Python). Podem ser trocados em `config.json`
/// (`ms_client_id`, `ms_tenant_id`) por um registo de aplicação próprio no Azure.
pub const DEFAULT_CLIENT_ID: &str = "de8bc8b5-d9f9-48b1-a8ad-b748da725064";
pub const DEFAULT_TENANT_ID: &str = "7a60a629-e61f-430c-9c63-266653b2b899";
const SCOPE: &str = "offline_access Mail.Read";
const BODY_LIMIT: usize = 4000;
const PAGE_SIZE: usize = 50;
const MAX_PAGES: usize = 10;

fn login_base() -> String {
    std::env::var("ISEP_MS_LOGIN_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "https://login.microsoftonline.com".into())
}
fn graph_base() -> String {
    std::env::var("ISEP_GRAPH_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "https://graph.microsoft.com".into())
}

fn client_id(config: &Json) -> String {
    let v = str_of(config, "ms_client_id");
    if v.is_empty() { DEFAULT_CLIENT_ID.into() } else { v.into() }
}
fn tenant_id(config: &Json) -> String {
    let v = str_of(config, "ms_tenant_id");
    if v.is_empty() { DEFAULT_TENANT_ID.into() } else { v.into() }
}

fn http() -> Result<HttpClient, String> {
    HttpClient::new(Duration::from_secs(30))
}

/// O inquilino do ISEP pode exigir que um administrador aprove aplicações de terceiros (AADSTS65001/90094/53003...).
fn needs_admin(description: &str) -> bool {
    let d = description.to_lowercase();
    ["aadsts65001", "aadsts90094", "aadsts900971", "aadsts53003", "aadsts50105", "admin", "consent"].iter().any(|k| d.contains(k))
}

const ADMIN_MESSAGE: &str = "A Microsoft do ISEP exige a aprovação de um administrador para esta ligação direta. Usa o Thunderbird (não precisa de aprovação).";

fn json_of(resp: &crate::http::Response) -> Value {
    serde_json::from_slice(&resp.body).unwrap_or(Value::Null)
}

/// A conta já foi autorizada? (há um token de renovação guardado)
pub fn is_connected(config: &Json) -> bool {
    !str_of(config, "ms_refresh_token").is_empty()
}

// ---------------------------------------------------------------- início de sessão (código de dispositivo)

/// Pede o código que o utilizador escreve em microsoft.com/devicelogin.
pub fn start_device_login(config: &Json) -> Result<Value, String> {
    let url = format!("{}/{}/oauth2/v2.0/devicecode", login_base(), tenant_id(config));
    let resp = http()?.post_form(&url, &[("client_id", &client_id(config)), ("scope", SCOPE)])?;
    let body = json_of(&resp);
    if resp.status != 200 {
        let why = body["error_description"].as_str().unwrap_or("pedido recusado");
        if needs_admin(why) {
            return Err(ADMIN_MESSAGE.into());
        }
        return Err(format!("A Microsoft recusou o pedido de início de sessão: {}", truncate_chars(why, 200)));
    }
    Ok(json!({
        "device_code": body["device_code"],
        "user_code": body["user_code"],
        "verification_uri": body["verification_uri"],
        "interval": body["interval"].as_u64().unwrap_or(5),
        "expires_in": body["expires_in"].as_u64().unwrap_or(900),
    }))
}

pub enum Poll {
    Pending,
    Done { refresh_token: String },
    Failed(String),
}

/// Pergunta se o utilizador já autorizou. Em caso de sucesso devolve o token de renovação (a gravar com `store_refresh_token`).
pub fn poll_device_login(config: &Json, device_code: &str) -> Result<Poll, String> {
    let url = format!("{}/{}/oauth2/v2.0/token", login_base(), tenant_id(config));
    let resp = http()?.post_form(
        &url,
        &[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", &client_id(config)), ("device_code", device_code)],
    )?;
    let body = json_of(&resp);
    if resp.status == 200 {
        return match body["refresh_token"].as_str() {
            Some(t) if !t.is_empty() => Ok(Poll::Done { refresh_token: t.to_string() }),
            _ => Ok(Poll::Failed("A Microsoft não devolveu um token de renovação.".into())),
        };
    }
    Ok(match body["error"].as_str() {
        Some("authorization_pending") | Some("slow_down") => Poll::Pending,
        Some("expired_token") | Some("authorization_declined") | Some("bad_verification_code") => {
            Poll::Failed("O código expirou ou a autorização foi recusada. Começa de novo.".into())
        }
        _ => {
            let why = body["error_description"].as_str().unwrap_or("erro desconhecido");
            Poll::Failed(if needs_admin(why) { ADMIN_MESSAGE.to_string() } else { truncate_chars(why, 200) })
        }
    })
}

/// Cifra o token de renovação para guardar em `config.json`.
pub fn store_refresh_token(config: &mut Json, refresh_token: &str) -> Result<(), String> {
    config.insert("ms_refresh_token".into(), json!(dpapi::encrypt(refresh_token)?));
    Ok(())
}

// ---------------------------------------------------------------- leitura

/// Troca o token de renovação por um token de acesso. Devolve também o novo token de renovação, se a Microsoft o rodou.
fn access_token(config: &Json) -> Result<(String, Option<String>), String> {
    let stored = str_of(config, "ms_refresh_token");
    if stored.is_empty() {
        return Err("A conta Microsoft ainda não foi ligada. Liga-a nas Definições.".into());
    }
    let refresh = dpapi::decrypt(stored)?;
    let url = format!("{}/{}/oauth2/v2.0/token", login_base(), tenant_id(config));
    let resp = http()?.post_form(
        &url,
        &[("grant_type", "refresh_token"), ("client_id", &client_id(config)), ("refresh_token", &refresh), ("scope", SCOPE)],
    )?;
    let body = json_of(&resp);
    if resp.status != 200 {
        let why = body["error_description"].as_str().unwrap_or("erro desconhecido");
        return Err(format!("A autorização da conta Microsoft expirou, liga-a de novo nas Definições. ({})", truncate_chars(why, 120)));
    }
    let access = body["access_token"].as_str().ok_or("A Microsoft não devolveu o token de acesso.")?.to_string();
    Ok((access, body["refresh_token"].as_str().map(str::to_owned).filter(|t| *t != refresh)))
}

fn local_from_graph(raw: &str) -> NaiveDateTime {
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.with_timezone(&Local).naive_local())
        .or_else(|_| NaiveDateTime::parse_from_str(raw.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S%.f").map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc).with_timezone(&Local).naive_local()))
        .unwrap_or_else(|_| Local::now().naive_local())
}

fn to_email(m: &Value) -> Email {
    let from = &m["from"]["emailAddress"];
    let (name, addr) = (from["name"].as_str().unwrap_or(""), from["address"].as_str().unwrap_or(""));
    let sender = match (name.is_empty(), addr.is_empty()) {
        (false, false) => format!("\"{name}\" <{addr}>"),
        (true, false) => addr.to_string(),
        (false, true) => name.to_string(),
        _ => "Desconhecido".to_string(),
    };
    let body = m["body"]["content"].as_str().filter(|b| !b.is_empty()).or_else(|| m["bodyPreview"].as_str()).unwrap_or("");
    Email {
        subject: m["subject"].as_str().filter(|s| !s.is_empty()).unwrap_or("Sem assunto").to_string(),
        sender,
        sender_email: addr.to_string(),
        received_time: local_from_graph(m["receivedDateTime"].as_str().unwrap_or("")).format("%Y-%m-%dT%H:%M:%S").to_string(),
        body: truncate_chars(body.trim(), BODY_LIMIT),
        message_id: m["internetMessageId"].as_str().map(str::to_owned).unwrap_or_default(),
    }
}

/// Emails da caixa de entrada recebidos depois de `since` (hora local), mais recentes primeiro.
/// Devolve também o token de renovação novo, se mudou, para o chamador o gravar.
pub fn fetch_inbox(config: &Json, since: NaiveDateTime) -> Result<(Vec<Email>, Option<String>), String> {
    let (token, rotated) = access_token(config)?;
    let since_utc = since.and_local_timezone(Local).earliest().map(|d| d.with_timezone(&Utc)).unwrap_or_else(|| since.and_utc());
    let filter = format!("receivedDateTime ge {}", since_utc.format("%Y-%m-%dT%H:%M:%SZ"));
    let mut url = format!(
        "{}/v1.0/me/mailFolders/inbox/messages?$filter={}&$orderby=receivedDateTime%20desc&$top={PAGE_SIZE}&$select=internetMessageId,subject,from,receivedDateTime,bodyPreview,body",
        graph_base(),
        encode_component(&filter)
    );
    let auth = format!("Bearer {token}");
    let client = http()?;
    let mut emails = Vec::new();
    for _ in 0..MAX_PAGES {
        let resp = client.get(&url, &[("Authorization", &auth), ("Prefer", "outlook.body-content-type=\"text\"")])?;
        let body = json_of(&resp);
        if resp.status != 200 {
            let why = body["error"]["message"].as_str().unwrap_or("erro desconhecido");
            return Err(format!("Não foi possível ler os emails (HTTP {}): {}", resp.status, truncate_chars(why, 200)));
        }
        emails.extend(body["value"].as_array().map(|a| a.iter().map(to_email).collect::<Vec<_>>()).unwrap_or_default());
        match body["@odata.nextLink"].as_str() {
            Some(next) => url = next.to_string(),
            None => break,
        }
    }
    Ok((emails, rotated))
}
