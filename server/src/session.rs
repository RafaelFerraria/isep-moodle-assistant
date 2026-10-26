//! Envio de mensagens para o Session (via session_sender.cjs, Node.js).
use crate::config::recipients_of;
use crate::paths;
use crate::util::{run_with_timeout, str_of, which, Json};
use std::process::Command;
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
    let Some(node) = which("node") else {
        log("ERRO: Node.js não encontrado no PATH (necessário para enviar para o Session).");
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
        let mut cmd = Command::new(&node);
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
