//! Início de sessão da conta de email da escola (Microsoft 365) por código de dispositivo (OAuth2), para a app ler a caixa
//! por IMAP sem depender de nenhum cliente de email. O utilizador autoriza uma vez; a app guarda só o token de renovação,
//! cifrado com o DPAPI. Fala HTTPS pela WinHTTP do sistema (sem bibliotecas TLS próprias).
//!
//! O cliente usado por omissão é o do Thunderbird (o do projeto Mozilla), que as escolas costumam ter aprovado. Pode trocar-se
//! por um registo próprio no Azure com `ms_client_id` / `ms_tenant_id` no `config.json` (o caminho certo para o serviço na nuvem).
use crate::dpapi;
use crate::http::HttpClient;
use crate::util::{str_of, truncate_chars, Json};
use serde_json::{json, Value};
use std::time::Duration;

/// ID público do Thunderbird na Microsoft. Não é um segredo: identifica a aplicação, não o utilizador.
pub const DEFAULT_CLIENT_ID: &str = "9e5f94bc-e8a4-4e73-b8be-63364c29d753";
/// «organizations»: qualquer conta escolar/profissional; a Microsoft encaminha para o inquilino da instituição.
pub const DEFAULT_TENANT: &str = "organizations";
const SCOPE: &str = "https://outlook.office.com/IMAP.AccessAsUser.All offline_access";

fn login_base() -> String {
    std::env::var("ISEP_MS_LOGIN_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "https://login.microsoftonline.com".into())
}

fn client_id(config: &Json) -> String {
    let v = str_of(config, "ms_client_id");
    if v.is_empty() { DEFAULT_CLIENT_ID.into() } else { v.into() }
}

fn tenant_id(config: &Json) -> String {
    let v = str_of(config, "ms_tenant_id");
    if v.is_empty() { DEFAULT_TENANT.into() } else { v.into() }
}

fn http() -> Result<HttpClient, String> {
    HttpClient::new(Duration::from_secs(30))
}

/// A escola pode exigir que um administrador aprove a aplicação (AADSTS65001/90094/53003...).
fn needs_admin(description: &str) -> bool {
    let d = description.to_lowercase();
    ["aadsts65001", "aadsts90094", "aadsts900971", "aadsts53003", "aadsts50105", "admin", "consent"].iter().any(|k| d.contains(k))
}

const ADMIN_MESSAGE: &str = "A tua escola exige a aprovação de um administrador para ligar o email. Pede ao apoio informático ou usa o Thunderbird.";

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

/// Troca o token de renovação por um token de acesso. Devolve também o novo token de renovação, se a Microsoft o rodou.
pub fn access_token(config: &Json) -> Result<(String, Option<String>), String> {
    let stored = str_of(config, "ms_refresh_token");
    if stored.is_empty() {
        return Err("A conta de email ainda não foi ligada. Liga-a nas Definições.".into());
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
        return Err(format!("A autorização da conta de email expirou, liga-a de novo nas Definições. ({})", truncate_chars(why, 120)));
    }
    let access = body["access_token"].as_str().ok_or("A Microsoft não devolveu o token de acesso.")?.to_string();
    Ok((access, body["refresh_token"].as_str().map(str::to_owned).filter(|t| *t != refresh)))
}
