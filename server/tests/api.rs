mod common;
use common::*;
use isep_assistant::api::{handle, security, ApiServer, App, Req, Resp};
use isep_assistant::config::{load_config, save_config};
use isep_assistant::digest::Deps;
use isep_assistant::history;
use isep_assistant::http::HttpClient;
use isep_assistant::model::Email;
use isep_assistant::system;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const TOKEN: &str = "token-de-teste";

fn sid() -> String {
    format!("05{}", "ef".repeat(32))
}

fn base_config() {
    // moodle_download_dir dentro da raiz temporária: os testes nunca escrevem na pasta Downloads do utilizador
    let downloads = isep_assistant::paths::root().join("downloads");
    save_config(&json_map(json!({"session_recipients": [sid()], "session_mnemonic": "segredo", "scheduled_time": "18:00",
        "email_address": "n@isep.ipp.pt", "moodle_download_dir": downloads.to_string_lossy()}))).unwrap();
}

fn app_with(deps: impl Fn() -> Deps + Send + Sync + 'static) -> App {
    let mut app = App::new(TOKEN);
    app.deps = Arc::new(deps);
    app
}

fn quiet_deps() -> Deps {
    Deps {
        get_emails: Box::new(|_, _| Ok(vec![Email::new("Aula de ESINF", "docente@isep.ipp.pt", "Sala F105 às 10h.")])),
        send: Box::new(|_, _, _| true),
        ensure_moodle_synced: Box::new(|_, _| {}),
    }
}

fn app() -> App {
    app_with(quiet_deps)
}

fn call(app: &App, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> Resp {
    let (p, query) = match path.split_once('?') {
        Some((p, q)) => (p, q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.replace("%20", " "))).collect()),
        None => (path, vec![]),
    };
    handle(
        app,
        &Req {
            method: method.into(),
            path: format!("/api/v1{p}"),
            query,
            host: Some("127.0.0.1:8765".into()),
            authorization: token.map(|t| format!("Bearer {t}")),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
        },
    )
}

fn get(app: &App, path: &str) -> Resp {
    call(app, "GET", path, Some(TOKEN), None)
}

fn wait_job(app: &App, id: &str) -> Value {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        let r = get(app, &format!("/jobs/{id}"));
        if r.body["status"] != "running" {
            return r.body;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("a tarefa não terminou");
}

// ---------------------------------------------------------------- segurança

#[test]
fn health_is_public_but_everything_else_needs_the_token() {
    let _home = Home::new();
    let a = app();
    assert_eq!(call(&a, "GET", "/health", None, None).body["ok"], true);
    for path in ["/status", "/settings", "/history", "/jobs", "/moodle/new-files", "/logs/notifier", "/jobs/x", "/history/x"] {
        assert_eq!(call(&a, "GET", path, None, None).status, 401, "{path}");
        assert_eq!(call(&a, "GET", path, Some("errado"), None).status, 401, "{path}");
    }
    for path in ["/digest/run", "/digest/preview", "/moodle/sync", "/summary/test", "/schedule/register"] {
        assert_eq!(call(&a, "POST", path, None, None).status, 401, "{path}");
    }
    assert_eq!(call(&a, "PUT", "/settings", None, Some(json!({}))).status, 401);
    assert_eq!(call(&a, "PUT", "/moodle/credentials", None, Some(json!({}))).status, 401);
}

#[test]
fn foreign_host_header_is_rejected_before_anything_else() {
    let _home = Home::new();
    let a = app();
    let mut req = Req { method: "GET".into(), path: "/api/v1/health".into(), query: vec![], host: Some("evil.example.com".into()), authorization: None, body: vec![] };
    assert_eq!(handle(&a, &req).status, 403);
    req.host = None;
    assert_eq!(handle(&a, &req).status, 403);
}

#[test]
fn host_allowlist_and_token_helpers() {
    for ok in ["127.0.0.1:8765", "localhost:1", "localhost", "[::1]:9", "LOCALHOST:5"] {
        assert!(security::host_allowed(Some(ok)), "{ok}");
    }
    for bad in ["evil.com", "127.0.0.1.evil.com:80", "", "0.0.0.0:80", "localhost.evil.com"] {
        assert!(!security::host_allowed(Some(bad)), "{bad}");
    }
    assert!(!security::host_allowed(None));
    assert!(security::token_matches("abc", "abc") && !security::token_matches("abc", "abd") && !security::token_matches("abc", "abcd"));
    assert_eq!(security::bearer(Some("Bearer xyz")), "xyz");
    assert_eq!(security::bearer(Some("bearer xyz")), "xyz");
    assert_eq!(security::bearer(Some("Basic xyz")), "");
    assert_ne!(security::new_token(), security::new_token());
    assert_eq!(security::new_token().len(), 64);
}

#[test]
fn unknown_routes_and_wrong_methods() {
    let _home = Home::new();
    let a = app();
    assert_eq!(call(&a, "GET", "/nada", Some(TOKEN), None).status, 404);
    assert_eq!(call(&a, "GET", "/logs/../config", Some(TOKEN), None).status, 404);
    assert_eq!(call(&a, "DELETE", "/settings", Some(TOKEN), None).status, 405);
    assert_eq!(call(&a, "GET", "/digest/run", Some(TOKEN), None).status, 405);
    let outside = handle(&a, &Req { method: "GET".into(), path: "/docs".into(), query: vec![], host: Some("127.0.0.1:1".into()), authorization: None, body: vec![] });
    assert_eq!(outside.status, 404);
}

// ---------------------------------------------------------------- estado

#[test]
fn status_reports_checks_next_run_and_mode_and_never_secrets() {
    let _home = Home::new();
    base_config();
    let r = get(&app(), "/status");
    assert_eq!(r.status, 200);
    let checks: std::collections::HashMap<String, Value> = r.body["checks"].as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap().to_string(), c.clone())).collect();
    for id in ["config", "session", "thunderbird", "node", "moodle", "tasks", "internet", "ai"] {
        assert!(checks.contains_key(id), "falta a verificação {id}");
    }
    assert_eq!(checks["session"]["ok"], true);
    assert_eq!(checks["moodle"]["ok"], false);
    assert_eq!(checks["moodle"]["required"], false);
    assert_eq!(checks["tasks"]["ok"], true);
    assert_eq!(r.body["digest"]["next_run"], "2026-10-27T18:00:00");
    assert_eq!(r.body["digest"]["scheduled_time"], "18:00");
    assert_eq!(r.body["summary_mode"]["mode"], "rules");
    assert!(r.body["active_job"].is_null());
    assert!(!r.body.to_string().contains("segredo"));
}

#[test]
fn status_flags_missing_pieces() {
    let _home = Home::new();
    save_config(&json_map(json!({}))).unwrap();
    let r = get(&app(), "/status");
    let failing: Vec<&str> = r.body["checks"].as_array().unwrap().iter().filter(|c| c["ok"] == false && c["required"] == true).map(|c| c["id"].as_str().unwrap()).collect();
    assert!(failing.contains(&"config") && failing.contains(&"session"), "{failing:?}");
    assert_eq!(r.body["ok"], false);
}

#[test]
fn status_reports_ai_readiness() {
    let _home = Home::new();
    let ai = |cfg: Value| {
        save_config(&json_map(cfg)).unwrap();
        let r = get(&app(), "/status");
        r.body["checks"].as_array().unwrap().iter().find(|c| c["id"] == "ai").unwrap().clone()
    };
    let no_key = ai(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}));
    assert_eq!(no_key["ok"], false);
    assert!(no_key["detail"].as_str().unwrap().contains("OPENAI_API_KEY"));
    assert_eq!(ai(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai", "ai_api_key": "k"}))["ok"], true);
    assert_eq!(ai(json!({"summary_mode": "ai", "ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": ["comando-inexistente-xyz"]}))["ok"], false);
}

// ---------------------------------------------------------------- definições

#[test]
fn get_settings_is_masked() {
    let _home = Home::new();
    base_config();
    let r = get(&app(), "/settings");
    assert_eq!(r.body["destination"]["session_configured"], true);
    assert!(!r.body.to_string().contains(&sid()) && !r.body.to_string().contains("segredo"));
}

#[test]
fn put_settings_persists_and_never_echoes_the_api_key() {
    let _home = Home::new();
    base_config();
    let a = app();
    let r = call(&a, "PUT", "/settings", Some(TOKEN), Some(json!({"summary": {
        "mode": "ai", "ai_provider": "api", "ai_api_type": "anthropic", "ai_model": "claude-haiku-5-5", "ai_api_key": "sk-ant-segredo"}})));
    assert_eq!(r.status, 200);
    assert!(!r.body.to_string().contains("sk-ant-segredo"));
    let s = &r.body["settings"]["summary"];
    assert_eq!((s["mode"].as_str(), s["ai_api_key_set"].as_bool(), s["ai_model"].as_str()), (Some("ai"), Some(true), Some("claude-haiku-5-5")));
    let saved = load_config();
    assert_eq!(saved["ai_api_key"], "sk-ant-segredo");
    assert_eq!(saved["session_mnemonic"], "segredo", "o resto ficou intacto");
    assert!(!get(&a, "/settings").body.to_string().contains("sk-ant-segredo"));
    let cleared = call(&a, "PUT", "/settings", Some(TOKEN), Some(json!({"summary": {"clear_ai_api_key": true}})));
    assert_eq!(cleared.body["settings"]["summary"]["ai_api_key_set"], false);
}

#[test]
fn put_settings_validation_errors_are_422_with_fields_and_nothing_is_saved() {
    let _home = Home::new();
    base_config();
    let r = call(&app(), "PUT", "/settings", Some(TOKEN), Some(json!({"summary": {"mode": "x"}, "schedule": {"time": "99:99"}})));
    assert_eq!(r.status, 422);
    let mut keys: Vec<&str> = r.body["errors"].as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["schedule.time", "summary.mode"]);
    assert!(load_config().get("summary_mode").is_none());
    assert_eq!(call(&app(), "PUT", "/settings", Some(TOKEN), Some(json!([1, 2]))).status, 422);
}

#[test]
fn changing_the_time_reregisters_the_windows_tasks() {
    let _home = Home::new();
    base_config();
    system::FAKE_REGISTRATIONS.lock().unwrap().clear();
    let a = app();
    let r = call(&a, "PUT", "/settings", Some(TOKEN), Some(json!({"schedule": {"time": "9:30"}})));
    assert_eq!(r.body["effects"], json!(["reschedule"]));
    assert_eq!(r.body["reschedule"]["ok"], true);
    call(&a, "PUT", "/settings", Some(TOKEN), Some(json!({"schedule": {"time": "09:30"}})));
    assert_eq!(*system::FAKE_REGISTRATIONS.lock().unwrap(), vec!["09:30".to_string()], "sem alteração, sem novo registo");
    let reg = call(&a, "POST", "/schedule/register", Some(TOKEN), None);
    assert_eq!(reg.body["ok"], true);
    assert_eq!(system::FAKE_REGISTRATIONS.lock().unwrap().last().unwrap(), "09:30");
}

#[test]
fn summary_test_uses_current_mode_and_reports_fallback() {
    let _home = Home::new();
    base_config();
    let a = app();
    let ok = call(&a, "POST", "/summary/test", Some(TOKEN), None).body;
    assert!(ok["ok"] == true && ok["ai_requested"] == false && ok["text"].as_str().unwrap().starts_with("📬 RESUMO ISEP"));
    save_config(&json_map(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}))).unwrap();
    let bad = call(&a, "POST", "/summary/test", Some(TOKEN), None).body;
    assert!(bad["ai_requested"] == true && bad["ai_fallback"] == true && bad["ok"] == false);
    assert!(bad["text"].as_str().unwrap().contains("IA indisponível"));
}

fn tiny_moodle() -> FakeServer {
    fake_server(|s| {
        let path = s.url.split('?').next().unwrap().to_string();
        match (s.method.as_str(), path.as_str()) {
            ("POST", "/login/index.php") if s.body.contains("password=boa") => Reply::text(200, r#"<script>{"sesskey":"k1"}</script>"#),
            ("POST", "/login/index.php") => Reply::text(200, r#"<div class="divErro">x</div>"#),
            ("POST", "/lib/ajax/service.php") => Reply::json(200, json!([{"data": {"courses": [{"id": 5, "fullname": "Disciplina Teste"}]}}])),
            ("GET", "/course/view.php") => Reply::text(200, r#"<li class="section main"><a href="/mod/page/view.php?id=1">Enunciado</a></li>"#),
            ("GET", "/mod/page/view.php") => Reply::text(200, r#"<div role="main"><p>Texto do enunciado</p></div>"#),
            _ => Reply::text(200, "<html></html>"),
        }
    })
}

#[cfg(windows)]
#[test]
fn moodle_credentials_validate_login_and_store_encrypted() {
    let home = Home::new();
    base_config();
    let m = tiny_moodle();
    std::env::set_var("ISEP_MOODLE_BASE_URL", &m.base);
    let a = app();
    let bad = call(&a, "PUT", "/moodle/credentials", Some(TOKEN), Some(json!({"username": "1234567@isep.ipp.pt", "password": "ma"})));
    assert_eq!(bad.status, 400);
    assert!(!home.path("MoodleSync/config.json").exists());
    let good = call(&a, "PUT", "/moodle/credentials", Some(TOKEN), Some(json!({"username": "1234567@isep.ipp.pt", "password": "boa"})));
    std::env::remove_var("ISEP_MOODLE_BASE_URL");
    assert_eq!(good.body, json!({"ok": true, "username": "1234567"}));
    let stored = std::fs::read_to_string(home.path("MoodleSync/config.json")).unwrap();
    assert!(stored.contains("\"username\": \"1234567\"") && !stored.contains("\"boa\""));
    let status = get(&a, "/status").body;
    assert_eq!(status["moodle"]["configured"], true);
    assert_eq!(status["moodle"]["username"], "1234567");
    assert_eq!(call(&a, "PUT", "/moodle/credentials", Some(TOKEN), Some(json!({"username": "x"}))).status, 422);
}

// ---------------------------------------------------------------- tarefas

#[test]
fn digest_preview_runs_in_background_and_returns_the_summary() {
    let _home = Home::new();
    base_config();
    let a = app();
    let r = call(&a, "POST", "/digest/preview", Some(TOKEN), None);
    assert_eq!(r.status, 202);
    let job = wait_job(&a, r.body["id"].as_str().unwrap());
    assert_eq!(job["status"], "done");
    assert!(job["result"]["summary"].as_str().unwrap().contains("Aula de ESINF"));
    assert_eq!(job["result"]["preview"], true);
    assert!(job["progress"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("PRÉ-VISUALIZAÇÃO")));
    assert!(history::last("digest", false).is_none(), "a pré-visualização não entra no histórico");
}

#[test]
fn digest_run_sends_for_real_and_is_recorded() {
    let _home = Home::new();
    base_config();
    let sent = Arc::new(Mutex::new(Vec::<String>::new()));
    let s2 = sent.clone();
    let a = app_with(move || {
        let s3 = s2.clone();
        Deps { send: Box::new(move |t, _, _| { s3.lock().unwrap().push(t.to_string()); true }), ..quiet_deps() }
    });
    let id = call(&a, "POST", "/digest/run", Some(TOKEN), None).body["id"].as_str().unwrap().to_string();
    let job = wait_job(&a, &id);
    assert_eq!(job["status"], "done");
    assert_eq!(job["result"]["sent"], true);
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(history::last("digest", false).unwrap()["trigger"], "api");
}

#[test]
fn a_panicking_job_is_reported_as_failed_and_the_server_survives() {
    let _home = Home::new();
    base_config();
    let a = app_with(|| Deps { get_emails: Box::new(|_, _| panic!("explodiu")), ..quiet_deps() });
    let id = call(&a, "POST", "/digest/run", Some(TOKEN), None).body["id"].as_str().unwrap().to_string();
    let job = wait_job(&a, &id);
    assert_eq!(job["status"], "failed");
    assert!(job["error"].as_str().unwrap().contains("Panic"));
    assert_eq!(call(&a, "GET", "/health", None, None).status, 200);
    // e a seguinte tarefa já pode arrancar
    assert_eq!(call(&a, "POST", "/digest/preview", Some(TOKEN), None).status, 202);
}

#[test]
fn only_one_job_at_a_time_returns_409() {
    let _home = Home::new();
    base_config();
    let (tx, rx) = mpsc::channel::<()>();
    let rx = Arc::new(Mutex::new(rx));
    let a = app_with(move || {
        let rx = rx.clone();
        Deps {
            get_emails: Box::new(move |_, _| {
                let _ = rx.lock().unwrap().recv_timeout(Duration::from_secs(10));
                Ok(vec![])
            }),
            ..quiet_deps()
        }
    });
    let first = call(&a, "POST", "/digest/run", Some(TOKEN), None);
    let second = call(&a, "POST", "/digest/preview", Some(TOKEN), None);
    assert_eq!(first.status, 202);
    assert_eq!(second.status, 409);
    assert_eq!(second.body["active_job_id"], first.body["id"]);
    assert_eq!(get(&a, "/status").body["active_job"]["id"], first.body["id"]);
    tx.send(()).unwrap();
    wait_job(&a, first.body["id"].as_str().unwrap());
    assert_eq!(call(&a, "POST", "/digest/preview", Some(TOKEN), None).status, 202);
}

#[cfg(windows)]
#[test]
fn moodle_sync_job_uses_the_stored_credentials() {
    let home = Home::new();
    base_config();
    let m = tiny_moodle();
    std::env::set_var("ISEP_MOODLE_BASE_URL", &m.base);
    let a = app();
    assert_eq!(call(&a, "PUT", "/moodle/credentials", Some(TOKEN), Some(json!({"username": "1234567", "password": "boa"}))).status, 200);
    let id = call(&a, "POST", "/moodle/sync", Some(TOKEN), None).body["id"].as_str().unwrap().to_string();
    let job = wait_job(&a, &id);
    std::env::remove_var("ISEP_MOODLE_BASE_URL");
    assert_eq!(job["status"], "done");
    assert_eq!(job["result"]["ok"], true, "{job}");
    assert!(job["result"]["new_files"].as_u64().unwrap() >= 1);
    assert!(job["progress"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("Autenticação com sucesso")), "{job}");
    assert_eq!(history::last("moodle_sync", false).unwrap()["trigger"], "api");
    let files = get(&a, "/moodle/new-files?limit=10").body["items"].as_array().unwrap().clone();
    assert!(files.iter().any(|f| f["name"] == "Enunciado.txt"));
    let _ = home;
}

#[test]
fn job_progress_can_be_polled_incrementally_and_unknown_jobs_are_404() {
    let _home = Home::new();
    base_config();
    let a = app();
    let id = call(&a, "POST", "/digest/run", Some(TOKEN), None).body["id"].as_str().unwrap().to_string();
    let job = wait_job(&a, &id);
    let total = job["progress_total"].as_u64().unwrap() as usize;
    assert!(total >= 4);
    let tail = get(&a, &format!("/jobs/{id}?since_line={}", total - 2)).body;
    assert_eq!(tail["progress"].as_array().unwrap().len(), 2);
    assert_eq!(tail["progress_total"], total);
    assert_eq!(get(&a, "/jobs/naoexiste").status, 404);
    assert_eq!(get(&a, "/jobs").body["jobs"][0]["id"], id.as_str());
    assert_eq!(get(&a, "/jobs?limit=0").status, 422);
}

// ---------------------------------------------------------------- histórico, ficheiros novos, logs

#[test]
fn history_listing_filtering_and_detail() {
    let _home = Home::new();
    let a = app();
    let rec = history::new_record("digest", "scheduled", true, "2026-10-26T18:00:00", None, Some("O RESUMO COMPLETO"), None);
    history::append(&rec).unwrap();
    history::append(&history::new_record("moodle_sync", "manual", false, "2026-10-26T18:10:00", None, None, Some("DNS"))).unwrap();
    let items = get(&a, "/history").body["items"].as_array().unwrap().clone();
    assert_eq!(items.iter().map(|i| i["kind"].as_str().unwrap()).collect::<Vec<_>>(), ["moodle_sync", "digest"]);
    assert!(items.iter().all(|i| i["summary"].is_null()));
    let only = get(&a, "/history?kind=digest").body["items"].as_array().unwrap().clone();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0]["has_summary"], true);
    assert_eq!(get(&a, &format!("/history/{}", rec["id"].as_str().unwrap())).body["summary"], "O RESUMO COMPLETO");
    assert_eq!(get(&a, "/history/xxxx").status, 404);
    assert_eq!(get(&a, "/history?kind=outro").status, 422);
    assert_eq!(get(&a, "/history?limit=0").status, 422);
    assert_eq!(get(&a, "/history?limit=201").status, 422);
}

#[test]
fn new_files_endpoint_lists_latest_first() {
    let home = Home::new();
    std::fs::write(home.path("MoodleSync/config.json"), "{}").unwrap();
    let lines: Vec<String> = (1..=3)
        .map(|i| json!({"ts": format!("2026-10-2{i}T10:00:00"), "kind": "novo", "course": "C", "section": "S", "name": format!("f{i}.pdf"), "path": format!("f{i}.pdf"), "size": 1}).to_string())
        .collect();
    std::fs::write(home.path("MoodleSync/new_files.jsonl"), lines.join("\n") + "\n").unwrap();
    let items = get(&app(), "/moodle/new-files?limit=2").body["items"].as_array().unwrap().clone();
    assert_eq!(items.iter().map(|i| i["name"].as_str().unwrap()).collect::<Vec<_>>(), ["f3.pdf", "f2.pdf"]);
}

#[test]
fn logs_tail_and_unknown_name() {
    let home = Home::new();
    let text: Vec<String> = (0..50).map(|i| format!("linha {i}")).collect();
    std::fs::write(home.path("EmailNotifier/notifier.log"), text.join("\n")).unwrap();
    let a = app();
    assert_eq!(get(&a, "/logs/notifier?lines=3").body["lines"], json!(["linha 47", "linha 48", "linha 49"]));
    assert_eq!(get(&a, "/logs/sync").body["lines"], json!([]));
    assert_eq!(get(&a, "/logs/passwords").status, 404);
}

// ---------------------------------------------------------------- servidor real

fn raw_request(port: u16, host: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn real_server_end_to_end_with_auth_host_check_and_no_cors() {
    let _home = Home::new();
    base_config();
    let server = ApiServer::bind(0, app()).unwrap();
    let port = server.port;
    let handle = server.server.clone();
    let runner = std::thread::spawn(move || server.serve());

    let client = HttpClient::new(Duration::from_secs(10)).unwrap();
    let base = format!("http://127.0.0.1:{port}/api/v1");
    assert_eq!(client.get(&format!("{base}/health"), &[]).unwrap().status, 200);
    assert_eq!(client.get(&format!("{base}/status"), &[]).unwrap().status, 401);
    let auth = format!("Bearer {TOKEN}");
    let status = client.get(&format!("{base}/status"), &[("Authorization", &auth), ("Origin", "https://evil.example")]).unwrap();
    assert_eq!(status.status, 200);
    assert!(status.header("access-control-allow-origin").is_none(), "sem CORS");
    assert_eq!(status.header("content-type"), Some("application/json"));
    let put = client.request("PUT", &format!("{base}/settings"), &[("Authorization", &auth), ("Content-Type", "application/json")], Some(br#"{"summary":{"mode":"x"}}"#)).unwrap();
    assert_eq!(put.status, 422);

    let evil = raw_request(port, "evil.example.com", "/api/v1/health");
    assert!(evil.starts_with("HTTP/1.1 403"), "{evil}");
    let ok = raw_request(port, &format!("localhost:{port}"), "/api/v1/health");
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");

    handle.unblock();
    runner.join().unwrap();
}
