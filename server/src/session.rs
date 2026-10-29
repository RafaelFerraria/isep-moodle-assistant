//! Envio de mensagens para o Session (via session_sender.cjs, Node.js).
use crate::config::recipients_of;
use crate::paths;
use crate::system::{node_runtime, NodeRuntime};
use crate::util::{run_with_timeout, str_of, Json};
use std::time::Duration;

const SEND_TIMEOUT: Duration = Duration::from_secs(120);

/// Envia `text` para todos os destinatários configurados; `true` só se todos receberam.
pub fn send_to_session(text: &str, config: &Json, log: &dyn Fn(&str)) -> bool {
    let recipients = recipients_of(config);
    let mnemonic = str_of(config, "session_mnemonic");
    if recipients.is_empty() || mnemonic.is_empty() {
        log("ERRO: Nenhum destinatário ou session_mnemonic não configurados no config.json!");
        return false;
    }
    let Some(node) = node_runtime() else {
        log("ERRO: não foi encontrado o runtime do Session (Node.js).");
        return false;
    };

    let dir = paths::email_dir();
    let _ = std::fs::create_dir_all(&dir);
    let temp = dir.join(format!("_msg_{}.txt", crate::history::new_id()));
    if let Err(e) = std::fs::write(&temp, text) {
        log(&format!("ERRO ao preparar a mensagem: {e}"));
        return false;
    }

    let mut all_ok = true;
    for target in &recipients {
        let short: String = target.chars().take(10).collect();
        log(&format!("A enviar resumo para o Session (Destino: {short}...)..."));
        let mut cmd = node.command();
        cmd.arg(paths::session_script())
            .args(["--to", target, "--file"])
            .arg(&temp)
            .args(["--mnemonic", mnemonic]);
        match run_with_timeout(cmd, None, SEND_TIMEOUT) {
            Ok(out) if out.status == Some(0) && !out.timed_out => log(&format!("Mensagem Session enviada com sucesso para {short}...!")),
            Ok(out) => {
                // a mnemónica vai na linha de comandos: nunca registar o comando, só a saída do processo
                let detail = if out.timed_out { "tempo limite".to_string() } else { format!("{}{}", out.stderr, out.stdout) };
                log(&format!("FALHA ao enviar para {short}...: {}", detail.trim().chars().take(300).collect::<String>()));
                all_ok = false;
            }
            Err(e) => {
                log(&format!("FALHA ao enviar para {short}...: {}", e.kind()));
                all_ok = false;
            }
        }
    }
    let _ = std::fs::remove_file(&temp);
    all_ok
}

// ---------------------------------------------------------------- configuração automática do bot

use crate::config::{load_config, mask_session_id, save_config};
use crate::util::now_iso;
use serde_json::{json, Value};

/// Estado da página do Session: o bot existe? há destinatários? o Node.js e o script estão disponíveis?
pub fn state(config: &Json) -> Value {
    let recipients = recipients_of(config);
    json!({
        "node_ok": node_runtime().is_some(),
        "script_ok": paths::session_script().exists(),
        "bot_created": !str_of(config, "session_mnemonic").is_empty(),
        "bot_id": str_of(config, "session_bot_id"),
        "recipients": recipients.iter().map(|r| mask_session_id(r)).collect::<Vec<_>>(),
        "configured": !recipients.is_empty() && !str_of(config, "session_mnemonic").is_empty(),
    })
}

fn require_sender() -> Result<NodeRuntime, String> {
    let node = node_runtime().ok_or("Não foi encontrado o Node.js (necessário para o Session). Instala-o em nodejs.org ou reinstala a app.")?;
    if !paths::session_script().exists() {
        return Err("O ficheiro session_sender.cjs não foi encontrado.".into());
    }
    Ok(node)
}

/// Cria a conta do bot (a frase secreta nunca sai do `config.json`). Sem `force`, mantém o bot que já existe.
pub fn create_bot(force: bool) -> Result<Value, String> {
    let mut config = load_config();
    if !force && !str_of(&config, "session_mnemonic").is_empty() {
        return Ok(state(&config));
    }
    let node = require_sender()?;
    let mut cmd = node.command();
    cmd.arg(paths::session_script()).arg("--generate");
    let out = run_with_timeout(cmd, None, Duration::from_secs(60)).map_err(|e| format!("não foi possível correr o Node.js: {e}"))?;
    if out.timed_out || out.status != Some(0) {
        return Err(format!("Falha ao criar o bot do Session: {}", out.stderr.trim().chars().take(200).collect::<String>()));
    }
    let bot: Value = serde_json::from_str(out.stdout.trim().lines().last().unwrap_or("")).map_err(|_| "Resposta inesperada ao criar o bot.".to_string())?;
    let (Some(mnemonic), Some(id)) = (bot["mnemonic"].as_str(), bot["sessionId"].as_str()) else {
        return Err("Resposta inesperada ao criar o bot.".into());
    };
    config.insert("session_mnemonic".into(), json!(mnemonic));
    config.insert("session_bot_id".into(), json!(id));
    // um bot novo é outra conta: os destinatários antigos nunca falaram com ele
    config.remove("session_recipients");
    config.remove("session_group_id");
    save_config(&config).map_err(|e| format!("não foi possível gravar a configuração: {e}"))?;
    Ok(state(&config))
}

/// Espera (até `timeout`) pela primeira mensagem privada para o bot e guarda quem a enviou como destinatário.
pub fn pair(timeout: Duration, log: &dyn Fn(&str)) -> Result<Value, String> {
    let config = load_config();
    let mnemonic = str_of(&config, "session_mnemonic").to_string();
    if mnemonic.is_empty() {
        return Err("Cria primeiro o bot do Session.".into());
    }
    let node = require_sender()?;
    log(&format!("À espera da tua mensagem para o bot ({}).", now_iso()));
    let mut cmd = node.command();
    cmd.arg(paths::session_script()).args(["--listen", "--mnemonic", &mnemonic, "--timeout", &timeout.as_secs().to_string()]);
    let out = run_with_timeout(cmd, None, timeout + Duration::from_secs(30)).map_err(|e| format!("não foi possível correr o Node.js: {e}"))?;
    let reply: Value = serde_json::from_str(out.stdout.trim().lines().last().unwrap_or("")).unwrap_or(Value::Null);
    if reply["timeout"].as_bool().unwrap_or(false) || out.timed_out {
        return Err("Não recebi nenhuma mensagem a tempo. Envia «Olá» ao bot e tenta de novo.".into());
    }
    let Some(id) = reply["sessionId"].as_str().filter(|s| crate::config::valid_session_id(s)) else {
        return Err(format!("O Session não devolveu um ID válido. {}", out.stderr.trim().chars().take(160).collect::<String>()));
    };
    add_recipient(id)?;
    log("Mensagem recebida: destinatário guardado.");
    Ok(json!({"session_id": mask_session_id(id)}))
}

pub fn add_recipient(id: &str) -> Result<(), String> {
    let id = id.trim();
    if !crate::config::valid_session_id(id) {
        return Err("O Session ID deve ter 66 caracteres e começar por 05.".into());
    }
    let mut config = load_config();
    let mut recipients = recipients_of(&config);
    if !recipients.iter().any(|r| r == id) {
        recipients.push(id.to_string());
    }
    config.insert("session_group_id".into(), json!(recipients[0]));
    config.insert("session_recipients".into(), json!(recipients));
    save_config(&config).map_err(|e| format!("não foi possível gravar a configuração: {e}"))
}

pub fn remove_recipient(index: usize) -> Result<(), String> {
    let mut config = load_config();
    let mut recipients = recipients_of(&config);
    if index >= recipients.len() {
        return Err("Destinatário desconhecido.".into());
    }
    recipients.remove(index);
    match recipients.first() {
        Some(first) => config.insert("session_group_id".into(), json!(first)),
        None => config.remove("session_group_id"),
    };
    config.insert("session_recipients".into(), json!(recipients));
    save_config(&config).map_err(|e| format!("não foi possível gravar a configuração: {e}"))
}

/// Envia uma mensagem de teste para os destinatários.
pub fn send_test(log: &dyn Fn(&str)) -> bool {
    send_to_session("✅ ISEP Assistant: o envio para o Session está a funcionar.", &load_config(), log)
}

/// Cria uma conta de Session nova para o utilizador (para quem ainda não tem). A frase de recuperação é devolvida UMA vez,
/// para ele a guardar e restaurar a conta na app do Session; a app nunca a guarda, só o ID, que passa a ser destinatário.
pub fn create_account() -> Result<Value, String> {
    let node = require_sender()?;
    let mut cmd = node.command();
    cmd.arg(paths::session_script()).arg("--generate");
    let out = run_with_timeout(cmd, None, Duration::from_secs(60)).map_err(|e| format!("não foi possível correr o Node.js: {e}"))?;
    if out.timed_out || out.status != Some(0) {
        return Err(format!("Falha ao criar a conta do Session: {}", out.stderr.trim().chars().take(200).collect::<String>()));
    }
    let acc: Value = serde_json::from_str(out.stdout.trim().lines().last().unwrap_or("")).map_err(|_| "Resposta inesperada ao criar a conta.".to_string())?;
    let (Some(mnemonic), Some(id)) = (acc["mnemonic"].as_str(), acc["sessionId"].as_str()) else {
        return Err("Resposta inesperada ao criar a conta.".into());
    };
    add_recipient(id)?;
    Ok(json!({"session_id": id, "mnemonic": mnemonic, "state": state(&load_config())}))
}
