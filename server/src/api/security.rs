//! Proteção da API local: só aceita pedidos de localhost com o token da sessão.
//!
//! Sem isto, qualquer página web aberta no browser poderia chamar http://127.0.0.1:<porta> (CSRF) e
//! um ataque de DNS rebinding conseguiria o mesmo com outro nome de anfitrião. Por isso:
//!  - o token é obrigatório (Authorization: Bearer ...), gerado em cada arranque do servidor;
//!  - o cabeçalho Host tem de ser localhost/127.0.0.1/[::1];
//!  - não há CORS: os browsers não conseguem ler as respostas de outras origens.
use crate::paths;
use rand::RngCore;
use serde_json::json;

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Comparação em tempo constante (não revela quantos caracteres coincidem).
pub fn token_matches(supplied: &str, expected: &str) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }
    supplied.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

pub fn host_allowed(host_header: Option<&str>) -> bool {
    let Some(h) = host_header.map(|h| h.trim().to_lowercase()).filter(|h| !h.is_empty()) else { return false };
    let host = if h.starts_with('[') {
        // IPv6 literal: [::1]:8765
        h.split(']').next().map(|s| format!("{s}]")).unwrap_or(h.clone())
    } else {
        match h.rsplit_once(':') {
            Some((name, _)) => name.to_string(),
            None => h.clone(),
        }
    };
    ["127.0.0.1", "localhost", "[::1]", "::1"].contains(&host.as_str())
}

pub fn bearer(authorization: Option<&str>) -> &str {
    match authorization {
        Some(h) if h.len() > 7 && h[..7].eq_ignore_ascii_case("bearer ") => &h[7..],
        _ => "",
    }
}

/// Guarda a porta e o token para a UI se ligar (ficheiro na pasta do utilizador).
pub fn write_state(port: u16, token: &str) -> std::io::Result<std::path::PathBuf> {
    let path = paths::api_state_file();
    let mut data = crate::util::Json::new();
    data.insert("port".into(), json!(port));
    data.insert("token".into(), json!(token));
    data.insert("pid".into(), json!(std::process::id()));
    crate::util::write_json_atomic(&path, &data)?;
    Ok(path)
}

pub fn clear_state() {
    let _ = std::fs::remove_file(paths::api_state_file());
}
