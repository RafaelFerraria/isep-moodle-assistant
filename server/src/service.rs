//! Operações de alto nível usadas pela API: estado, definições, testes e dados.
use crate::ai::{api_defaults, describe, resolve_cli_command};
use crate::config::{self, ValidationError};
use crate::digest::moodle_dir_of;
use crate::history;
use crate::moodle_report::load_new_files;
use crate::moodle_sync;
use crate::paths;
use crate::summarizer::{sample_emails, summarize_emails};
use crate::system;
use crate::util::{read_json, str_of, str_or, which, Json};
use chrono::{Local, NaiveDateTime};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub const LOG_NAMES: [&str; 2] = ["notifier", "sync"];

/// Endereço do Moodle do utilizador (ver `institutions::moodle_url`). `None` se a instituição não tem Moodle conhecido
/// e o utilizador ainda não indicou o endereço.
pub fn moodle_base_url(config: &Json) -> Option<String> {
    crate::institutions::moodle_url(config)
}

pub const NO_MOODLE_URL: &str = "Falta o endereço do Moodle da tua instituição. Indica-o nas Definições (Moodle).";

fn check(id: &str, label: &str, ok: bool, detail: &str, required: bool) -> Value {
    json!({"id": id, "label": label, "ok": ok, "detail": detail, "required": required})
}

/// A IA está pronta a usar? (CLI instalada / token disponível)
pub fn ai_ready(config: &Json) -> Value {
    if str_or(config, "summary_mode", "rules") != "ai" {
        return check("ai", "Resumo por IA", true, "Modo 'Regras' (local) ativo.", false);
    }
    if str_or(config, "ai_provider", "cli") == "cli" {
        let exe = match resolve_cli_command(config) {
            Ok(cmd) => cmd[0].clone(),
            Err(e) => return check("ai", "Resumo por IA", false, &e, true),
        };
        let found = which(&exe).is_some();
        let detail = if found { format!("CLI '{exe}' encontrada.") } else { format!("CLI '{exe}' não encontrada no PATH.") };
        return check("ai", "Resumo por IA", found, &detail, true);
    }
    let api_type = str_or(config, "ai_api_type", "openai");
    let env_name = api_defaults(api_type).map(|d| d.env).unwrap_or("");
    let has_key = !str_of(config, "ai_api_key").is_empty() || (!env_name.is_empty() && std::env::var(env_name).map(|v| !v.is_empty()).unwrap_or(false));
    let detail = if has_key { "Token de API disponível.".to_string() } else { format!("Sem token (define-o ou usa a variável {env_name}).") };
    check("ai", "Resumo por IA", has_key, &detail, true)
}

pub fn moodle_state(config: &Json) -> Value {
    let dir = moodle_dir_of(config);
    let sync_cfg = dir.as_deref().map(|d| read_json(&paths::sync_config(Some(d)))).unwrap_or_default();
    let configured = !str_of(&sync_cfg, "username").is_empty() && !str_of(&sync_cfg, "encrypted_password").is_empty();
    let download_dir = [str_of(&sync_cfg, "download_dir"), str_of(config, "moodle_download_dir")]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| paths::default_download_dir().to_string_lossy().into_owned());
    let last_sync = str_of(&sync_cfg, "last_sync_date");
    json!({
        "configured": configured,
        "username": str_of(&sync_cfg, "username"),
        "last_sync_date": if last_sync.is_empty() { Value::Null } else { json!(last_sync) },
        "run_after_hour": sync_cfg.get("run_after_hour").cloned().unwrap_or(json!(18)),
        "download_dir": download_dir,
        "sync_dir": dir.map(|d| d.to_string_lossy().into_owned()),
    })
}

pub fn get_status(active_job: Value) -> Value {
    let config = config::load_config();
    let tasks = system::query_tasks();
    let mood = moodle_state(&config);
    let settings = config::public_settings(&config);
    let thunderbird = system::find_thunderbird_path();

    let checks = vec![
        check("config", "Configuração criada", !config.is_empty(), if config.is_empty() { "Abre o assistente de configuração." } else { "" }, true),
        check("session", "Destino Session configurado", settings["destination"]["session_configured"].as_bool().unwrap_or(false), "", true),
        check("email", "Email da escola ligado", crate::oauth::is_connected(&config), "Liga a conta de email nas Definições.",
            matches!(str_or(&config, "email_method", "thunderbird"), "imap" | "graph" | "outlook")),
        check("thunderbird", "Thunderbird instalado", thunderbird.is_some(),
            if thunderbird.is_some() { "" } else { "Necessário para ler os emails do ISEP." },
            str_or(&config, "email_method", "thunderbird") == "thunderbird"),
        check("node", "Motor do Session (Node.js)", system::node_path().is_some(), "Necessário para enviar para o Session; vem dentro da app.", true),
        check("moodle", "Credenciais do Moodle", mood["configured"].as_bool().unwrap_or(false),
            if mood["configured"].as_bool().unwrap_or(false) { "" } else { "Define o utilizador e a palavra-passe do Moodle." }, false),
        check("tasks", "Tarefas agendadas no Windows", !tasks.is_empty() && !tasks_broken(&tasks),
            &if tasks.is_empty() {
                "Sem tarefas: o resumo não corre sozinho.".to_string()
            } else if tasks_broken(&tasks) {
                "Uma tarefa aponta para um programa que já não existe: volta a registar o agendamento.".to_string()
            } else {
                format!("{} tarefa(s) registada(s).", tasks.len())
            },
            system::IS_WINDOWS),
        check("internet", "Ligação ao Moodle", system::internet_ok(&moodle_base_url(&config).map(|u| crate::institutions::host_of(&u)).unwrap_or_else(|| "login.microsoftonline.com".into())), "", true),
        ai_ready(&config),
    ];
    let mut next_runs: Vec<&str> = tasks.iter().filter_map(|t| t["next_run"].as_str()).collect();
    next_runs.sort();
    let ok = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(false) || !c["required"].as_bool().unwrap_or(true));
    json!({
        "version": crate::VERSION,
        "now": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        "ok": ok,
        "checks": checks,
        "summary_mode": {"mode": settings["summary"]["mode"], "description": describe(&config)},
        "digest": {
            "last": history::last("digest", false), "last_ok": history::last("digest", true),
            "next_run": next_runs.first(), "scheduled_time": settings["schedule"]["time"],
        },
        "moodle": merge(&mood, json!({"last": history::last("moodle_sync", false)})),
        "tasks": tasks,
        "active_job": active_job,
    })
}

/// Alguma tarefa aponta para um ficheiro que não existe? (nunca correria)
fn tasks_broken(tasks: &[Value]) -> bool {
    tasks.iter().any(|t| t["target_ok"].as_bool() == Some(false))
}

/// Estado do assistente de configuração: o que já está feito e o que falta.
pub fn get_setup(active_job: Value) -> Value {
    let config = config::load_config();
    let settings = config::public_settings(&config);
    let tasks = system::query_tasks();
    let mood = moodle_state(&config);
    let method = str_or(&config, "email_method", "thunderbird").to_string();
    let (email_ok, email_detail) = match method.as_str() {
        "imap" | "graph" | "outlook" => (
            crate::oauth::is_connected(&config) && str_of(&config, "email_address").contains('@'),
            "Email da escola ligado.".to_string(),
        ),
        _ => (system::find_thunderbird_path().is_some(), "Thunderbird instalado com a conta do ISEP.".to_string()),
    };
    let session = crate::session::state(&config);
    let session_ok = session["configured"].as_bool().unwrap_or(false) && session["node_ok"].as_bool().unwrap_or(false);
    let schedule_ok = !tasks.is_empty() && !tasks_broken(&tasks);
    let student = &settings["student"];
    let steps = vec![
        check("profile", "Perfil", student["configured"].as_bool().unwrap_or(false), "Instituição, curso e ano.", true),
        check("email", "Emails", email_ok, &email_detail, true),
        check("moodle", "Moodle", mood["configured"].as_bool().unwrap_or(false), "Número e palavra-passe do Moodle.", false),
        check("session", "Session", session_ok, "Bot criado e o teu Session associado.", true),
        check("summary", "Tipo de resumo", true, &describe(&config), false),
        check("schedule", "Agendamento", schedule_ok, "O resumo corre sozinho à hora definida.", true),
    ];
    json!({
        "done": settings["setup"]["done"],
        "email_method": method,
        "steps": steps,
        "ready": steps.iter().all(|c| c["ok"].as_bool().unwrap_or(false) || !c["required"].as_bool().unwrap_or(true)),
        "active_job": active_job,
    })
}

fn merge(a: &Value, b: Value) -> Value {
    let mut out = a.as_object().cloned().unwrap_or_default();
    if let Value::Object(extra) = b {
        out.extend(extra);
    }
    Value::Object(out)
}

pub fn get_settings() -> Value {
    config::public_settings(&config::load_config())
}

/// Valida e grava. Se a hora mudou, volta a registar as tarefas do Windows.
pub fn update_settings(patch: &Value) -> Result<Value, ValidationError> {
    let current = config::load_config();
    let (new_config, effects) = config::apply_settings_patch(&current, patch)?;
    if let Err(e) = config::save_config(&new_config) {
        let mut errors = Json::new();
        errors.insert("config".into(), json!(format!("não foi possível gravar: {e}")));
        return Err(ValidationError { errors });
    }
    let mut result = json!({"settings": config::public_settings(&new_config), "effects": effects});
    if effects.iter().any(|e| e == "reschedule") {
        let (ok, message) = system::register_tasks(str_of(&new_config, "scheduled_time"));
        result["reschedule"] = json!({"ok": ok, "message": message});
    }
    Ok(result)
}

/// Resumo de exemplo com a configuração atual (verifica a ligação à IA sem ler emails reais).
pub fn test_summary() -> Value {
    let config = config::load_config();
    let text = summarize_emails(&sample_emails(), &config, &[]);
    let fallback = text.contains("IA indisponível");
    json!({"text": text, "ai_requested": str_or(&config, "summary_mode", "rules") == "ai", "ai_fallback": fallback, "ok": !fallback})
}

/// Valida o login no Moodle e guarda a palavra-passe cifrada (DPAPI). Nunca a devolve.
pub fn set_moodle_credentials(username: &str, password: &str) -> Result<String, String> {
    let config = config::load_config();
    let download_dir = match str_of(&config, "moodle_download_dir") {
        "" => paths::default_download_dir(),
        d => PathBuf::from(d),
    };
    let sync_dir = moodle_dir_of(&config);
    let base = moodle_base_url(&config).ok_or(NO_MOODLE_URL)?;
    moodle_sync::set_credentials(username, password, Some(&base), sync_dir.as_deref(), &download_dir)
}

pub fn recent_new_files(limit: usize) -> Vec<Value> {
    let config = config::load_config();
    let Some(dir) = moodle_dir_of(&config) else { return vec![] };
    let files = load_new_files(&paths::new_files_log(Some(&dir)), NaiveDateTime::MIN);
    files.into_iter().rev().take(limit).map(|f| serde_json::to_value(f).unwrap_or(Value::Null)).collect()
}

pub fn tail_log(name: &str, lines: usize) -> Option<Vec<String>> {
    if !LOG_NAMES.contains(&name) {
        return None;
    }
    let lines = lines.clamp(1, 2000);
    let path = if name == "notifier" {
        paths::notifier_log()
    } else {
        paths::sync_log(moodle_dir_of(&config::load_config()).as_deref())
    };
    let Ok(file) = std::fs::File::open(path) else { return Some(vec![]) };
    let mut tail: VecDeque<String> = VecDeque::with_capacity(lines + 1);
    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        tail.push_back(String::from_utf8_lossy(&line).trim_end_matches('\r').to_string());
        if tail.len() > lines {
            tail.pop_front();
        }
    }
    Some(tail.into_iter().collect())
}
