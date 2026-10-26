//! Histórico de execuções (resumos enviados e sincronizações do Moodle) em JSON Lines.
use crate::paths;
use crate::util::now_iso;
use chrono::NaiveDateTime;
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Mutex;

pub const MAX_RECORDS: usize = 500;
pub const PRUNE_AT: usize = 700;
static LOCK: Mutex<()> = Mutex::new(());

pub fn new_record(kind: &str, trigger: &str, ok: bool, started: &str, details: Option<Value>,
                  summary: Option<&str>, error: Option<&str>) -> Value {
    let finished = now_iso();
    let fmt = "%Y-%m-%dT%H:%M:%S";
    let duration = match (NaiveDateTime::parse_from_str(&finished, fmt), NaiveDateTime::parse_from_str(started, fmt)) {
        (Ok(f), Ok(s)) => json!(((f - s).num_milliseconds() as f64 / 100.0).round() / 10.0),
        _ => Value::Null,
    };
    json!({
        "id": new_id(),
        "kind": kind,          // "digest" | "moodle_sync"
        "trigger": trigger,    // "scheduled" | "manual" | "api"
        "ok": ok,
        "started": started,
        "finished": finished,
        "duration_s": duration,
        "error": error,
        "details": details.unwrap_or_else(|| json!({})),
        "summary": summary,
    })
}

/// Identificador curto aleatório (12 caracteres hexadecimais).
pub fn new_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn append(record: &Value) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = paths::history_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // se a última escrita foi cortada a meio (sem \n final), começa numa linha nova para não corromper este registo
    let needs_newline = match std::fs::File::open(&path) {
        Ok(mut f) => {
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            if len > 0 {
                let mut last = [0u8; 1];
                f.seek(SeekFrom::Start(len - 1)).and_then(|_| f.read_exact(&mut last)).is_ok() && last[0] != b'\n'
            } else {
                false
            }
        }
        Err(_) => false,
    };
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(format!("{}{}\n", if needs_newline { "\n" } else { "" }, record).as_bytes())?;
    drop(file);
    prune_if_needed(&path)
}

fn read_all(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok()) // ignora linhas cortadas
                .filter(Value::is_object)
                .collect()
        })
        .unwrap_or_default()
}

fn prune_if_needed(path: &std::path::Path) -> std::io::Result<()> {
    let records = read_all(path);
    if records.len() > PRUNE_AT {
        let keep = &records[records.len() - MAX_RECORDS..];
        let body: String = keep.iter().map(|r| format!("{r}\n")).collect();
        std::fs::write(path, body)?;
    }
    Ok(())
}

/// Mais recentes primeiro. A lista omite o texto do resumo (pesado) a menos que se peça.
pub fn list_records(limit: usize, offset: usize, kind: Option<&str>, include_summary: bool) -> Vec<Value> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut records = read_all(&paths::history_file());
    if let Some(k) = kind {
        records.retain(|r| r.get("kind").and_then(Value::as_str) == Some(k));
    }
    records.reverse();
    records
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|mut r| {
            if !include_summary {
                if let Some(obj) = r.as_object_mut() {
                    let has = obj.get("summary").map(|s| !s.is_null() && s.as_str() != Some("")).unwrap_or(false);
                    obj.insert("summary".into(), Value::Null);
                    obj.insert("has_summary".into(), json!(has));
                }
            }
            r
        })
        .collect()
}

pub fn get(record_id: &str) -> Option<Value> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read_all(&paths::history_file())
        .into_iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(record_id))
}

pub fn last(kind: &str, only_ok: bool) -> Option<Value> {
    list_records(MAX_RECORDS, 0, Some(kind), false)
        .into_iter()
        .find(|r| !only_ok || r.get("ok").and_then(Value::as_bool).unwrap_or(false))
}
