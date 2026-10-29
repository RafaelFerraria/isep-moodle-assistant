mod common;
use chrono::{Duration as Dur, Local};
use common::*;
use isep_assistant::api::{handle, App, Req, Resp};
use isep_assistant::config::{load_config, save_config};
use isep_assistant::history;
use isep_assistant::progress::{Progress, State};
use isep_assistant::views;
use serde_json::{json, Value};
use std::path::Path;

const TOKEN: &str = "token-de-teste";

fn sid() -> String {
    format!("05{}", "ab".repeat(32))
}

fn call(method: &str, path: &str, body: Option<Value>) -> Resp {
    call_on(&App::new(TOKEN), method, path, body)
}

fn call_on(app: &App, method: &str, path: &str, body: Option<Value>) -> Resp {
    let (p, query) = match path.split_once('?') {
        Some((p, q)) => (p, q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        None => (path, vec![]),
    };
    handle(
        app,
        &Req {
            method: method.into(),
            path: format!("/api/v1{p}"),
            query,
            host: Some("127.0.0.1:1".into()),
            authorization: Some(format!("Bearer {TOKEN}")),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
        },
    )
}

fn iso(offset_days: i64) -> String {
    (Local::now().naive_local() + Dur::days(offset_days)).format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn digest(offset_days: i64, summary: &str, emails: u64, files: u64, ok: bool) {
    let mut rec = history::new_record("digest", "scheduled", ok, &iso(offset_days), Some(json!({"emails_new": emails, "moodle_files": files, "sent": ok})), Some(summary), None);
    rec["started"] = json!(iso(offset_days));
    history::append(&rec).unwrap();
}

// ---------------------------------------------------------------- resumo (página principal)

#[test]
fn overview_highlights_today_and_totals_the_period() {
    let _home = Home::new();
    digest(-20, "resumo antigo", 4, 1, true);
    digest(-3, "resumo de há 3 dias", 5, 2, true);
    digest(0, "resumo de hoje", 3, 0, true);
    digest(0, "", 0, 0, false);

    let r = call("GET", "/overview?days=7", None);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["today"]["summary"], "resumo de hoje");
    assert_eq!(r.body["totals"]["runs"], 3, "o de há 20 dias fica de fora dos 7 dias");
    assert_eq!(r.body["totals"]["emails"], 8);
    assert_eq!(r.body["totals"]["moodle_files"], 2);
    assert_eq!(r.body["totals"]["failed"], 1);

    let r30 = call("GET", "/overview?days=30", None);
    assert_eq!(r30.body["totals"]["runs"], 4);
    assert_eq!(r30.body["items"].as_array().unwrap().len(), 4);
}

#[test]
fn overview_without_a_digest_today_falls_back_to_the_latest() {
    let _home = Home::new();
    digest(-2, "o último", 1, 0, true);
    let r = call("GET", "/overview", None);
    assert!(r.body["today"].is_null());
    assert_eq!(r.body["latest"]["summary"], "o último");
    assert_eq!(call("GET", "/overview?days=31", None).status, 422);
    assert_eq!(call("GET", "/overview?days=0", None).status, 422);
}

// ---------------------------------------------------------------- emails

#[test]
fn emails_page_reads_the_configured_source_and_marks_sent_ones() {
    let home = Home::new();
    let profiles = home.path("Profiles");
    let inbox = profiles.join("x.default/ImapMail/h/INBOX");
    std::fs::create_dir_all(inbox.parent().unwrap()).unwrap();
    let today = Local::now().format("%a, %d %b %Y 08:00:00 +0000").to_string();
    std::fs::write(&inbox, format!("From - x\nSubject: Aula de AC\nFrom: docente@isep.ipp.pt\nDate: {today}\nMessage-ID: <m1@isep>\n\nSala F105 às 10h.\n")).unwrap();
    std::env::set_var("ISEP_THUNDERBIRD_PROFILES", &profiles);
    save_config(&json_map(json!({"email_method": "thunderbird"}))).unwrap();

    let r = call("GET", "/emails?days=3", None);
    std::env::remove_var("ISEP_THUNDERBIRD_PROFILES");
    assert_eq!(r.status, 200);
    assert_eq!(r.body["source"], "thunderbird");
    assert_eq!(r.body["error"], Value::Null);
    assert_eq!(r.body["items"][0]["subject"], "Aula de AC");
    assert!(r.body["items"][0]["preview"].as_str().unwrap().contains("Sala F105"));
    assert_eq!(r.body["items"][0]["in_digest"], false);
}

#[test]
fn emails_page_reports_a_source_that_is_not_ready_instead_of_failing() {
    let _home = Home::new();
    save_config(&json_map(json!({"email_method": "graph"}))).unwrap();
    let r = call("GET", "/emails", None);
    assert_eq!(r.status, 200);
    assert!(r.body["error"].as_str().unwrap().contains("ainda não foi ligada"), "{}", r.body);
    save_config(&json_map(json!({"email_method": "imap"}))).unwrap();
    assert!(call("GET", "/emails", None).body["error"].as_str().unwrap().contains("imap"));
}

#[cfg(windows)]
#[test]
fn microsoft_account_connects_with_a_device_code_and_the_token_is_stored_encrypted() {
    let _home = Home::new();
    let login = fake_server(|req| {
        let form = isep_assistant::http::decode_component(&req.body);
        if req.url.ends_with("/devicecode") {
            Reply::json(200, json!({"device_code": "dc-1", "user_code": "ABCD-1234", "verification_uri": "https://microsoft.com/devicelogin", "interval": 5, "expires_in": 900}))
        } else if form.contains("device_code=pendente") {
            Reply::json(400, json!({"error": "authorization_pending"}))
        } else if form.contains("device_code=dc-1") {
            Reply::json(200, json!({"access_token": "at", "refresh_token": "rt-secreto"}))
        } else {
            Reply::json(400, json!({"error": "bad_verification_code"}))
        }
    });
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.base);
    save_config(&json_map(json!({"email_method": "thunderbird"}))).unwrap();

    let start = call("POST", "/email/connect/start", None);
    assert_eq!(start.status, 200, "{}", start.body);
    assert_eq!(start.body["user_code"], "ABCD-1234");

    let pending = call("POST", "/email/connect/poll", Some(json!({"device_code": "pendente"})));
    assert_eq!(pending.body["status"], "pending");
    let unknown = call("POST", "/email/connect/poll", Some(json!({"device_code": "desconhecido"})));
    assert_eq!(unknown.body["status"], "failed", "um código desconhecido não fica à espera para sempre");

    let done = call("POST", "/email/connect/poll", Some(json!({"device_code": "dc-1"})));
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    assert_eq!(done.body["status"], "done", "{}", done.body);
    let cfg = load_config();
    assert_eq!(cfg["email_method"], "graph");
    let stored = cfg["ms_refresh_token"].as_str().unwrap();
    assert!(!stored.contains("rt-secreto") && stored.chars().all(|c| c.is_ascii_hexdigit()), "o token tem de ir cifrado");
    let public = call("GET", "/settings", None).body;
    assert_eq!(public["email"]["connected"], true);
    assert!(!public.to_string().contains("rt-secreto"));
}

#[cfg(windows)]
#[test]
fn graph_source_reads_the_inbox_and_keeps_a_rotated_refresh_token() {
    let _home = Home::new();
    let login = fake_server(|_| Reply::json(200, json!({"access_token": "at-1", "refresh_token": "rt-novo"})));
    let graph = fake_server(|req| {
        assert!(req.header("authorization") == Some("Bearer at-1"), "{:?}", req.headers);
        Reply::json(200, json!({"value": [
            {"internetMessageId": "<g1@isep>", "subject": "Prazo do PI", "from": {"emailAddress": {"name": "Docente", "address": "d@isep.ipp.pt"}},
             "receivedDateTime": "2026-10-27T09:30:00Z", "bodyPreview": "curto", "body": {"content": "Entrega até sexta."}},
            {"internetMessageId": "<g2@isep>", "subject": "", "from": {"emailAddress": {"address": "x@isep.ipp.pt"}},
             "receivedDateTime": "2026-10-27T08:00:00Z", "bodyPreview": "só a pré-visualização"}
        ]}))
    });
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.base);
    std::env::set_var("ISEP_GRAPH_URL", &graph.base);
    let mut cfg = json_map(json!({"email_method": "graph"}));
    isep_assistant::graph::store_refresh_token(&mut cfg, "rt-velho").unwrap();
    save_config(&cfg).unwrap();

    let source = isep_assistant::mail::source_for("outlook").unwrap();
    assert_eq!(source.id(), "graph", "«outlook» é o nome antigo do mesmo serviço");
    let since = (Local::now() - Dur::days(2)).naive_local();
    let emails = source.fetch(&load_config(), since, isep_assistant::mail::FetchOptions { sync: false });
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    std::env::remove_var("ISEP_GRAPH_URL");
    let emails = emails.unwrap();
    assert_eq!(emails.len(), 2);
    assert_eq!(emails[0].subject, "Prazo do PI");
    assert_eq!(emails[0].sender, "\"Docente\" <d@isep.ipp.pt>");
    assert_eq!(emails[0].message_id, "<g1@isep>");
    assert_eq!(emails[0].body, "Entrega até sexta.");
    assert_eq!(emails[1].subject, "Sem assunto");
    assert_eq!(emails[1].body, "só a pré-visualização");
    let rotated = isep_assistant::dpapi::decrypt(load_config()["ms_refresh_token"].as_str().unwrap()).unwrap();
    assert_eq!(rotated, "rt-novo", "o token novo tem de ficar guardado");
    let url = &graph.requests()[0].url;
    assert!(url.contains("receivedDateTime") && url.contains("inbox"), "{url}");
}

// ---------------------------------------------------------------- ficheiros do Moodle

#[test]
fn moodle_files_page_shows_pending_downloading_done_and_errors_with_progress() {
    let home = Home::new();
    let dir = home.path("MoodleSync");
    let mut p = Progress::new(Some(&dir));
    p.start();
    p.set_courses(4);
    p.begin_course("Arquitetura de Computadores");
    p.queue(Path::new("a.pdf"), "AC", "01", "a.pdf");
    p.queue(Path::new("b.pdf"), "AC", "01", "b.pdf");
    p.queue(Path::new("c.pdf"), "AC", "01", "c.pdf");
    p.queue(Path::new("a.pdf"), "AC", "01", "a.pdf"); // repetido: ignorado
    p.update(Path::new("a.pdf"), "AC", "01", "a.pdf", State::Done, 2048, None);
    p.update(Path::new("b.pdf"), "AC", "01", "b.pdf", State::Downloading, 0, None);
    p.update(Path::new("d.pdf"), "AC", "02", "d.pdf", State::Error, 0, Some("HTTP 500"));
    p.end_course();
    // a sincronização escreveu o lock: está mesmo em curso
    std::fs::write(home.path("MoodleSync/sync.lock"), "1").unwrap();

    let r = call("GET", "/moodle/files", None);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["running"], true);
    assert_eq!(r.body["percent"], 25);
    assert_eq!(r.body["courses_done"], 1);
    assert_eq!(r.body["counts"], json!({"pending": 1, "downloading": 1, "done": 1, "unchanged": 0, "error": 1}));
    assert_eq!(r.body["items"].as_array().unwrap().len(), 4);
    let err = r.body["items"].as_array().unwrap().iter().find(|i| i["state"] == "error").unwrap();
    assert_eq!(err["error"], "HTTP 500");

    p.finish();
    let after = call("GET", "/moodle/files", None).body;
    assert_eq!(after["running"], false);
    assert_eq!(after["counts"]["downloading"], 0, "o que ficou a meio volta a pendente");
}

#[test]
fn a_dead_sync_does_not_stay_running_forever() {
    let home = Home::new();
    let mut p = Progress::new(Some(&home.path("MoodleSync")));
    p.start(); // sem sync.lock: o processo morreu
    assert_eq!(call("GET", "/moodle/files", None).body["running"], false);
}

#[test]
fn moodle_files_page_is_empty_but_valid_before_the_first_sync() {
    let _home = Home::new();
    let r = call("GET", "/moodle/files", None);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["running"], false);
    assert_eq!(r.body["items"], json!([]));
    assert_eq!(r.body["downloaded"], json!([]));
}

// ---------------------------------------------------------------- histórico e logs

#[test]
fn activity_keeps_only_seven_days_and_prune_drops_old_logs_and_history() {
    let home = Home::new();
    let old = (Local::now() - Dur::days(9)).format("%Y-%m-%d %H:%M:%S").to_string();
    let recent = (Local::now() - Dur::days(1)).format("%Y-%m-%d %H:%M:%S").to_string();
    let log = home.path("EmailNotifier/notifier.log");
    std::fs::write(&log, format!("[{old}] muito antigo\n  continuação antiga\n[{recent}] recente\n  continuação recente\n")).unwrap();
    digest(-1, "ontem", 1, 0, true);
    digest(-40, "há 40 dias", 1, 0, true);

    let r = call("GET", "/activity?days=7", None);
    assert_eq!(r.status, 200);
    let lines: Vec<&str> = r.body["logs"]["notifier"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
    assert_eq!(lines, vec![format!("[{recent}] recente").as_str(), "  continuação recente"]);
    assert_eq!(r.body["history"].as_array().unwrap().len(), 1);
    assert_eq!(call("GET", "/activity?days=8", None).status, 422);

    let (lines_dropped, records_dropped) = views::prune();
    assert_eq!(lines_dropped, 2);
    assert_eq!(records_dropped, 1);
    let kept = std::fs::read_to_string(&log).unwrap();
    assert!(!kept.contains("antigo") && kept.contains("continuação recente"));
    assert_eq!(history::list_records(10, 0, None, false).len(), 1);
}

// ---------------------------------------------------------------- assistente de configuração e Session

#[test]
fn setup_wizard_state_and_existing_installs_skip_it() {
    let _home = Home::new();
    let fresh = call("GET", "/settings", None).body;
    assert_eq!(fresh["setup"]["done"], false, "instalação nova começa pela configuração");
    let setup = call("GET", "/setup", None).body;
    assert_eq!(setup["ready"], false);
    let ids: Vec<&str> = setup["steps"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["email", "moodle", "session", "summary", "schedule"]);

    let r = call("PUT", "/settings", Some(json!({"setup": {"done": true}})));
    assert_eq!(r.status, 200);
    assert_eq!(call("GET", "/settings", None).body["setup"]["done"], true);
    assert_eq!(call("PUT", "/settings", Some(json!({"setup": {"done": "sim"}}))).status, 422);

    // quem já tinha o Session a funcionar não repete o assistente
    save_config(&json_map(json!({"session_recipients": [sid()], "session_mnemonic": "x"}))).unwrap();
    assert_eq!(call("GET", "/settings", None).body["setup"]["done"], true);
}

#[test]
fn email_settings_accept_only_known_methods() {
    let _home = Home::new();
    assert_eq!(call("PUT", "/settings", Some(json!({"email": {"method": "graph", "address": "1234567@isep.ipp.pt"}}))).status, 200);
    let s = call("GET", "/settings", None).body;
    assert_eq!(s["email"]["method"], "graph");
    assert_eq!(s["email"]["address"], "1234567@isep.ipp.pt");
    assert_eq!(call("PUT", "/settings", Some(json!({"email": {"method": "imap"}}))).status, 422);
    assert_eq!(call("PUT", "/settings", Some(json!({"email": {"address": "sem-arroba"}}))).status, 422);
}

#[test]
fn session_recipients_are_validated_added_and_removed() {
    let _home = Home::new();
    save_config(&json_map(json!({"session_mnemonic": "segredo", "session_bot_id": "05bot"}))).unwrap();
    let s = call("GET", "/session", None).body;
    assert_eq!(s["bot_created"], true);
    assert_eq!(s["configured"], false);
    assert_eq!(s["bot_id"], "05bot");
    assert!(!s.to_string().contains("segredo"), "a frase secreta nunca sai");

    let bad = call("POST", "/session/recipients", Some(json!({"session_id": "123"})));
    assert_eq!(bad.status, 422);
    assert!(bad.body["errors"]["session_id"].as_str().unwrap().contains("66"));

    let ok = call("POST", "/session/recipients", Some(json!({"session_id": sid()})));
    assert_eq!(ok.status, 200);
    assert_eq!(ok.body["configured"], true);
    call("POST", "/session/recipients", Some(json!({"session_id": sid()}))); // repetido: não duplica
    assert_eq!(load_config()["session_recipients"].as_array().unwrap().len(), 1);

    assert_eq!(call("DELETE", "/session/recipients/5", None).status, 404);
    assert_eq!(call("DELETE", "/session/recipients/x", None).status, 422);
    let gone = call("DELETE", "/session/recipients/0", None);
    assert_eq!(gone.status, 200);
    assert_eq!(gone.body["configured"], false);
    assert!(load_config().get("session_group_id").is_none());
}

#[test]
fn creating_the_bot_fails_clearly_when_the_sender_script_is_missing_and_keeps_an_existing_bot() {
    let _home = Home::new();
    let r = call("POST", "/session/bot", Some(json!({})));
    assert_eq!(r.status, 400);
    assert!(r.body["detail"].as_str().unwrap().contains("Node.js") || r.body["detail"].as_str().unwrap().contains("session_sender"), "{}", r.body);

    save_config(&json_map(json!({"session_mnemonic": "ja-existe", "session_bot_id": "05ja"}))).unwrap();
    let again = call("POST", "/session/bot", Some(json!({})));
    assert_eq!(again.status, 200, "sem force não troca o bot");
    assert_eq!(load_config()["session_mnemonic"], "ja-existe");
}

#[test]
fn pairing_needs_a_bot_first() {
    let _home = Home::new();
    let app = App::new(TOKEN);
    let job = call_on(&app, "POST", "/session/pair", None);
    assert_eq!(job.status, 202);
    let id = job.body["id"].as_str().unwrap().to_string();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let j = call_on(&app, "GET", &format!("/jobs/{id}"), None).body;
        if j["status"] != "running" {
            assert_eq!(j["status"], "failed");
            assert!(j["error"].as_str().unwrap().contains("bot"), "{j}");
            break;
        }
        assert!(std::time::Instant::now() < end, "a tarefa não terminou");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn scheduled_task_pointing_to_a_missing_program_is_reported() {
    let tasks = isep_assistant::system::parse_tasks_json(
        &json!({"name": "T", "state": "Ready", "last_run": null, "next_run": null, "last_result": 0, "target": "C:\\nao\\existe.bat", "target_ok": false}).to_string(),
    );
    assert_eq!(tasks[0]["target_ok"], false);
    assert_eq!(tasks[0]["target"], "C:\\nao\\existe.bat");
}

// ---------------------------------------------------------------- tudo dentro da app

#[test]
fn session_runtime_and_script_can_be_pointed_at_the_embedded_ones() {
    let home = Home::new();
    // sem nada configurado, o script é o da pasta de dados (desenvolvimento)
    std::env::remove_var("ISEP_SESSION_SCRIPT");
    std::env::remove_var("ISEP_NODE");
    assert_eq!(isep_assistant::paths::session_script(), home.path("EmailNotifier/session_sender.cjs"));

    std::env::set_var("ISEP_SESSION_SCRIPT", home.path("dentro/da/app.cjs"));
    std::env::set_var("ISEP_NODE", home.path("runtime/node.exe"));
    let script = isep_assistant::paths::session_script();
    let rt = isep_assistant::system::node_runtime().unwrap();
    std::env::remove_var("ISEP_SESSION_SCRIPT");
    std::env::remove_var("ISEP_NODE");
    assert_eq!(script, home.path("dentro/da/app.cjs"));
    assert_eq!(rt.exe, home.path("runtime/node.exe"));
    assert!(!rt.electron, "um node explícito não precisa de ELECTRON_RUN_AS_NODE");
}

#[cfg(windows)]
#[test]
fn microsoft_admin_consent_error_gets_a_clear_message_pointing_to_thunderbird() {
    let _home = Home::new();
    let login = fake_server(|req| {
        if req.url.ends_with("/devicecode") {
            Reply::json(400, json!({"error": "invalid_request", "error_description": "AADSTS65001: The user or administrator has not consented to use the application."}))
        } else {
            Reply::json(400, json!({"error": "access_denied", "error_description": "AADSTS90094: Admin consent is required."}))
        }
    });
    std::env::set_var("ISEP_MS_LOGIN_URL", &login.base);
    let start = call("POST", "/email/connect/start", None);
    let poll = call("POST", "/email/connect/poll", Some(json!({"device_code": "x"})));
    std::env::remove_var("ISEP_MS_LOGIN_URL");
    assert_eq!(start.status, 502);
    assert!(start.body["detail"].as_str().unwrap().contains("Thunderbird"), "{}", start.body);
    assert_eq!(poll.body["status"], "failed");
    assert!(poll.body["message"].as_str().unwrap().contains("administrador"), "{}", poll.body);
}
