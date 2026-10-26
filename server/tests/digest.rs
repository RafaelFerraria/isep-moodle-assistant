mod common;
use chrono::Duration;
use common::*;
use isep_assistant::config::save_config;
use isep_assistant::digest::*;
use isep_assistant::history;
use isep_assistant::model::Email;
use isep_assistant::paths;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    emails: Vec<Email>,
    sent: Vec<String>,
    send_ok: bool,
    fetch_error: Option<String>,
    since: Vec<chrono::NaiveDateTime>,
}

fn sid() -> String {
    format!("05{}", "cd".repeat(32))
}

fn setup(home: &Home) -> (Arc<Mutex<State>>, Deps) {
    let _ = home;
    let state = Arc::new(Mutex::new(State { send_ok: true, ..Default::default() }));
    let (s1, s2) = (state.clone(), state.clone());
    let deps = Deps {
        get_emails: Box::new(move |_cfg, since| {
            let mut st = s1.lock().unwrap();
            st.since.push(since);
            match &st.fetch_error {
                Some(e) => Err(e.clone()),
                None => Ok(st.emails.clone()),
            }
        }),
        send: Box::new(move |text, _cfg, _log| {
            let mut st = s2.lock().unwrap();
            st.sent.push(text.to_string());
            st.send_ok
        }),
        ensure_moodle_synced: Box::new(|_, _| {}),
    };
    save_config(&json_map(json!({"session_recipients": [sid()], "session_mnemonic": "m", "summary_mode": "rules"}))).unwrap();
    (state, deps)
}

fn mail(subject: &str, id: &str) -> Email {
    let mut e = Email::new(subject, "x@isep.ipp.pt", "Texto do aviso.");
    e.message_id = id.into();
    e
}

fn last_run() -> Value {
    serde_json::from_str(&std::fs::read_to_string(paths::last_run_file()).unwrap()).unwrap()
}

#[test]
fn sends_summary_and_records_state_and_history() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    state.lock().unwrap().emails = vec![mail("Aviso importante", "<a@x>")];
    let res = run_digest(true, "scheduled", None, None, &deps, true);
    assert!(res.ok && res.sent && res.emails_new == 1);
    assert!(state.lock().unwrap().sent[0].contains("Aviso importante"));
    assert!(last_run()["sent_ids"].as_array().unwrap().contains(&json!("<a@x>")));
    let rec = history::last("digest", false).unwrap();
    assert_eq!(rec["ok"], true);
    assert_eq!(rec["trigger"], "scheduled");
    assert_eq!(rec["details"]["emails_new"], 1);
    assert!(history::get(rec["id"].as_str().unwrap()).unwrap()["summary"].as_str().unwrap().contains("Aviso importante"));
}

#[test]
fn already_sent_emails_are_not_sent_again() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    state.lock().unwrap().emails = vec![mail("Aviso", "<a@x>")];
    run_digest(true, "manual", None, None, &deps, true);
    state.lock().unwrap().sent.clear();
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(res.skipped && res.ok);
    assert!(state.lock().unwrap().sent.is_empty());
    assert_eq!(history::last("digest", false).unwrap()["details"]["skipped"], true);
}

#[test]
fn failed_send_keeps_emails_for_the_next_run() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    state.lock().unwrap().emails = vec![mail("Aviso", "<a@x>")];
    state.lock().unwrap().send_ok = false;
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(!res.ok && res.error.is_some() && !paths::last_run_file().exists());
    state.lock().unwrap().send_ok = true;
    assert!(run_digest(true, "manual", None, None, &deps, true).sent);
}

#[test]
fn fetch_error_is_reported_and_state_is_not_advanced() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    state.lock().unwrap().fetch_error = Some("Sem ligação à Internet.".into());
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(!res.ok && res.error.as_deref().unwrap().contains("Sem ligação"));
    assert!(state.lock().unwrap().sent.is_empty() && !paths::last_run_file().exists());
    assert_eq!(history::last("digest", false).unwrap()["ok"], false);
}

#[test]
fn preview_never_sends_or_persists() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    state.lock().unwrap().emails = vec![mail("Aviso", "<a@x>")];
    let res = run_digest(false, "api", None, None, &deps, true);
    assert!(res.ok && res.preview && !res.sent && res.summary.as_deref().unwrap().contains("Aviso"));
    assert!(state.lock().unwrap().sent.is_empty() && !paths::last_run_file().exists() && history::last("digest", false).is_none());
}

#[test]
fn missing_config_gives_a_clear_error() {
    let home = Home::new();
    let (_s, deps) = setup(&home);
    std::fs::remove_file(paths::email_config()).unwrap();
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(!res.ok && res.error.as_deref().unwrap().contains("config.json"));
}

#[test]
fn search_window_overlaps_previous_run() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    update_last_run(&[], None);
    run_digest(true, "manual", None, None, &deps, true);
    let since = state.lock().unwrap().since[0];
    let expected = chrono::Local::now().naive_local() - Duration::hours(OVERLAP_HOURS);
    assert!((since - expected).num_seconds().abs() < 5);
}

#[test]
fn reads_last_run_files_written_by_the_python_version_with_microseconds() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    std::fs::write(
        home.path("EmailNotifier/last_run.json"),
        "\u{feff}{\"last_run\": \"2026-10-26T08:00:20.303484\", \"sent_ids\": [\"<old@x>\"]}",
    )
    .unwrap();
    state.lock().unwrap().emails = vec![mail("Velho", "<old@x>")];
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(res.skipped, "o id já enviado pela versão Python tem de ser reconhecido");
    let since = state.lock().unwrap().since[0];
    assert_eq!(since, parse_iso("2026-10-26T08:00:20.303484").unwrap() - Duration::hours(OVERLAP_HOURS));
}

#[test]
fn moodle_files_alone_trigger_a_summary() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    std::fs::write(home.path("MoodleSync/config.json"), "{}").unwrap();
    let ts = chrono::Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S").to_string();
    let entry = json!({"ts": ts, "kind": "novo", "course": "LAPR3", "section": "S", "name": "Enunciado.pdf", "path": "x/Enunciado.pdf", "size": 1});
    std::fs::write(home.path("MoodleSync/new_files.jsonl"), format!("{entry}\n")).unwrap();
    let old = (chrono::Local::now().naive_local() - Duration::hours(1)).format("%Y-%m-%dT%H:%M:%S").to_string();
    std::fs::write(home.path("EmailNotifier/last_run.json"), json!({"last_run": old, "last_moodle_ts": old, "sent_ids": []}).to_string()).unwrap();
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(res.sent && res.moodle_files == 1);
    assert!(state.lock().unwrap().sent[0].contains("Enunciado.pdf"));
    // a próxima execução já não repete os mesmos ficheiros
    state.lock().unwrap().sent.clear();
    assert!(run_digest(true, "manual", None, None, &deps, true).skipped);
}

#[test]
fn ai_fallback_is_reported() {
    let home = Home::new();
    let (state, deps) = setup(&home);
    save_config(&json_map(json!({"session_recipients": [sid()], "session_mnemonic": "m", "summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}))).unwrap();
    state.lock().unwrap().emails = vec![mail("Aviso", "<a@x>")];
    let res = run_digest(true, "manual", None, None, &deps, true);
    assert!(res.sent && res.ai_fallback && !res.ai_used);
    assert!(state.lock().unwrap().sent[0].contains("IA indisponível"));
    assert_eq!(history::last("digest", false).unwrap()["details"]["ai_fallback"], true);
}

#[test]
fn moodle_dir_prefers_config_then_sibling_folder() {
    let home = Home::new();
    assert!(moodle_dir_of(&Default::default()).is_none());
    std::fs::write(home.path("MoodleSync/config.json"), "{}").unwrap();
    assert_eq!(moodle_dir_of(&Default::default()).unwrap(), home.path("MoodleSync"));
    assert_eq!(moodle_dir_of(&json_map(json!({"moodle_sync_dir": "X:/outro"}))).unwrap(), std::path::PathBuf::from("X:/outro"));
}

#[test]
fn progress_callback_receives_log_lines() {
    let home = Home::new();
    let (_s, deps) = setup(&home);
    let lines = Mutex::new(Vec::<String>::new());
    let cb = |l: &str| lines.lock().unwrap().push(l.to_string());
    let res = run_digest(true, "manual", Some(&cb), None, &deps, true);
    let got = lines.lock().unwrap();
    assert!(got.iter().any(|l| l.contains("INÍCIO")) && got.iter().any(|l| l.contains("Nenhum email novo")));
    assert!(res.log.iter().any(|l| l.contains("TERMINADA")));
}

#[test]
fn last_run_keeps_at_most_1000_ids() {
    let _home = Home::new();
    let ids: Vec<String> = (0..1200).map(|i| format!("<{i}@x>")).collect();
    update_last_run(&ids, None);
    let ids_saved = last_run()["sent_ids"].as_array().unwrap().len();
    assert_eq!(ids_saved, 1000);
    assert_eq!(last_run()["sent_ids"][0], "<200@x>");
}
