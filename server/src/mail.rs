//! Leitura dos emails do ISEP a partir da caixa de entrada local do Mozilla Thunderbird (ficheiro mbox).
use crate::model::Email;
use crate::system::find_thunderbird_path;
use crate::util::{hide_window, str_or, truncate_chars, Json};
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use mail_parser::MessageParser;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const BODY_LIMIT: usize = 4000;

#[derive(Clone, Copy)]
pub struct FetchOptions {
    /// Esperar pela rede e deixar o Thunderbird sincronizar antes de ler.
    pub sync: bool,
}

impl Default for FetchOptions {
    fn default() -> Self {
        FetchOptions { sync: true }
    }
}

// ---------------------------------------------------------------- rede e sincronização do Thunderbird

pub fn wait_for_internet(max_wait: Duration) -> bool {
    let start = Instant::now();
    let addr: SocketAddr = "1.1.1.1:53".parse().unwrap();
    while start.elapsed() < max_wait {
        if TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    false
}

pub fn profiles_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("ISEP_THUNDERBIRD_PROFILES") {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Thunderbird").join("Profiles")
}

pub fn find_thunderbird_inboxes() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 8 {
                walk(&path, out, depth + 1);
            } else if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_uppercase();
                if name == "INBOX" || name == "ENTRADA" {
                    out.push(path);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&profiles_dir(), &mut out, 0);
    out
}

fn is_thunderbird_running() -> bool {
    let mut cmd = Command::new("tasklist");
    cmd.args(["/FI", "IMAGENAME eq thunderbird.exe", "/NH"]).stdin(Stdio::null());
    hide_window(&mut cmd);
    cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("thunderbird.exe")).unwrap_or(false)
}

type Snapshot = Vec<(PathBuf, Option<SystemTime>, u64)>;

fn snapshot() -> Snapshot {
    find_thunderbird_inboxes()
        .into_iter()
        .map(|p| {
            let md = std::fs::metadata(&p).ok();
            (p, md.as_ref().and_then(|m| m.modified().ok()), md.map(|m| m.len()).unwrap_or(0))
        })
        .collect()
}

/// Espera pelo menos `min_wait` e depois até a caixa deixar de mudar durante `quiet` (a sincronização IMAP
/// depois de acordar da suspensão pode demorar bem mais do que um tempo fixo).
fn wait_for_inbox_to_settle(min_wait: Duration, quiet: Duration, max_wait: Duration) {
    let start = Instant::now();
    std::thread::sleep(min_wait);
    let mut last = snapshot();
    let mut last_change = Instant::now();
    while start.elapsed() < max_wait {
        std::thread::sleep(Duration::from_secs(3));
        let cur = snapshot();
        if cur != last {
            last = cur;
            last_change = Instant::now();
        } else if last_change.elapsed() >= quiet {
            return;
        }
    }
}

/// Se o Thunderbird não estiver aberto, arranca-o em modo headless para descarregar os emails novos.
pub fn sync_thunderbird_headless(timeout: Duration) -> bool {
    let Some(tb) = find_thunderbird_path() else { return false };
    if is_thunderbird_running() {
        wait_for_inbox_to_settle(Duration::from_secs(3), Duration::from_secs(20), Duration::from_secs(180));
        return true;
    }
    let mut cmd = Command::new(tb);
    cmd.arg("--headless").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    hide_window(&mut cmd);
    let Ok(mut child) = cmd.spawn() else { return false };
    wait_for_inbox_to_settle(timeout, Duration::from_secs(20), Duration::from_secs(180));
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_secs(2));
    true
}

// ---------------------------------------------------------------- mbox

fn header_value(headers: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(headers);
    let mut value: Option<String> = None;
    for line in text.lines() {
        if let Some(v) = value.as_mut() {
            if line.starts_with(' ') || line.starts_with('\t') {
                v.push(' ');
                v.push_str(line.trim());
                continue;
            }
            break;
        }
        if line.len() > name.len() && line.is_char_boundary(name.len()) && line[..name.len()].eq_ignore_ascii_case(name) && line[name.len()..].starts_with(':') {
            value = Some(line[name.len() + 1..].trim().to_string());
        }
    }
    value
}

/// Data do email em hora local. Sem Date válido, assume "agora" (como a versão Python).
fn message_time(raw_date: Option<&str>) -> NaiveDateTime {
    let now = Local::now().naive_local();
    let Some(raw) = raw_date else { return now };
    if let Ok(dt) = DateTime::parse_from_rfc2822(raw) {
        return dt.with_timezone(&Local).naive_local();
    }
    mail_parser::DateTime::parse_rfc822(raw)
        .and_then(|d| DateTime::<Utc>::from_timestamp(d.to_timestamp(), 0))
        .map(|d| d.with_timezone(&Local).naive_local())
        .unwrap_or(now)
}

fn display_address(msg: &mail_parser::Message) -> String {
    let Some(addr) = msg.from().and_then(|a| a.first()) else { return "Desconhecido".into() };
    match (addr.name(), addr.address()) {
        (Some(n), Some(a)) => format!("\"{n}\" <{a}>"),
        (None, Some(a)) => a.to_string(),
        (Some(n), None) => n.to_string(),
        _ => "Desconhecido".into(),
    }
}

fn to_email(raw: &[u8], received: NaiveDateTime) -> Option<Email> {
    let msg = MessageParser::default().parse(raw)?;
    let sender = display_address(&msg);
    let body = msg.body_text(0).map(|b| b.into_owned()).unwrap_or_default();
    Some(Email {
        subject: msg.subject().filter(|s| !s.is_empty()).unwrap_or("Sem assunto").to_string(),
        sender_email: sender.clone(),
        sender,
        received_time: received.format("%Y-%m-%dT%H:%M:%S").to_string(),
        body: truncate_chars(&body, BODY_LIMIT),
        message_id: msg.message_id().map(|id| format!("<{id}>")).unwrap_or_default(),
    })
}

/// Lê um ficheiro mbox e devolve os emails recebidos depois de `since` (mais recentes primeiro).
pub fn read_mbox(path: &Path, since: NaiveDateTime) -> std::io::Result<Vec<Email>> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut emails: Vec<(NaiveDateTime, Email)> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut previous_blank = true;
    let mut line = Vec::new();

    let flush = |current: &mut Vec<u8>, emails: &mut Vec<(NaiveDateTime, Email)>| {
        if current.is_empty() {
            return;
        }
        let header_end = current.windows(4).position(|w| w == b"\r\n\r\n").or_else(|| current.windows(2).position(|w| w == b"\n\n")).unwrap_or(current.len());
        let time = message_time(header_value(&current[..header_end], "Date").as_deref());
        if time >= since {
            if let Some(e) = to_email(current, time) {
                emails.push((time, e));
            }
        }
        current.clear();
    };

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.starts_with(b"From ") && previous_blank {
            flush(&mut current, &mut emails);
            previous_blank = false;
            continue; // a linha "From " é só o separador do mbox
        }
        previous_blank = line == b"\n" || line == b"\r\n";
        current.extend_from_slice(&line);
    }
    flush(&mut current, &mut emails);

    emails.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(emails.into_iter().map(|(_, e)| e).collect())
}

pub fn fetch_emails_thunderbird(since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String> {
    if opts.sync {
        if !wait_for_internet(Duration::from_secs(30)) {
            return Err("Sem ligação à Internet. A abortar sem avançar a data de última execução.".into());
        }
        sync_thunderbird_headless(Duration::from_secs(25));
    }
    let mut inboxes = find_thunderbird_inboxes();
    if inboxes.is_empty() {
        return Err("Caixa de entrada do Thunderbird não encontrada.\nPor favor abre o Thunderbird, adiciona a tua conta institucional e aguarda 1 minuto até os emails sincronizarem.".into());
    }
    // a caixa mais recentemente modificada
    inboxes.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    read_mbox(&inboxes[0], since).map_err(|e| format!("Não foi possível ler a caixa de entrada: {e}"))
}

// ---------------------------------------------------------------- origens de email

/// De onde vêm os emails. Cada origem é independente do resto da app: o resumo, a página de emails e a API só
/// conhecem esta interface, por isso trocar o Thunderbird pelo serviço próprio (ou, na nuvem, por um serviço
/// do servidor) não toca em mais nada.
pub trait MailSource {
    fn id(&self) -> &'static str;
    /// Emails recebidos depois de `since`, mais recentes primeiro.
    fn fetch(&self, config: &Json, since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String>;
}

pub struct ThunderbirdSource;
impl MailSource for ThunderbirdSource {
    fn id(&self) -> &'static str {
        "thunderbird"
    }
    fn fetch(&self, _config: &Json, since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String> {
        fetch_emails_thunderbird(since, opts)
    }
}

/// Serviço próprio da app: Microsoft Graph com início de sessão por código de dispositivo.
pub struct GraphSource;
impl MailSource for GraphSource {
    fn id(&self) -> &'static str {
        "graph"
    }
    fn fetch(&self, config: &Json, since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String> {
        if opts.sync && !wait_for_internet(Duration::from_secs(30)) {
            return Err("Sem ligação à Internet. A abortar sem avançar a data de última execução.".into());
        }
        let (emails, rotated) = crate::graph::fetch_inbox(config, since)?;
        if let Some(token) = rotated {
            // a Microsoft roda o token de renovação: grava o novo para a próxima execução
            let mut fresh = crate::config::load_config();
            if crate::graph::store_refresh_token(&mut fresh, &token).is_ok() {
                let _ = crate::config::save_config(&fresh);
            }
        }
        Ok(emails)
    }
}

/// Nomes aceites em `email_method` (`outlook` é o nome antigo da versão Python para o mesmo serviço).
pub fn source_for(method: &str) -> Result<Box<dyn MailSource>, String> {
    match method.to_lowercase().as_str() {
        "thunderbird" => Ok(Box::new(ThunderbirdSource)),
        "graph" | "outlook" => Ok(Box::new(GraphSource)),
        other => Err(format!("O método de email '{other}' ainda não está disponível na versão Rust (usa 'thunderbird' ou 'graph').")),
    }
}

/// Escolhe o método de leitura configurado.
pub fn get_emails(config: &Json, since: NaiveDateTime, opts: FetchOptions) -> Result<Vec<Email>, String> {
    source_for(str_or(config, "email_method", "thunderbird"))?.fetch(config, since, opts)
}
