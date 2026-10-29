//! Resumo diário: lê emails, junta as novidades do Moodle, gera o resumo e envia para o Session.
use crate::config::load_config;
use crate::history;
use crate::mail::{self, FetchOptions};
use crate::model::Email;
use crate::moodle_report::{build_moodle_section, load_new_files};
use crate::moodle_sync;
use crate::paths;
use crate::session::send_to_session;
use crate::summarizer::summarize_emails;
use crate::util::{now_iso, read_json, str_of, write_json_atomic, Json};
use chrono::{Duration as ChronoDuration, Local, NaiveDateTime};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub const OVERLAP_HOURS: i64 = 48;
const MOODLE_SYNC_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Default, Clone)]
pub struct DigestResult {
    pub ok: bool,
    pub sent: bool,
    pub skipped: bool, // nada de novo para enviar
    pub preview: bool,
    pub emails_found: usize,
    pub emails_new: usize,
    pub moodle_files: usize,
    pub ai_used: bool,
    pub ai_fallback: bool,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub log: Vec<String>,
}

impl DigestResult {
    pub fn to_json(&self) -> Value {
        json!({
            "ok": self.ok, "sent": self.sent, "skipped": self.skipped, "preview": self.preview,
            "emails_found": self.emails_found, "emails_new": self.emails_new, "moodle_files": self.moodle_files,
            "ai_used": self.ai_used, "ai_fallback": self.ai_fallback, "summary": self.summary,
            "error": self.error, "log": self.log,
        })
    }
}

type GetEmails = Box<dyn Fn(&Json, NaiveDateTime) -> Result<Vec<Email>, String> + Send + Sync>;
type Send_ = Box<dyn Fn(&str, &Json, &dyn Fn(&str)) -> bool + Send + Sync>;
type EnsureSynced = Box<dyn Fn(&Json, &dyn Fn(&str)) + Send + Sync>;

/// Dependências externas (emails, Session, sincronização do Moodle) — trocáveis nos testes.
pub struct Deps {
    pub get_emails: GetEmails,
    pub send: Send_,
    pub ensure_moodle_synced: EnsureSynced,
}

impl Default for Deps {
    fn default() -> Self {
        Deps {
            get_emails: Box::new(|config, since| mail::get_emails(config, since, FetchOptions::default())),
            send: Box::new(|text, config, log| send_to_session(text, config, log)),
            ensure_moodle_synced: Box::new(ensure_moodle_synced),
        }
    }
}

pub fn file_log(msg: &str) {
    let line = format!("[{}] {msg}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(paths::notifier_log()) {
        let _ = writeln!(f, "{line}");
    }
}

// ---------------------------------------------------------------- estado entre execuções (last_run.json)

fn last_run_value(key: &str) -> Option<String> {
    read_json(&paths::last_run_file()).get(key).and_then(Value::as_str).map(str::to_owned)
}

pub fn sent_ids() -> Vec<String> {
    read_json(&paths::last_run_file())
        .get("sent_ids")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// Lê datas ISO com ou sem fração de segundo (a versão Python escreve microssegundos).
pub fn parse_iso(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").ok()
}

fn iso_now_micro() -> String {
    Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

pub fn get_since_datetime(log: &dyn Fn(&str)) -> NaiveDateTime {
    if let Some(v) = last_run_value("last_run") {
        match parse_iso(&v) {
            Some(dt) => return dt,
            None => log(&format!("Aviso ao ler last_run.json: data inválida «{v}»")),
        }
    }
    Local::now().naive_local() - ChronoDuration::hours(24)
}

pub fn update_last_run(new_ids: &[String], moodle_ts: Option<&str>) {
    let mut ids = sent_ids();
    ids.extend(new_ids.iter().cloned());
    let keep = ids.len().saturating_sub(1000);
    let mut data = Json::new();
    data.insert("last_run".into(), json!(iso_now_micro()));
    data.insert(
        "last_moodle_ts".into(),
        json!(moodle_ts.map(str::to_owned).or_else(|| last_run_value("last_moodle_ts")).unwrap_or_else(iso_now_micro)),
    );
    data.insert("sent_ids".into(), json!(ids[keep..]));
    let _ = write_json_atomic(&paths::last_run_file(), &data);
}

// ---------------------------------------------------------------- Moodle

/// Pasta do sincronizador a usar (config > pasta ao lado, se existir).
pub fn moodle_dir_of(config: &Json) -> Option<PathBuf> {
    let configured = str_of(config, "moodle_sync_dir");
    if !configured.is_empty() {
        return Some(PathBuf::from(configured));
    }
    let default = paths::sync_dir();
    default.join("config.json").exists().then_some(default)
}

/// Garante que o Moodle já foi sincronizado hoje (ou sincroniza-o agora) para o resumo incluir o que foi descarregado.
pub fn ensure_moodle_synced(config: &Json, log: &dyn Fn(&str)) {
    let Some(dir) = moodle_dir_of(config) else { return };
    if config.get("moodle_auto_download").and_then(Value::as_bool) == Some(false) {
        return; // o utilizador desativou o descarregamento automático
    }
    let Some(base) = crate::institutions::moodle_url(config) else {
        log("Moodle: falta o endereço da instituição; nada descarregado.");
        return;
    };
    let lock = paths::sync_lock(Some(&dir));
    let mut waited = Duration::ZERO;
    while lock.exists() && waited < MOODLE_SYNC_TIMEOUT {
        // outra sincronização (arranque do Windows) a decorrer: espera que termine
        std::thread::sleep(Duration::from_secs(10));
        waited += Duration::from_secs(10);
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    if str_of(&read_json(&paths::sync_config(Some(&dir))), "last_sync_date") == today {
        return;
    }
    log("A sincronizar o Moodle antes de gerar o resumo...");
    moodle_sync::run_sync("scheduled", Some(&dir), Some(&base), None);
}

fn moodle_section(config: &Json, fallback_since: NaiveDateTime, log: &dyn Fn(&str)) -> (Vec<String>, usize) {
    let Some(dir) = moodle_dir_of(config) else { return (vec![], 0) };
    let since = last_run_value("last_moodle_ts").and_then(|v| parse_iso(&v)).unwrap_or(fallback_since);
    let new_files = load_new_files(&paths::new_files_log(Some(&dir)), since);
    let download_dir = match str_of(config, "moodle_download_dir") {
        "" => paths::default_download_dir(),
        d => PathBuf::from(d),
    };
    log(&format!("Ficheiros novos no Moodle: {}", new_files.len()));
    (build_moodle_section(&new_files, &download_dir, 6), new_files.len())
}

// ---------------------------------------------------------------- execução

/// Gera (e, se `send`, envia) o resumo. Com `send = false` é só uma pré-visualização: não envia,
/// não avança last_run.json e não entra no histórico.
pub fn run_digest(send: bool, trigger: &str, progress: Option<&dyn Fn(&str)>, config: Option<Json>, deps: &Deps, sync_moodle: bool) -> DigestResult {
    let mut res = DigestResult { preview: !send, ..Default::default() };
    let started = now_iso();
    let lines = std::sync::Mutex::new(Vec::<String>::new());
    let log = |msg: &str| {
        lines.lock().unwrap().push(msg.to_string());
        file_log(msg);
        if let Some(p) = progress {
            p(msg);
        }
    };

    log(if send { "=== INÍCIO DA EXECUÇÃO DO NOTIFICADOR ISEP ===" } else { "=== PRÉ-VISUALIZAÇÃO DO RESUMO ===" });
    let config = config.unwrap_or_else(load_config);
    let finish = |mut res: DigestResult| {
        res.log = lines.lock().unwrap().clone();
        if send {
            // pré-visualizações não entram no histórico
            let details = json!({"emails_found": res.emails_found, "emails_new": res.emails_new, "moodle_files": res.moodle_files,
                "sent": res.sent, "skipped": res.skipped, "ai_used": res.ai_used, "ai_fallback": res.ai_fallback});
            let rec = history::new_record("digest", trigger, res.ok, &started, Some(details), res.summary.as_deref(), res.error.as_deref());
            let _ = history::append(&rec);
        }
        res
    };
    if config.is_empty() {
        res.error = Some("config.json não encontrado. Executa primeiro o CONFIGURAR_TUDO.bat.".into());
        log(&format!("ERRO: {}", res.error.as_deref().unwrap()));
        return finish(res);
    }

    // Emails sincronizados tarde (PC em suspensão) trazem uma data anterior ao último arranque,
    // por isso pesquisa com sobreposição e deduplica por Message-ID.
    let since = get_since_datetime(&log) - ChronoDuration::hours(OVERLAP_HOURS);
    log(&format!("A procurar emails recebidos desde: {}", since.format("%d/%m/%Y %H:%M:%S")));
    let emails = match (deps.get_emails)(&config, since) {
        Ok(e) => e,
        Err(err) => {
            log(&format!("ERRO ao obter emails: {err}"));
            res.error = Some(err);
            return finish(res);
        }
    };
    res.emails_found = emails.len();
    log(&format!("Emails encontrados: {}", emails.len()));

    if sync_moodle {
        (deps.ensure_moodle_synced)(&config, &log);
    }
    let moodle_ts = iso_now_micro();
    let (moodle_lines, moodle_files) = moodle_section(&config, since, &log);
    res.moodle_files = moodle_files;

    let already: std::collections::HashSet<String> = sent_ids().into_iter().collect();
    let emails: Vec<Email> = emails.into_iter().filter(|e| e.message_id.is_empty() || !already.contains(&e.message_id)).collect();
    res.emails_new = emails.len();
    log(&format!("Emails novos (ainda não enviados): {}", emails.len()));

    if emails.is_empty() && moodle_lines.is_empty() {
        log("Nenhum email novo recebido. Nenhuma mensagem necessária.");
        res.ok = true;
        res.skipped = true;
        if send {
            update_last_run(&[], Some(&moodle_ts));
        }
        log("=== EXECUÇÃO TERMINADA (SEM NOVOS EMAILS) ===");
        return finish(res);
    }

    log("A gerar resumo e priorização dos emails...");
    let summary = summarize_emails(&emails, &config, &moodle_lines);
    res.ai_fallback = summary.contains("IA indisponível");
    res.ai_used = str_of(&config, "summary_mode") == "ai" && !res.ai_fallback;
    res.summary = Some(summary.clone());
    log("Resumo gerado com sucesso!");

    if !send {
        res.ok = true;
        res.log = lines.lock().unwrap().clone();
        return res;
    }
    res.sent = (deps.send)(&summary, &config, &log);
    res.ok = res.sent;
    if res.sent {
        let ids: Vec<String> = emails.iter().filter(|e| !e.message_id.is_empty()).map(|e| e.message_id.clone()).collect();
        update_last_run(&ids, Some(&moodle_ts));
        log("Estado atualizado com sucesso no last_run.json.");
    } else {
        res.error = Some("Falha no envio para o Session.".into());
        log("Aviso: last_run.json não foi atualizado devido a falha no envio.");
    }
    log("=== EXECUÇÃO TERMINADA ===");
    finish(res)
}
