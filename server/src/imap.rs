//! Origem de email própria da app: IMAP à caixa de entrada da escola, com email + palavra-passe (o predefinido) ou, se já
//! estiver ligado, com OAuth2 (XOAUTH2).
//!
//! A ligação TLS e o protocolo IMAP correm num pequeno script (`mail_fetcher.cjs`) executado pelo Electron que já vai dentro
//! da app, como o do Session. Assim não há Thunderbird, nem Node.js, nem bibliotecas TLS a instalar. A palavra-passe fica
//! cifrada com o DPAPI do Windows e vai para o script por stdin (nunca na linha de comandos).
use crate::mail::{email_from_raw, FetchOptions, MailSource};
use crate::model::Email;
use crate::oauth;
use crate::paths;
use crate::system::node_runtime;
use crate::util::{run_with_timeout, str_of, Json};
use chrono::{Duration as Dur, NaiveDateTime};
use serde_json::Value;
use std::time::Duration;

const DEFAULT_HOST: &str = "outlook.office365.com";
const DEFAULT_PORT: u16 = 993;
const FETCH_TIMEOUT: Duration = Duration::from_secs(180);

pub struct ImapSource;

/// A palavra-passe do email: a cifrada pela app e, para quem vem da versão Python, a que estava em texto simples.
pub fn password_of(config: &Json) -> Option<String> {
    let enc = str_of(config, "email_password_enc");
    if !enc.is_empty() {
        return crate::dpapi::decrypt(enc).ok().filter(|p| !p.is_empty());
    }
    Some(str_of(config, "email_password").to_string()).filter(|p| !p.is_empty())
}

pub fn has_password(config: &Json) -> bool {
    !str_of(config, "email_password_enc").is_empty() || !str_of(config, "email_password").is_empty()
}

/// O email está pronto a ler? (endereço e uma forma de entrar)
pub fn is_configured(config: &Json) -> bool {
    str_of(config, "email_address").contains('@') && (has_password(config) || oauth::is_connected(config))
}

/// Cifra e guarda a palavra-passe (e apaga a de texto simples, se existir).
pub fn store_password(config: &mut Json, password: &str) -> Result<(), String> {
    config.insert("email_password_enc".into(), serde_json::json!(crate::dpapi::encrypt(password)?));
    config.remove("email_password");
    Ok(())
}

fn server(config: &Json) -> (String, u16) {
    let host = std::env::var("ISEP_IMAP_HOST").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| {
        let h = str_of(config, "imap_host");
        if h.is_empty() { DEFAULT_HOST.into() } else { h.into() }
    });
    let port = std::env::var("ISEP_IMAP_PORT").ok().and_then(|v| v.parse().ok()).or_else(|| config.get("imap_port").and_then(Value::as_u64).map(|p| p as u16));
    (host, port.unwrap_or(DEFAULT_PORT))
}

impl MailSource for ImapSource {
    fn id(&self) -> &'static str {
        "imap"
    }

    fn fetch(&self, config: &Json, since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String> {
        let user = str_of(config, "email_address");
        if !user.contains('@') {
            return Err("Indica o teu email da escola nas Definições.".into());
        }
        if opts.sync && !crate::mail::wait_for_internet(Duration::from_secs(30)) {
            return Err("Sem ligação à Internet. A abortar sem avançar a data de última execução.".into());
        }
        // a conta Microsoft ligada tem prioridade: muitas escolas desligaram o login por palavra-passe
        let (secret, auth) = if oauth::is_connected(config) {
            let (token, rotated) = oauth::access_token(config)?;
            if let Some(t) = rotated {
                // a Microsoft roda o token de renovação: grava o novo para a próxima execução
                let mut fresh = crate::config::load_config();
                if oauth::store_refresh_token(&mut fresh, &t).is_ok() {
                    let _ = crate::config::save_config(&fresh);
                }
            }
            (token, "xoauth2")
        } else if let Some(p) = password_of(config) {
            (p, "password")
        } else {
            return Err("Entra com a conta da escola ou indica a palavra-passe do email nas Definições.".into());
        };
        let node = node_runtime().ok_or("Não foi encontrado o motor da app (Node.js).")?;
        let script = paths::mail_script();
        if !script.exists() {
            return Err("Falta o ficheiro mail_fetcher.cjs. Reinstala a app.".into());
        }
        let (host, port) = server(config);
        // o IMAP só filtra por dia: pede um dia a mais e filtra à hora certa aqui
        let day = (since - Dur::days(1)).format("%Y-%m-%d").to_string();
        let mut cmd = node.command();
        cmd.arg(&script).args(["--host", &host, "--port", &port.to_string(), "--user", user, "--since", &day, "--auth", auth]);
        let out = run_with_timeout(cmd, Some(&secret), FETCH_TIMEOUT).map_err(|e| format!("não foi possível correr o leitor de email: {e}"))?;
        if out.timed_out {
            return Err("O servidor de email demorou demasiado a responder.".into());
        }
        let reply: Value = serde_json::from_str(out.stdout.trim().lines().last().unwrap_or("")).map_err(|_| {
            format!("Resposta inesperada do leitor de email. {}", out.stderr.trim().chars().take(160).collect::<String>())
        })?;
        if let Some(e) = reply["error"].as_str() {
            return Err(format!("Não foi possível ler os emails: {e}"));
        }
        let mut emails: Vec<Email> = reply["messages"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["raw"].as_str().and_then(|h| hex::decode(h).ok())).filter_map(|raw| email_from_raw(&raw, since)).collect())
            .unwrap_or_default();
        emails.sort_by(|a, b| b.received_time.cmp(&a.received_time));
        Ok(emails)
    }
}
