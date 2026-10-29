//! Início de sessão da conta de email da escola (Microsoft 365) por código de dispositivo (OAuth2), para a app ler a caixa
//! por IMAP sem depender de nenhum cliente de email. O utilizador autoriza uma vez; a app guarda só o token de renovação,
//! cifrado com o DPAPI. Fala HTTPS pela WinHTTP do sistema (sem bibliotecas TLS próprias).
//!
//! O cliente usado por omissão é o do Thunderbird (o do projeto Mozilla), que as escolas costumam ter aprovado. Pode trocar-se
//! por um registo próprio no Azure com `ms_client_id` / `ms_tenant_id` no `config.json` (o caminho certo para o serviço na nuvem).
use crate::dpapi;
use crate::http::HttpClient;
use crate::util::{str_of, truncate_chars, Json};
use percent_encoding::percent_decode_str;
use rand::RngCore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// ID público do Thunderbird na Microsoft. Não é um segredo: identifica a aplicação, não o utilizador.
pub const DEFAULT_CLIENT_ID: &str = "9e5f94bc-e8a4-4e73-b8be-63364c29d753";
/// «organizations»: qualquer conta escolar/profissional; a Microsoft encaminha para o inquilino da instituição.
pub const DEFAULT_TENANT: &str = "organizations";
const SCOPE: &str = "https://outlook.office.com/IMAP.AccessAsUser.All offline_access";
/// O endereço para onde a Microsoft devolve o utilizador depois de entrar. É o do Thunderbird: a app abre a página de
/// início de sessão numa janela sua, apanha este redirecionamento (nunca chega a ligar-se a `localhost`) e lê o código.
pub const REDIRECT_URI: &str = "https://localhost";
const PENDING_MAX_AGE: Duration = Duration::from_secs(15 * 60);

/// Início de sessão em curso (o que é preciso para validar o regresso): estado anti-falsificação e segredo PKCE.
struct Pending {
    state: String,
    verifier: String,
    started: Instant,
}
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

fn base64url(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    base64url(&buf)
}

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

// ---------------------------------------------------------------- início de sessão (código de autorização + PKCE)

/// Endereço da página de início de sessão da Microsoft. `login_hint` preenche o email do utilizador.
pub fn start_auth_code(config: &Json, login_hint: &str) -> Result<Value, String> {
    let state = random_token(18);
    let verifier = random_token(48);
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    let enc = |v: &str| crate::http::encode_component(v);
    let mut url = format!(
        "{}/{}/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        login_base(),
        tenant_id(config),
        enc(&client_id(config)),
        enc(REDIRECT_URI),
        enc(SCOPE),
        enc(&state),
        enc(&challenge)
    );
    if !login_hint.trim().is_empty() {
        url.push_str(&format!("&login_hint={}", enc(login_hint.trim())));
    }
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending { state, verifier, started: Instant::now() });
    Ok(json!({ "auth_url": url, "redirect_uri": REDIRECT_URI }))
}

fn query_of(url: &str) -> Vec<(String, String)> {
    let q = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    let q = q.split('#').next().unwrap_or("");
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            let dec = |s: &str| percent_decode_str(&s.replace('+', " ")).decode_utf8_lossy().into_owned();
            (dec(k), dec(v))
        })
        .collect()
}

/// Recebe o endereço para onde a Microsoft redirecionou o utilizador, valida-o e troca o código pelo token de renovação.
pub fn finish_auth_code(config: &Json, redirect_url: &str) -> Result<String, String> {
    if !redirect_url.starts_with(REDIRECT_URI) {
        return Err("O endereço não é o do regresso da Microsoft (deve começar por https://localhost).".into());
    }
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(p) = pending.filter(|p| p.started.elapsed() < PENDING_MAX_AGE) else {
        return Err("O início de sessão expirou ou não foi iniciado. Começa de novo.".into());
    };
    let q = query_of(redirect_url);
    let get = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
    if let Some(err) = get("error") {
        let why = get("error_description").unwrap_or(err);
        return Err(if err == "access_denied" && !needs_admin(why) {
            "Cancelaste o início de sessão.".to_string()
        } else if needs_admin(why) {
            ADMIN_MESSAGE.to_string()
        } else {
            format!("A Microsoft recusou o início de sessão: {}", truncate_chars(why, 200))
        });
    }
    if get("state") != Some(p.state.as_str()) {
        return Err("A resposta da Microsoft não corresponde a este pedido (estado diferente). Começa de novo.".into());
    }
    let Some(code) = get("code").filter(|c| !c.is_empty()) else {
        return Err("A Microsoft não devolveu o código de autorização.".into());
    };
    let url = format!("{}/{}/oauth2/v2.0/token", login_base(), tenant_id(config));
    let resp = http()?.post_form(
        &url,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id(config)),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", &p.verifier),
            ("scope", SCOPE),
        ],
    )?;
    let body = json_of(&resp);
    if resp.status != 200 {
        let why = body["error_description"].as_str().unwrap_or("erro desconhecido");
        return Err(if needs_admin(why) { ADMIN_MESSAGE.to_string() } else { format!("A Microsoft recusou o início de sessão: {}", truncate_chars(why, 200)) });
    }
    match body["refresh_token"].as_str() {
        Some(t) if !t.is_empty() => Ok(t.to_string()),
        _ => Err("A Microsoft não devolveu um token de renovação.".into()),
    }
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
