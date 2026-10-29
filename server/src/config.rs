//! Leitura/escrita da configuração e validação das definições editáveis (usadas pela API/UI).
//!
//! Os segredos (mnemónica do Session, token de API da IA…) nunca saem por aqui: só se definem ou limpam.
use crate::ai::{api_defaults, API_TYPES, CLI_NAMES};
use crate::paths;
use crate::util::{read_json, str_of, write_json_atomic, Json};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::{LazyLock, Mutex};

static WRITE_LOCK: Mutex<()> = Mutex::new(());
static TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([01]?\d|2[0-3]):[0-5]\d$").unwrap());
static SESSION_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^05[0-9a-fA-F]{64}$").unwrap());
static HTTP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^https?://").unwrap());

/// Definições inválidas: campo -> mensagem.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub errors: Json,
}

pub fn load_config() -> Json {
    read_json(&paths::email_config())
}

pub fn save_config(config: &Json) -> std::io::Result<()> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    write_json_atomic(&paths::email_config(), config)
}

pub fn valid_session_id(value: &str) -> bool {
    SESSION_ID_RE.is_match(value)
}

pub fn mask_session_id(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() > 12 {
        format!("{}…{}", chars[..6].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
    } else {
        value.to_string()
    }
}

pub fn recipients_of(config: &Json) -> Vec<String> {
    let mut out: Vec<String> = match config.get("session_recipients") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        _ => vec![],
    };
    out.retain(|s| !s.trim().is_empty());
    if out.is_empty() {
        let group = str_of(config, "session_group_id");
        if !group.is_empty() {
            out.push(group.to_string());
        }
    }
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn or<'a>(config: &'a Json, key: &str, default: &'a str) -> &'a str {
    crate::util::str_or(config, key, default)
}

/// A configuração inicial está concluída? Uma instalação anterior já funcional (Session configurado) conta como concluída,
/// para quem já usava a app não ter de repetir o assistente.
pub fn setup_done(config: &Json) -> bool {
    if let Some(v) = config.get("setup_done").and_then(Value::as_bool) {
        return v;
    }
    !recipients_of(config).is_empty() && !str_of(config, "session_mnemonic").is_empty()
}

/// Definições para mostrar na UI. Os segredos aparecem só como `..._set: true/false`.
pub fn public_settings(config: &Json) -> Value {
    let api_type = or(config, "ai_api_type", "openai");
    let env_name = api_defaults(api_type).map(|d| d.env).unwrap_or("");
    let recipients = recipients_of(config);
    let has_mnemonic = !str_of(config, "session_mnemonic").is_empty();
    json!({
        "summary": {
            "mode": or(config, "summary_mode", "rules"),
            "ai_provider": or(config, "ai_provider", "cli"),
            "ai_cli": or(config, "ai_cli", "claude"),
            "ai_cli_command": config.get("ai_cli_command").cloned().unwrap_or(json!("")),
            "ai_api_type": api_type,
            "ai_model": str_of(config, "ai_model"),
            "ai_base_url": str_of(config, "ai_base_url"),
            "rules": crate::rules::Rules::from_config(config).to_json(),
            "ai_api_key_set": !str_of(config, "ai_api_key").is_empty(),
            "ai_api_key_env": env_name,
            "ai_api_key_env_set": !env_name.is_empty() && std::env::var(env_name).map(|v| !v.is_empty()).unwrap_or(false),
        },
        "schedule": { "time": or(config, "scheduled_time", "18:00") },
        "destination": {
            "session_recipients": recipients.iter().map(|r| mask_session_id(r)).collect::<Vec<_>>(),
            "session_configured": !recipients.is_empty() && has_mnemonic,
        },
        "email": {
            "method": or(config, "email_method", "thunderbird"),
            "address": str_of(config, "email_address"),
            "connected": crate::oauth::is_connected(config),
        },
        "setup": { "done": setup_done(config) },
        "student": student_public(config),
        "moodle": {
            "sync_dir": or(config, "moodle_sync_dir", &paths::sync_dir().to_string_lossy()),
            "download_dir": or(config, "moodle_download_dir", &paths::default_download_dir().to_string_lossy()),
            "default_download_dir": paths::default_download_dir().to_string_lossy(),
            "download_dir_is_default": str_of(config, "moodle_download_dir").is_empty(),
            "auto_download": config.get("moodle_auto_download").and_then(Value::as_bool).unwrap_or(true),
            "url": crate::institutions::moodle_url(config),
            "url_source": crate::institutions::moodle_url_source(config),
        },
    })
}

/// Perfil do estudante (guardado só neste PC; mais tarde irá para a base de dados associado ao email).
fn student_public(config: &Json) -> Value {
    let id = str_of(config, "student_institution");
    let name = match crate::institutions::find(id) {
        Some(i) => i.name.to_string(),
        None => str_of(config, "student_institution_name").to_string(),
    };
    let year = config.get("student_year").and_then(Value::as_u64).unwrap_or(0);
    json!({
        "institution_id": id, "institution_name": name, "course": str_of(config, "student_course"), "year": year,
        "configured": !id.is_empty() && !str_of(config, "student_course").is_empty() && year > 0,
    })
}

fn err(errors: &mut Json, field: &str, msg: &str) {
    errors.insert(field.to_string(), Value::String(msg.to_string()));
}

fn sub<'a>(patch: &'a Value, key: &str) -> Option<&'a Json> {
    patch.get(key).and_then(Value::as_object)
}

fn summary_rules_patch(patch: &Value) -> Option<&Value> {
    patch.get("summary").and_then(|s| s.get("rules"))
}

/// Aplica uma alteração parcial (mesma forma que `public_settings`) e devolve `(config, efeitos)`.
///
/// Efeito possível: `"reschedule"` (a hora mudou: voltar a registar as tarefas do Windows).
/// Devolve todos os campos inválidos de uma vez.
pub fn apply_settings_patch(config: &Json, patch: &Value) -> Result<(Json, Vec<String>), ValidationError> {
    let mut new = config.clone();
    let mut errors = Json::new();
    let mut effects: Vec<String> = Vec::new();
    let one_of = |value: &Value, allowed: &[&str]| -> Option<String> {
        value.as_str().filter(|v| allowed.contains(v)).map(str::to_owned)
    };
    let list = |items: &[&str]| format!("deve ser um de {:?}", items);

    if let Some(summary) = sub(patch, "summary") {
        if let Some(v) = summary.get("mode") {
            match one_of(v, &["rules", "ai"]) {
                Some(m) => {
                    new.insert("summary_mode".into(), json!(m));
                }
                None => err(&mut errors, "summary.mode", &list(&["ai", "rules"])),
            }
        }
        if let Some(v) = summary.get("ai_provider") {
            match one_of(v, &["cli", "api"]) {
                Some(m) => {
                    new.insert("ai_provider".into(), json!(m));
                }
                None => err(&mut errors, "summary.ai_provider", &list(&["api", "cli"])),
            }
        }
        if let Some(v) = summary.get("ai_cli") {
            let mut allowed: Vec<&str> = CLI_NAMES.to_vec();
            allowed.push("custom");
            match one_of(v, &allowed) {
                Some(m) => {
                    new.insert("ai_cli".into(), json!(m));
                }
                None => {
                    let mut sorted = allowed.clone();
                    sorted.sort();
                    err(&mut errors, "summary.ai_cli", &list(&sorted))
                }
            }
        }
        if let Some(v) = summary.get("ai_cli_command") {
            if v.is_string() || v.is_array() {
                new.insert("ai_cli_command".into(), v.clone());
            } else {
                err(&mut errors, "summary.ai_cli_command", "texto ou lista");
            }
        }
        if let Some(v) = summary.get("ai_api_type") {
            match one_of(v, API_TYPES) {
                Some(m) => {
                    new.insert("ai_api_type".into(), json!(m));
                }
                None => {
                    let mut sorted = API_TYPES.to_vec();
                    sorted.sort();
                    err(&mut errors, "summary.ai_api_type", &list(&sorted))
                }
            }
        }
        for key in ["ai_model", "ai_base_url"] {
            if let Some(v) = summary.get(key) {
                match v.as_str() {
                    None => err(&mut errors, &format!("summary.{key}"), "deve ser texto"),
                    Some(s) if key == "ai_base_url" && !s.is_empty() && !HTTP_RE.is_match(s) => {
                        err(&mut errors, "summary.ai_base_url", "deve começar por http:// ou https://")
                    }
                    Some(s) => {
                        new.insert(key.into(), json!(s.trim()));
                    }
                }
            }
        }
        if summary.get("clear_ai_api_key").and_then(Value::as_bool).unwrap_or(false) {
            new.remove("ai_api_key");
        }
        if let Some(k) = summary.get("ai_api_key").and_then(Value::as_str) {
            if !k.trim().is_empty() {
                new.insert("ai_api_key".into(), json!(k.trim()));
            }
        }
    }

    // coerência: o modo IA exige uma ligação válida
    if str_of(&new, "summary_mode") == "ai" {
        let provider = or(&new, "ai_provider", "cli");
        let cli_empty = match new.get("ai_cli_command") {
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            _ => true,
        };
        if provider == "cli" && str_of(&new, "ai_cli") == "custom" && cli_empty {
            errors.insert("summary.ai_cli_command".into(), json!("obrigatório quando a CLI é 'custom'"));
        }
        if provider == "api" && str_of(&new, "ai_api_type") == "openai_compatible" {
            if str_of(&new, "ai_base_url").is_empty() {
                errors.insert("summary.ai_base_url".into(), json!("obrigatório para APIs compatíveis com OpenAI"));
            }
            if str_of(&new, "ai_model").is_empty() {
                errors.insert("summary.ai_model".into(), json!("obrigatório para APIs compatíveis com OpenAI"));
            }
        }
    }

    if let Some(schedule) = sub(patch, "schedule") {
        if let Some(t) = schedule.get("time") {
            match t.as_str().filter(|s| TIME_RE.is_match(s)) {
                Some(s) => {
                    let (h, m) = s.split_once(':').unwrap();
                    let normalized = format!("{:02}:{:02}", h.parse::<u32>().unwrap(), m.parse::<u32>().unwrap());
                    if normalized != str_of(&new, "scheduled_time") {
                        effects.push("reschedule".into());
                    }
                    new.insert("scheduled_time".into(), json!(normalized));
                }
                None => {
                    errors.insert("schedule.time".into(), json!("formato HH:MM"));
                }
            }
        }
    }

    if let Some(dest) = sub(patch, "destination") {
        if let Some(sid) = dest.get("session_id") {
            let sid = sid.as_str().map(str::trim).unwrap_or("");
            if SESSION_ID_RE.is_match(sid) {
                new.insert("session_recipients".into(), json!([sid]));
                new.insert("session_group_id".into(), json!(sid));
            } else {
                errors.insert("destination.session_id".into(), json!("deve ter 66 caracteres hexadecimais e começar por 05"));
            }
        }
    }

    if let Some(email) = sub(patch, "email") {
        if let Some(v) = email.get("method") {
            match one_of(v, &["thunderbird", "imap"]) {
                Some(m) => {
                    new.insert("email_method".into(), json!(m));
                }
                None => err(&mut errors, "email.method", &list(&["imap", "thunderbird"])),
            }
        }
        if let Some(v) = email.get("address") {
            match v.as_str().map(str::trim) {
                Some(a) if a.is_empty() || a.contains('@') => {
                    new.insert("email_address".into(), json!(a));
                }
                _ => err(&mut errors, "email.address", "endereço de email inválido"),
            }
        }
    }

    if let Some(rules) = summary_rules_patch(patch) {
        match crate::rules::Rules::from_config(&new).patch(rules) {
            Ok(r) => {
                new.insert("summary_rules".into(), r.to_json());
            }
            Err(fields) => {
                for (k, m) in fields {
                    errors.insert(format!("summary.rules.{k}"), json!(m));
                }
            }
        }
    }

    if let Some(student) = sub(patch, "student") {
        let mut next_inst = str_of(&new, "student_institution").to_string();
        if let Some(v) = student.get("institution_id") {
            match v.as_str().filter(|id| *id == crate::institutions::OTHER || crate::institutions::find(id).is_some()) {
                Some(id) => {
                    next_inst = id.to_string();
                    new.insert("student_institution".into(), json!(id));
                    if id != crate::institutions::OTHER {
                        new.remove("student_institution_name");
                    }
                }
                None => err(&mut errors, "student.institution_id", "instituição desconhecida"),
            }
        }
        if let Some(v) = student.get("institution_name") {
            match v.as_str().map(str::trim).filter(|s| (2..=100).contains(&s.chars().count())) {
                Some(n) => {
                    new.insert("student_institution_name".into(), json!(n));
                }
                None => err(&mut errors, "student.institution_name", "entre 2 e 100 caracteres"),
            }
        }
        if next_inst == crate::institutions::OTHER && str_of(&new, "student_institution_name").is_empty() {
            err(&mut errors, "student.institution_name", "indica o nome da instituição");
        }
        if let Some(v) = student.get("course") {
            match v.as_str().map(str::trim).filter(|s| (2..=120).contains(&s.chars().count())) {
                Some(c) => {
                    new.insert("student_course".into(), json!(c));
                }
                None => err(&mut errors, "student.course", "entre 2 e 120 caracteres"),
            }
        }
        if let Some(v) = student.get("year") {
            match v.as_u64().filter(|y| (1..=6).contains(y)) {
                Some(y) => {
                    new.insert("student_year".into(), json!(y));
                }
                None => err(&mut errors, "student.year", "entre 1 e 6"),
            }
        }
    }

    if let Some(setup) = sub(patch, "setup") {
        if let Some(v) = setup.get("done") {
            match v.as_bool() {
                Some(b) => {
                    new.insert("setup_done".into(), json!(b));
                }
                None => err(&mut errors, "setup.done", "deve ser verdadeiro ou falso"),
            }
        }
    }

    if let Some(moodle) = sub(patch, "moodle") {
        if let Some(v) = moodle.get("url") {
            match v.as_str().map(str::trim) {
                Some("") => {
                    new.remove("moodle_url");
                }
                Some(raw) => match crate::institutions::normalize_url(raw) {
                    Some(u) => {
                        new.insert("moodle_url".into(), json!(u));
                    }
                    None => err(&mut errors, "moodle.url", "endereço inválido (ex.: https://moodle.escola.pt)"),
                },
                None => err(&mut errors, "moodle.url", "deve ser texto"),
            }
        }
        if let Some(v) = moodle.get("auto_download") {
            match v.as_bool() {
                Some(b) => {
                    new.insert("moodle_auto_download".into(), json!(b));
                }
                None => err(&mut errors, "moodle.auto_download", "deve ser verdadeiro ou falso"),
            }
        }
        if moodle.get("download_dir").and_then(Value::as_str).map(str::trim) == Some("") {
            // vazio = voltar à pasta predefinida
            new.remove("moodle_download_dir");
        }
        for (key, target) in [("sync_dir", "moodle_sync_dir"), ("download_dir", "moodle_download_dir")] {
            if let Some(v) = moodle.get(key) {
                if key == "download_dir" && v.as_str().map(str::trim) == Some("") {
                    continue; // vazio = pasta predefinida (já tratado acima)
                }
                match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(s) => {
                        new.insert(target.into(), json!(s));
                    }
                    None => {
                        errors.insert(format!("moodle.{key}"), json!("caminho em falta"));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok((new, effects))
    } else {
        Err(ValidationError { errors })
    }
}
