//! Localização dos ficheiros de dados (mesmo layout da versão Python, para as instalações existentes).
//!
//! Tudo é resolvido em tempo de chamada para poder ser redirecionado nos testes com
//! `ISEP_ASSISTANT_ROOT` (dados) e `ISEP_ASSISTANT_STATE` (porta/token da API).
use std::env;
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    if let Some(v) = env::var_os("ISEP_ASSISTANT_ROOT") {
        return PathBuf::from(v);
    }
    // Em desenvolvimento o executável está em server/target/<perfil>/: sobe até achar EmailNotifier/ e MoodleSync/
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = env::current_dir() {
        starts.push(cwd);
    }
    for start in starts {
        for dir in start.ancestors() {
            if dir.join("EmailNotifier").is_dir() && dir.join("MoodleSync").is_dir() {
                return dir.to_path_buf();
            }
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn email_dir() -> PathBuf {
    root().join("EmailNotifier")
}
pub fn sync_dir() -> PathBuf {
    root().join("MoodleSync")
}
pub fn email_config() -> PathBuf {
    email_dir().join("config.json")
}
pub fn last_run_file() -> PathBuf {
    email_dir().join("last_run.json")
}
pub fn notifier_log() -> PathBuf {
    email_dir().join("notifier.log")
}
pub fn history_file() -> PathBuf {
    email_dir().join("history.jsonl")
}
pub fn session_script() -> PathBuf {
    email_dir().join("session_sender.cjs")
}
/// Programa e argumentos que o Agendador de Tarefas executa: este mesmo executável, com a raiz de dados fixada
/// (a tarefa não herda o ambiente nem a pasta de trabalho da app).
pub fn digest_command() -> (PathBuf, String) {
    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("isep-assistant-api.exe"));
    (exe, format!("digest --root \"{}\"", root().display()))
}

fn or_default(dir: Option<&Path>) -> PathBuf {
    dir.map(Path::to_path_buf).unwrap_or_else(sync_dir)
}
pub fn sync_config(dir: Option<&Path>) -> PathBuf {
    or_default(dir).join("config.json")
}
pub fn sync_log(dir: Option<&Path>) -> PathBuf {
    or_default(dir).join("sync_log.txt")
}
pub fn new_files_log(dir: Option<&Path>) -> PathBuf {
    or_default(dir).join("new_files.jsonl")
}
pub fn sync_progress(dir: Option<&Path>) -> PathBuf {
    or_default(dir).join("sync_progress.json")
}
pub fn sync_lock(dir: Option<&Path>) -> PathBuf {
    or_default(dir).join("sync.lock")
}

pub fn default_download_dir() -> PathBuf {
    home_dir().join("Downloads").join("Moodle_ISEP")
}

pub fn home_dir() -> PathBuf {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Pasta com a porta e o token da API local (fora do repositório).
pub fn state_dir() -> PathBuf {
    if let Some(v) = env::var_os("ISEP_ASSISTANT_STATE") {
        return PathBuf::from(v);
    }
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"));
    base.join("ISEPAssistant")
}

pub fn api_state_file() -> PathBuf {
    state_dir().join("api.json")
}
