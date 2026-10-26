//! Utilitários partilhados: JSON tolerante, escrita atómica, execução de processos com limite de tempo.
use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub type Json = Map<String, Value>;

/// Lê um objeto JSON tolerando BOM UTF-8; devolve um objeto vazio se faltar ou estiver inválido.
pub fn read_json(path: &Path) -> Json {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let text = text.trim_start_matches('\u{feff}');
            match serde_json::from_str::<Value>(text) {
                Ok(Value::Object(map)) => map,
                _ => Json::new(),
            }
        }
        Err(_) => Json::new(),
    }
}

/// Escreve para um ficheiro temporário e substitui (nunca deixa o ficheiro a meio).
pub fn write_json_atomic(path: &Path, data: &Json) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, serde_json::to_string_pretty(data)?)?;
    std::fs::rename(&tmp, path)
}

pub fn str_of<'a>(map: &'a Json, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Texto de um campo ou o valor por omissão se estiver vazio/ausente.
pub fn str_or<'a>(map: &'a Json, key: &str, default: &'a str) -> &'a str {
    let v = str_of(map, key);
    if v.is_empty() {
        default
    } else {
        v
    }
}

/// Resolve um executável no PATH (com PATHEXT no Windows: .exe, .cmd, .bat…).
pub fn which(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return candidate.exists().then(|| candidate.to_path_buf());
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let direct = dir.join(name);
        if direct.is_file() && Path::new(name).extension().is_some() {
            return Some(direct);
        }
        for ext in &exts {
            let full = dir.join(format!("{name}{ext}"));
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

pub struct ProcOutput {
    pub status: Option<i32>, // None se terminou por sinal
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Corre um comando sem janela, envia `input` por stdin e espera no máximo `timeout`.
pub fn run_with_timeout(mut cmd: Command, input: Option<&str>, timeout: Duration) -> std::io::Result<ProcOutput> {
    hide_window(&mut cmd);
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;

    if let (Some(text), Some(mut stdin)) = (input.map(str::to_owned), child.stdin.take()) {
        thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let (tx_out, rx_out) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();
    if let Some(mut out) = child.stdout.take() {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            let _ = tx_out.send(buf);
        });
    }
    if let Some(mut err) = child.stderr.take() {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            let _ = tx_err.send(buf);
        });
    }

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(s) => break s.code(),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let collect = |rx: mpsc::Receiver<Vec<u8>>| {
        rx.recv_timeout(Duration::from_secs(2))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    };
    Ok(ProcOutput { status, stdout: collect(rx_out), stderr: collect(rx_err), timed_out })
}

#[cfg(windows)]
pub fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
pub fn hide_window(_cmd: &mut Command) {}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Trunca por caracteres (não por bytes) — o texto tem acentos.
pub fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
