//! Dados das páginas da app: resumo do dia e dos últimos dias, emails, ficheiros do Moodle e atividade/logs.
use crate::config;
use crate::digest::{moodle_dir_of, sent_ids};
use crate::history;
use crate::mail::{self, FetchOptions};
use crate::paths;
use crate::progress;
use crate::service;
use crate::util::{str_or, truncate_chars};
use chrono::{Duration, Local, NaiveDateTime};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Dias de logs guardados localmente.
pub const LOG_DAYS: i64 = 7;
/// Dias de histórico de resumos guardados (o seletor da página principal vai até 30).
pub const HISTORY_DAYS: i64 = 30;

fn start_of(days: usize) -> NaiveDateTime {
    Local::now().naive_local() - Duration::days(days as i64)
}

fn num(r: &Value, key: &str) -> u64 {
    r["details"][key].as_u64().unwrap_or(0)
}

/// Resumo de hoje (em destaque) e dos últimos `days` dias.
pub fn overview(days: usize) -> Value {
    let records = history::list_since(start_of(days), Some("digest"), true);
    let today = Local::now().format("%Y-%m-%d").to_string();
    let has_text = |r: &&Value| r["summary"].as_str().map(|s| !s.is_empty()).unwrap_or(false);
    let is_today = |r: &&Value| r["started"].as_str().map(|s| s.starts_with(&today)).unwrap_or(false);
    let todays = records.iter().filter(has_text).find(is_today).cloned();
    let latest = records.iter().find(has_text).cloned();

    let sent = records.iter().filter(|r| r["details"]["sent"].as_bool().unwrap_or(false)).count();
    let failed = records.iter().filter(|r| !r["ok"].as_bool().unwrap_or(false)).count();
    json!({
        "days": days,
        "today": todays,
        "latest": latest,
        "items": records,
        "totals": {
            "runs": records.len(), "sent": sent, "failed": failed,
            "emails": records.iter().map(|r| num(r, "emails_new")).sum::<u64>(),
            "moodle_files": records.iter().map(|r| num(r, "moodle_files")).sum::<u64>(),
        },
    })
}

/// Emails da origem configurada (Thunderbird ou serviço próprio) dos últimos `days` dias.
pub fn emails(days: usize, limit: usize) -> Value {
    let config = config::load_config();
    let method = str_or(&config, "email_method", "thunderbird").to_lowercase();
    let source = match mail::source_for(&method) {
        Ok(s) => s,
        Err(e) => return json!({"source": method, "days": days, "total": 0, "items": [], "error": e}),
    };
    match source.fetch(&config, start_of(days), FetchOptions { sync: false }) {
        Ok(found) => {
            let sent: HashSet<String> = sent_ids().into_iter().collect();
            let items: Vec<Value> = found
                .iter()
                .take(limit)
                .map(|e| {
                    json!({
                        "subject": e.subject, "sender": e.sender, "received_time": e.received_time,
                        "preview": truncate_chars(&e.body.split_whitespace().collect::<Vec<_>>().join(" "), 280),
                        "message_id": e.message_id, "in_digest": !e.message_id.is_empty() && sent.contains(&e.message_id),
                    })
                })
                .collect();
            json!({"source": source.id(), "days": days, "total": found.len(), "items": items, "error": null})
        }
        Err(e) => json!({"source": source.id(), "days": days, "total": 0, "items": [], "error": e}),
    }
}

/// Ficheiros do Moodle: o que está pendente/em curso/feito na sincronização atual (ou na última) e o que já foi descarregado.
pub fn moodle_files(limit: usize) -> Value {
    let config = config::load_config();
    let dir = moodle_dir_of(&config);
    let mut state = progress::load(dir.as_deref());
    state["downloaded"] = json!(service::recent_new_files(limit));
    state["moodle"] = service::moodle_state(&config);
    state
}

// ---------------------------------------------------------------- logs

/// Mantém as linhas com data a partir de `cutoff`. As linhas sem data (continuações) seguem a linha anterior.
pub fn filter_log(text: &str, cutoff: NaiveDateTime) -> Vec<String> {
    let mut keep = true;
    let mut out = Vec::new();
    for line in text.lines() {
        let stamp = line.strip_prefix('[').and_then(|r| r.get(..19)).and_then(|t| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S").ok());
        if let Some(ts) = stamp {
            keep = ts >= cutoff;
        }
        if keep {
            out.push(line.trim_end_matches('\r').to_string());
        }
    }
    out
}

fn log_lines(name: &str, days: i64, max: usize) -> Vec<String> {
    let path = if name == "notifier" { paths::notifier_log() } else { paths::sync_log(moodle_dir_of(&config::load_config()).as_deref()) };
    let text = std::fs::read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut lines = filter_log(&text, Local::now().naive_local() - Duration::days(days));
    let skip = lines.len().saturating_sub(max);
    lines.drain(..skip);
    lines
}

/// Histórico e logs dos últimos `days` dias (máx. 7, o que se guarda).
pub fn activity(days: usize) -> Value {
    let days = days.clamp(1, LOG_DAYS as usize);
    json!({
        "days": days,
        "history": history::list_since(start_of(days), None, false),
        "logs": {"notifier": log_lines("notifier", days as i64, 400), "sync": log_lines("sync", days as i64, 400)},
    })
}

/// Apaga o que passou do prazo: logs com mais de 7 dias e histórico com mais de 30. Devolve (linhas de log, registos).
pub fn prune() -> (usize, usize) {
    let cutoff = Local::now().naive_local() - Duration::days(LOG_DAYS);
    let mut dropped_lines = 0;
    let sync_log = paths::sync_log(moodle_dir_of(&config::load_config()).as_deref());
    for path in [paths::notifier_log(), sync_log] {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let kept = filter_log(&text, cutoff);
        let before = text.lines().count();
        if kept.len() < before {
            let body = if kept.is_empty() { String::new() } else { kept.join("\n") + "\n" };
            if std::fs::write(&path, body).is_ok() {
                dropped_lines += before - kept.len();
            }
        }
    }
    let dropped_records = history::prune_before(Local::now().naive_local() - Duration::days(HISTORY_DAYS)).unwrap_or(0);
    (dropped_lines, dropped_records)
}
