mod common;
use common::*;
use isep_assistant::ai::*;
use isep_assistant::model::Email;
use serde_json::{json, Value};

fn emails() -> Vec<Email> {
    vec![Email::new("Aula de ESINF", "docente@isep.ipp.pt", "Sala F105 às 10h.")]
}

fn run(cfg: Value) -> Result<String, String> {
    summarize_with_ai(&emails(), &json_map(cfg), &["📥 NOVO NO MOODLE (1 ficheiro(s))".to_string()], "seg 26/10/2026 19:00")
}

fn ai_server() -> FakeServer {
    fake_server(|s| {
        if s.url.contains("anthropic") {
            Reply::json(200, json!({"content": [{"type": "text", "text": format!("RESPOSTA-ANTHROPIC {}", "x".repeat(40))}]}))
        } else if s.url.contains("generateContent") {
            Reply::json(200, json!({"candidates": [{"content": {"parts": [{"text": format!("RESPOSTA-GEMINI {}", "x".repeat(40))}]}}]}))
        } else if s.url.contains("falha") {
            Reply::json(401, json!({"error": {"message": "Incorrect API key provided: sk-SEGREDO123456"}}))
        } else {
            Reply::json(200, json!({"choices": [{"message": {"content": format!("RESPOSTA-OPENAI {}", "x".repeat(40))}}]}))
        }
    })
}

#[test]
fn prompt_contains_emails_moodle_and_rules() {
    let p = build_prompt(&emails(), &["📥 NOVO NO MOODLE (1 ficheiro(s))".into()], "seg 26/10");
    assert!(p.contains("Sala F105") && p.contains("NOVO NO MOODLE") && p.contains("Não inventes") && p.contains("seg 26/10"));
}

#[test]
fn prompt_limits_amount_of_email_text() {
    let many: Vec<Email> = (0..100).map(|i| Email::new(&format!("Assunto {i}"), "x", &"y".repeat(5000))).collect();
    let p = build_prompt(&many, &[], "hoje");
    assert_eq!(p.matches("--- EMAIL #").count(), MAX_EMAILS);
    assert!(p.chars().count() < MAX_EMAILS * (MAX_BODY_CHARS + 200) + 3000);
}

#[test]
fn openai_compatible_sends_bearer_and_model() {
    let srv = ai_server();
    let out = run(json!({"ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K", "ai_model": "m1",
                         "ai_base_url": format!("{}/v1/chat/completions", srv.base)}))
    .unwrap();
    assert!(out.starts_with("RESPOSTA-OPENAI"));
    let req = &srv.requests()[0];
    assert_eq!(req.header("authorization"), Some("Bearer K"));
    let body: Value = serde_json::from_str(&req.body).unwrap();
    assert_eq!(body["model"], "m1");
    assert!(body["messages"][0]["content"].as_str().unwrap().contains("Sala F105"));
}

#[test]
fn anthropic_uses_x_api_key_header_and_default_model() {
    let srv = ai_server();
    let out = run(json!({"ai_provider": "api", "ai_api_type": "anthropic", "ai_api_key": "AK",
                         "ai_base_url": format!("{}/anthropic/v1/messages", srv.base)}))
    .unwrap();
    assert!(out.starts_with("RESPOSTA-ANTHROPIC"));
    let req = &srv.requests()[0];
    assert_eq!(req.header("x-api-key"), Some("AK"));
    assert!(req.header("anthropic-version").is_some());
    let body: Value = serde_json::from_str(&req.body).unwrap();
    assert_eq!(body["model"], api_defaults("anthropic").unwrap().model);
}

#[test]
fn gemini_uses_header_not_query_string_and_substitutes_model() {
    let srv = ai_server();
    let out = run(json!({"ai_provider": "api", "ai_api_type": "gemini", "ai_api_key": "GK", "ai_model": "g-test",
                         "ai_base_url": format!("{}/models/{{model}}:generateContent", srv.base)}))
    .unwrap();
    assert!(out.starts_with("RESPOSTA-GEMINI"));
    let req = &srv.requests()[0];
    assert_eq!(req.url, "/models/g-test:generateContent");
    assert_eq!(req.header("x-goog-api-key"), Some("GK"));
}

#[test]
fn api_key_from_environment() {
    let _home = Home::new();
    let srv = ai_server();
    std::env::set_var("OPENAI_API_KEY", "ENVKEY");
    let out = run(json!({"ai_provider": "api", "ai_api_type": "openai", "ai_base_url": format!("{}/v1/chat/completions", srv.base)}));
    std::env::remove_var("OPENAI_API_KEY");
    assert!(out.is_ok());
    assert_eq!(srv.requests()[0].header("authorization"), Some("Bearer ENVKEY"));
}

#[test]
fn missing_key_is_a_clear_error() {
    let _home = Home::new();
    let err = run(json!({"ai_provider": "api", "ai_api_type": "openai"})).unwrap_err();
    assert!(err.contains("OPENAI_API_KEY"), "{err}");
}

#[test]
fn http_error_never_leaks_response_body() {
    let srv = ai_server();
    let err = run(json!({"ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K", "ai_model": "m",
                         "ai_base_url": format!("{}/falha", srv.base)}))
    .unwrap_err();
    assert!(err.contains("401") && err.contains("token inválido"), "{err}");
    assert!(!err.contains("SEGREDO"));
}

#[test]
fn network_error_message_has_no_url_or_key() {
    let err = run(json!({"ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K-SECRETA", "ai_model": "m",
                         "ai_base_url": "http://127.0.0.1:1/x"}))
    .unwrap_err();
    assert!(err.contains("Erro de rede") && !err.contains("K-SECRETA") && !err.contains("127.0.0.1"), "{err}");
}

#[cfg(windows)]
fn echo_cmd(script: &str) -> Value {
    // powershell lê o stdin e escreve uma linha longa o bastante
    json!(["powershell", "-NoProfile", "-Command", script])
}

#[cfg(windows)]
#[test]
fn cli_receives_prompt_on_stdin() {
    let script = "$d = [Console]::In.ReadToEnd(); Write-Output ('CLI recebeu ' + $d.Length + ' caracteres; sala: ' + $d.Contains('F105'))";
    let out = run(json!({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": echo_cmd(script)})).unwrap();
    assert!(out.contains("sala: True"), "{out}");
}

#[test]
fn cli_command_parsing_presets_and_errors() {
    let cfg = |v: Value| json_map(v);
    assert_eq!(resolve_cli_command(&cfg(json!({"ai_cli": "custom", "ai_cli_command": "meu-cli --flag \"com espaço\""}))).unwrap()[..2], ["meu-cli", "--flag"]);
    assert_eq!(resolve_cli_command(&cfg(json!({"ai_cli": "custom", "ai_cli_command": "meu-cli --flag \"com espaço\""}))).unwrap()[2], "com espaço");
    assert_eq!(resolve_cli_command(&cfg(json!({"ai_cli": "claude"}))).unwrap(), ["claude", "-p"]);
    assert_eq!(resolve_cli_command(&cfg(json!({"ai_cli": "codex"}))).unwrap()[..2], ["codex", "exec"]);
    assert!(resolve_cli_command(&cfg(json!({"ai_cli": "inexistente"}))).unwrap_err().contains("desconhecida"));
    assert!(resolve_cli_command(&cfg(json!({"ai_cli": "custom", "ai_cli_command": ""}))).unwrap_err().contains("vazio"));
    assert!(run(json!({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": ["comando-que-nao-existe-xyz"]}))
        .unwrap_err()
        .contains("não encontrado"));
}

#[cfg(windows)]
#[test]
fn cli_failure_and_short_answer_are_reported() {
    let err = run(json!({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": echo_cmd("[Console]::Error.WriteLine('x'); exit 3")})).unwrap_err();
    assert!(err.contains("erro") && err.contains("3"), "{err}");
    let short = run(json!({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": echo_cmd("Write-Output ok")})).unwrap_err();
    assert!(short.contains("curta"), "{short}");
}

#[test]
fn describe_modes() {
    let d = |v: Value| describe(&json_map(v));
    assert!(d(json!({})).contains("Regras"));
    assert!(d(json!({"summary_mode": "ai", "ai_provider": "cli", "ai_cli": "claude"})).contains("claude"));
    assert!(d(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"})).contains("openai"));
}
