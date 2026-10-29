mod common;
use common::*;
use isep_assistant::api::{handle, App, Req, Resp};
use isep_assistant::config::{load_config, save_config};
use isep_assistant::http::decode_component;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const TOKEN: &str = "t";

fn call(app: &App, method: &str, path: &str, body: Option<Value>) -> Resp {
    handle(
        app,
        &Req {
            method: method.into(),
            path: format!("/api/v1{path}"),
            query: vec![],
            host: Some("127.0.0.1:1".into()),
            authorization: Some(format!("Bearer {TOKEN}")),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
        },
    )
}

fn b64url(bytes: &[u8]) -> String {
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

fn param(url: &str, name: &str) -> String {
    let q = url.split_once('?').unwrap().1;
    q.split('&')
        .find_map(|p| p.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| decode_component(v)))
        .unwrap_or_else(|| panic!("falta {name} em {url}"))
}

fn form(body: &str, name: &str) -> Option<String> {
    body.split('&').find_map(|p| p.split_once('=').filter(|(k, _)| decode_component(k) == name).map(|(_, v)| decode_component(v)))
}

struct Login {
    server: FakeServer,
}

fn fake_login(status: u16, reply: Value) -> Login {
    Login { server: fake_server(move |_| Reply::json(status, reply.clone())) }
}

#[cfg(windows)]
#[test]
fn sign_in_url_uses_the_school_flow_with_pkce_and_the_code_is_exchanged_for_an_encrypted_token() {
    let _home = Home::new();
    let login = fake_login(200, json!({"access_token": "at", "refresh_token": "rt-da-escola"}));
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.server.base);
    let app = App::new(TOKEN);

    let start = call(&app, "POST", "/email/oauth/start", Some(json!({"email": "1234567@isep.ipp.pt"})));
    assert_eq!(start.status, 200, "{}", start.body);
    assert_eq!(start.body["redirect_uri"], "https://localhost");
    let url = start.body["auth_url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("{}/organizations/oauth2/v2.0/authorize?", login.server.base)), "{url}");
    assert_eq!(param(&url, "client_id"), "9e5f94bc-e8a4-4e73-b8be-63364c29d753");
    assert_eq!(param(&url, "response_type"), "code");
    assert_eq!(param(&url, "redirect_uri"), "https://localhost");
    assert_eq!(param(&url, "code_challenge_method"), "S256");
    assert_eq!(param(&url, "login_hint"), "1234567@isep.ipp.pt");
    let scope = param(&url, "scope");
    assert!(scope.contains("IMAP.AccessAsUser.All") && scope.contains("offline_access"), "{scope}");
    let (state, challenge) = (param(&url, "state"), param(&url, "code_challenge"));
    assert!(state.len() >= 16 && challenge.len() == 43, "state={state} challenge={challenge}");

    // a Microsoft devolve o utilizador a https://localhost com o código e o mesmo estado
    let back = format!("https://localhost/?code=CODIGO-123&state={state}&session_state=xyz");
    let done = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": back, "email": "1234567@isep.ipp.pt"})));
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    assert_eq!(done.body, json!({"ok": true}), "{}", done.body);

    // o pedido de troca leva o código e o segredo PKCE cujo SHA-256 é o desafio enviado no início
    let exchange = login.server.requests().into_iter().find(|r| r.url.ends_with("/token")).expect("pedido ao token");
    assert_eq!(form(&exchange.body, "grant_type").as_deref(), Some("authorization_code"));
    assert_eq!(form(&exchange.body, "code").as_deref(), Some("CODIGO-123"));
    assert_eq!(form(&exchange.body, "redirect_uri").as_deref(), Some("https://localhost"));
    let verifier = form(&exchange.body, "code_verifier").unwrap();
    assert_eq!(b64url(&Sha256::digest(verifier.as_bytes())), challenge, "o segredo PKCE tem de corresponder ao desafio");

    // fica guardado cifrado, o método passa a IMAP e o endereço fica nas definições
    let cfg = load_config();
    let stored = cfg["ms_refresh_token"].as_str().unwrap();
    assert!(!stored.contains("rt-da-escola") && stored.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(isep_assistant::dpapi::decrypt(stored).unwrap(), "rt-da-escola");
    assert_eq!(cfg["email_method"], "imap");
    let settings = call(&app, "GET", "/settings", None).body;
    assert_eq!(settings["email"]["oauth_connected"], true);
    assert_eq!(settings["email"]["connected"], true);
    assert_eq!(settings["email"]["address"], "1234567@isep.ipp.pt");
    assert!(!settings.to_string().contains("rt-da-escola"));
}

#[test]
fn a_response_with_another_state_or_no_pending_sign_in_is_refused_and_never_asks_for_a_token() {
    let _home = Home::new();
    let login = fake_login(200, json!({"refresh_token": "nao-devia-ser-pedido"}));
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.server.base);
    let app = App::new(TOKEN);

    // sem início de sessão em curso
    let none = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": "https://localhost/?code=c&state=s"})));
    assert_eq!(none.body["ok"], false);
    assert!(none.body["error"].as_str().unwrap().contains("expirou ou não foi iniciado"));

    // estado diferente (alguém a tentar injetar um código)
    call(&app, "POST", "/email/oauth/start", Some(json!({"email": "a@isep.ipp.pt"})));
    let forged = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": "https://localhost/?code=c&state=OUTRO"})));
    assert_eq!(forged.body["ok"], false);
    assert!(forged.body["error"].as_str().unwrap().contains("estado diferente"), "{}", forged.body);
    // o pedido pendente foi consumido: reutilizá-lo também falha
    let again = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": "https://localhost/?code=c&state=OUTRO"})));
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    assert_eq!(again.body["ok"], false);
    assert!(login.server.requests().iter().all(|r| !r.url.ends_with("/token")), "nenhum token foi pedido");
    assert!(load_config().get("ms_refresh_token").is_none());
}

#[test]
fn the_return_address_must_be_the_microsoft_one() {
    let _home = Home::new();
    let app = App::new(TOKEN);
    call(&app, "POST", "/email/oauth/start", Some(json!({"email": "a@isep.ipp.pt"})));
    let r = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": "https://evil.example/?code=c&state=s"})));
    assert_eq!(r.body["ok"], false);
    assert!(r.body["error"].as_str().unwrap().contains("https://localhost"));
    assert_eq!(call(&app, "POST", "/email/oauth/start", Some(json!({"email": "sem-arroba"}))).status, 422);
}

#[test]
fn cancelled_denied_and_admin_consent_have_clear_messages() {
    let _home = Home::new();
    let app = App::new(TOKEN);
    let attempt = |query: &str| {
        let start = call(&app, "POST", "/email/oauth/start", Some(json!({"email": "a@isep.ipp.pt"})));
        let state = param(start.body["auth_url"].as_str().unwrap(), "state");
        let url = format!("https://localhost/?{}&state={state}", query);
        call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": url}))).body["error"].as_str().unwrap().to_string()
    };
    assert!(attempt("error=access_denied&error_description=The+user+cancelled").contains("Cancelaste"));
    let admin = attempt("error=access_denied&error_description=AADSTS65001%3A+consent+required");
    assert!(admin.contains("administrador"), "{admin}");
    assert!(attempt("error=server_error&error_description=falhou+algo").contains("falhou algo"));
}

#[cfg(windows)]
#[test]
fn a_rejected_code_exchange_is_reported_without_storing_anything() {
    let _home = Home::new();
    let login = fake_login(400, json!({"error": "invalid_grant", "error_description": "AADSTS70000: o código expirou"}));
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.server.base);
    let app = App::new(TOKEN);
    let start = call(&app, "POST", "/email/oauth/start", Some(json!({"email": "a@isep.ipp.pt"})));
    let state = param(start.body["auth_url"].as_str().unwrap(), "state");
    let r = call(&app, "POST", "/email/oauth/finish", Some(json!({"redirect_url": format!("https://localhost/?code=velho&state={state}")})));
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    assert_eq!(r.body["ok"], false);
    assert!(r.body["error"].as_str().unwrap().contains("o código expirou"), "{}", r.body);
    assert!(load_config().get("ms_refresh_token").is_none());
}

#[test]
fn signing_out_removes_only_the_microsoft_token() {
    let _home = Home::new();
    let mut cfg = json_map(json!({"email_method": "imap", "email_address": "a@isep.ipp.pt"}));
    isep_assistant::oauth::store_refresh_token(&mut cfg, "rt").unwrap();
    save_config(&cfg).unwrap();
    let app = App::new(TOKEN);
    let r = call(&app, "PUT", "/settings", Some(json!({"email": {"sign_out": true}})));
    assert_eq!(r.body["settings"]["email"]["oauth_connected"], false);
    assert!(load_config().get("ms_refresh_token").is_none());
    assert_eq!(load_config()["email_address"], "a@isep.ipp.pt");
}

// ---------------------------------------------------------------- o OAuth tem prioridade sobre a palavra-passe

#[cfg(windows)]
#[test]
fn a_connected_microsoft_account_is_used_instead_of_a_password_the_school_disabled() {
    let Some(node) = isep_assistant::util::which("node") else { return };
    let _home = Home::new();
    let login = fake_login(200, json!({"access_token": "at-escola", "refresh_token": "rt-novo"}));

    // IMAP falso: guarda o que recebe e aceita qualquer autenticação
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut out = stream.try_clone().unwrap();
        out.write_all(b"* OK pronto\r\n").unwrap();
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            seen2.lock().unwrap().push(line.clone());
            let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
            if cmd.starts_with("SELECT") {
                out.write_all(format!("* 0 EXISTS\r\n{tag} OK aberta\r\n").as_bytes()).unwrap();
            } else if cmd.starts_with("UID SEARCH") {
                out.write_all(format!("* SEARCH\r\n{tag} OK feito\r\n").as_bytes()).unwrap();
            } else if cmd.starts_with("LOGOUT") {
                out.write_all(format!("* BYE\r\n{tag} OK adeus\r\n").as_bytes()).unwrap();
                break;
            } else {
                out.write_all(format!("{tag} OK feito\r\n").as_bytes()).unwrap();
            }
        }
    });
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.server.base);
    std::env::set_var("ISEP_IMAP_HOST", "127.0.0.1");
    std::env::set_var("ISEP_IMAP_PORT", port.to_string());
    std::env::set_var("ISEP_IMAP_PLAIN", "1");
    std::env::set_var("ISEP_NODE", &node);
    std::env::set_var("ISEP_MAIL_SCRIPT", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("EmailNotifier").join("mail_fetcher.cjs"));

    let mut cfg = json_map(json!({"email_method": "imap", "email_address": "a@isep.ipp.pt", "email_password": "palavra-passe-antiga"}));
    isep_assistant::oauth::store_refresh_token(&mut cfg, "rt").unwrap();
    save_config(&cfg).unwrap();
    let app = App::new(TOKEN);
    let test = call(&app, "POST", "/email/test", None);
    for v in ["ISEP_MS_LOGIN_URL", "ISEP_IMAP_HOST", "ISEP_IMAP_PORT", "ISEP_IMAP_PLAIN", "ISEP_NODE", "ISEP_MAIL_SCRIPT"] {
        std::env::remove_var(v);
    }
    assert_eq!(test.body["ok"], true, "{}", test.body);
    let lines = seen.lock().unwrap().clone();
    assert!(lines.iter().any(|l| l.contains("AUTHENTICATE XOAUTH2")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("LOGIN")), "a palavra-passe não deve ser usada: {lines:?}");
}
