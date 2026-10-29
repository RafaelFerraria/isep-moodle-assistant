mod common;
use common::*;
use isep_assistant::config::*;
use isep_assistant::history;
use serde_json::{json, Value};

const SID: &str = "05abababababababababababababababababababababababababababababababab";

fn sid() -> String {
    format!("05{}", "ab".repeat(32))
}

#[test]
fn sid_constant_has_66_chars() {
    assert_eq!(SID.len(), 66);
    assert_eq!(sid().len(), 66);
}

// ---------------------------------------------------------------- config: vista pública

#[test]
fn public_settings_never_exposes_secrets() {
    let _home = Home::new();
    let cfg = json_map(json!({
        "session_mnemonic": "palavra um dois", "ai_api_key": "sk-segredo", "email_password": "pw",
        "session_recipients": [sid()], "summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"
    }));
    let pub_ = public_settings(&cfg);
    let blob = pub_.to_string();
    for secret in ["palavra um dois", "sk-segredo", "\"pw\"", &sid()] {
        assert!(!blob.contains(secret), "vazou {secret}");
    }
    assert_eq!(pub_["summary"]["ai_api_key_set"], true);
    assert_eq!(pub_["destination"]["session_configured"], true);
    assert_eq!(pub_["destination"]["session_recipients"][0], format!("{}…{}", &sid()[..6], &sid()[62..]));
}

#[test]
fn defaults_when_config_is_empty() {
    let _home = Home::new();
    let p = public_settings(&Default::default());
    assert_eq!(p["summary"]["mode"], "rules");
    assert_eq!(p["summary"]["ai_provider"], "cli");
    assert_eq!(p["schedule"]["time"], "18:00");
    assert_eq!(p["destination"]["session_configured"], false);
    assert_eq!(p["email"]["method"], "imap");
}

#[test]
fn env_key_detection() {
    let _home = Home::new();
    std::env::set_var("ANTHROPIC_API_KEY", "x");
    let p = public_settings(&json_map(json!({"ai_api_type": "anthropic"})));
    std::env::remove_var("ANTHROPIC_API_KEY");
    assert_eq!(p["summary"]["ai_api_key_env"], "ANTHROPIC_API_KEY");
    assert_eq!(p["summary"]["ai_api_key_env_set"], true);
}

#[test]
fn recipients_accepts_string_list_and_legacy_group_id() {
    assert_eq!(recipients_of(&json_map(json!({"session_recipients": SID}))), vec![SID]);
    assert_eq!(recipients_of(&json_map(json!({"session_recipients": [SID, " "]}))), vec![SID]);
    assert_eq!(recipients_of(&json_map(json!({"session_group_id": SID}))), vec![SID]);
    assert!(recipients_of(&Default::default()).is_empty());
}

// ---------------------------------------------------------------- config: alterações

fn patch(cfg: Value, p: Value) -> Result<(serde_json::Map<String, Value>, Vec<String>), ValidationError> {
    apply_settings_patch(&json_map(cfg), &p)
}

#[test]
fn patch_switches_to_ai_cli() {
    let (new, effects) = patch(json!({}), json!({"summary": {"mode": "ai", "ai_provider": "cli", "ai_cli": "gemini"}})).unwrap();
    assert_eq!((new["summary_mode"].as_str(), new["ai_provider"].as_str(), new["ai_cli"].as_str()), (Some("ai"), Some("cli"), Some("gemini")));
    assert!(effects.is_empty());
}

#[test]
fn patch_collects_all_errors_at_once() {
    let err = patch(
        json!({}),
        json!({"summary": {"mode": "magia", "ai_cli": "x", "ai_base_url": "ftp://a"}, "schedule": {"time": "25:99"}, "destination": {"session_id": "123"}}),
    )
    .unwrap_err();
    let mut keys: Vec<&str> = err.errors.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["destination.session_id", "schedule.time", "summary.ai_base_url", "summary.ai_cli", "summary.mode"]);
}

#[test]
fn custom_cli_and_compatible_api_need_their_fields() {
    let e1 = patch(json!({}), json!({"summary": {"mode": "ai", "ai_provider": "cli", "ai_cli": "custom"}})).unwrap_err();
    assert!(e1.errors.contains_key("summary.ai_cli_command"));
    let e2 = patch(json!({}), json!({"summary": {"mode": "ai", "ai_provider": "api", "ai_api_type": "openai_compatible"}})).unwrap_err();
    assert!(e2.errors.contains_key("summary.ai_base_url") && e2.errors.contains_key("summary.ai_model"));
}

#[test]
fn schedule_normalises_time_and_flags_reschedule_only_on_change() {
    let (new, effects) = patch(json!({"scheduled_time": "18:00"}), json!({"schedule": {"time": "7:05"}})).unwrap();
    assert_eq!(new["scheduled_time"], "07:05");
    assert_eq!(effects, ["reschedule"]);
    let (_, effects2) = apply_settings_patch(&new, &json!({"schedule": {"time": "07:05"}})).unwrap();
    assert!(effects2.is_empty());
}

#[test]
fn api_key_can_be_set_and_cleared_but_not_read() {
    let (new, _) = patch(json!({}), json!({"summary": {"ai_api_key": " sk-abc "}})).unwrap();
    assert_eq!(new["ai_api_key"], "sk-abc");
    let (cleared, _) = apply_settings_patch(&new, &json!({"summary": {"clear_ai_api_key": true}})).unwrap();
    assert!(!cleared.contains_key("ai_api_key"));
    let (untouched, _) = apply_settings_patch(&new, &json!({"summary": {"mode": "rules"}})).unwrap();
    assert_eq!(untouched["ai_api_key"], "sk-abc");
}

#[test]
fn session_id_sets_both_keys_and_unknown_keys_survive() {
    let (new, _) = patch(json!({"extra": 1, "session_mnemonic": "m"}), json!({"destination": {"session_id": sid()}})).unwrap();
    assert_eq!(new["session_recipients"], json!([sid()]));
    assert_eq!(new["session_group_id"], sid());
    assert_eq!(new["extra"], 1);
    assert_eq!(new["session_mnemonic"], "m");
}

#[test]
fn patch_does_not_mutate_the_original() {
    let original = json_map(json!({"summary_mode": "rules"}));
    let _ = apply_settings_patch(&original, &json!({"summary": {"mode": "ai", "ai_provider": "cli"}})).unwrap();
    assert_eq!(Value::Object(original), json!({"summary_mode": "rules"}));
}

#[test]
fn save_and_load_roundtrip_is_atomic_and_tolerates_bom_and_garbage() {
    let home = Home::new();
    save_config(&json_map(json!({"a": "ç", "n": 1}))).unwrap();
    assert_eq!(Value::Object(load_config()), json!({"a": "ç", "n": 1}));
    assert!(!home.path("EmailNotifier/config.json.tmp").exists());

    let path = home.path("EmailNotifier/config.json");
    std::fs::write(&path, b"\xef\xbb\xbf{\"k\": 1}").unwrap();
    assert_eq!(Value::Object(load_config()), json!({"k": 1}));
    std::fs::write(&path, "{não é json").unwrap();
    assert!(load_config().is_empty());
    std::fs::remove_file(&path).unwrap();
    assert!(load_config().is_empty());
}

// ---------------------------------------------------------------- histórico

fn rec(kind: &str, ok: bool, summary: Option<&str>) -> Value {
    let r = history::new_record(kind, "manual", ok, "2026-10-26T18:00:00", None, summary, None);
    history::append(&r).unwrap();
    r
}

#[test]
fn append_and_list_newest_first_without_summary_by_default() {
    let _home = Home::new();
    let a = rec("digest", true, Some("texto A"));
    let b = rec("moodle_sync", true, None);
    let items = history::list_records(50, 0, None, false);
    assert_eq!(items[0]["id"], b["id"]);
    assert_eq!(items[1]["id"], a["id"]);
    assert!(items[1]["summary"].is_null());
    assert_eq!(items[1]["has_summary"], true);
    assert_eq!(items[0]["has_summary"], false);
    assert_eq!(history::list_records(50, 0, None, true)[1]["summary"], "texto A");
}

#[test]
fn filter_by_kind_limit_and_offset() {
    let _home = Home::new();
    for _ in 0..3 {
        rec("digest", true, None);
    }
    rec("moodle_sync", true, None);
    assert_eq!(history::list_records(50, 0, Some("digest"), false).len(), 3);
    assert_eq!(history::list_records(2, 0, None, false).len(), 2);
    assert_eq!(history::list_records(10, 3, None, false).len(), 1);
}

#[test]
fn get_last_and_last_ok() {
    let _home = Home::new();
    let r = rec("digest", true, Some("completo"));
    let bad = rec("digest", false, None);
    assert_eq!(history::get(r["id"].as_str().unwrap()).unwrap()["summary"], "completo");
    assert!(history::get("naoexiste").is_none());
    assert_eq!(history::last("digest", false).unwrap()["id"], bad["id"]);
    assert_eq!(history::last("digest", true).unwrap()["id"], r["id"]);
    assert!(history::last("moodle_sync", false).is_none());
}

#[test]
fn record_has_duration_and_short_hex_id() {
    let r = history::new_record("digest", "manual", true, "2026-10-26T18:00:00", None, None, None);
    assert!(r["duration_s"].is_number());
    let id = r["id"].as_str().unwrap();
    assert_eq!(id.len(), 12);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn truncated_line_does_not_break_reading_or_the_next_record() {
    let home = Home::new();
    rec("digest", true, None);
    let path = home.path("EmailNotifier/history.jsonl");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("{\"id\": \"cortado\", \"kind\": \"dige");
    std::fs::write(&path, text).unwrap();
    rec("digest", true, None);
    assert_eq!(history::list_records(50, 0, None, false).len(), 2);
}

#[test]
fn file_is_pruned_when_it_grows_too_much() {
    let home = Home::new();
    for i in 0..(history::PRUNE_AT + 5) {
        history::append(&json!({"id": i.to_string(), "kind": "digest", "ok": true})).unwrap();
    }
    let lines = std::fs::read_to_string(home.path("EmailNotifier/history.jsonl")).unwrap().lines().count();
    assert!(lines <= history::PRUNE_AT);
    assert_eq!(history::list_records(1, 0, None, false)[0]["id"], (history::PRUNE_AT + 4).to_string());
}
