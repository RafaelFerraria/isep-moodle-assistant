#![allow(dead_code)]
//! Ajudantes dos testes: raiz de dados isolada e servidores HTTP falsos.
use serde_json::Value;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use tiny_http::{Header, Response, Server};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Raiz de dados temporária (EmailNotifier/ e MoodleSync/ vazios). Mantém um lock global: as variáveis de
/// ambiente são do processo, por isso os testes que as usam correm um de cada vez.
pub struct Home {
    pub dir: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl Home {
    pub fn new() -> Home {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("EmailNotifier")).unwrap();
        std::fs::create_dir_all(dir.path().join("MoodleSync")).unwrap();
        std::env::set_var("ISEP_ASSISTANT_ROOT", dir.path());
        std::env::set_var("ISEP_ASSISTANT_STATE", dir.path().join("state"));
        std::env::set_var("ISEP_FAKE_SCHEDULER", "1"); // nunca tocar no Agendador de Tarefas real
        std::env::remove_var("ISEP_MOODLE_BASE_URL");
        for var in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "AI_API_KEY"] {
            std::env::remove_var(var);
        }
        Home { dir, _guard: guard }
    }
    pub fn path(&self, rel: &str) -> std::path::PathBuf {
        self.dir.path().join(rel)
    }
}

pub fn json_map(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().expect("objeto JSON")
}

/// Um pedido recebido pelo servidor falso.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// O que o servidor falso responde.
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Reply {
    pub fn json(status: u16, v: Value) -> Reply {
        Reply { status, body: v.to_string().into_bytes(), headers: vec![("Content-Type".into(), "application/json".into())] }
    }
    pub fn text(status: u16, t: &str) -> Reply {
        Reply { status, body: t.as_bytes().to_vec(), headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())] }
    }
    pub fn bytes(status: u16, b: &[u8], headers: &[(&str, &str)]) -> Reply {
        Reply { status, body: b.to_vec(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }
    pub fn with(mut self, k: &str, v: &str) -> Reply {
        self.headers.push((k.into(), v.into()));
        self
    }
}

pub struct FakeServer {
    pub base: String,
    pub seen: std::sync::Arc<Mutex<Vec<Seen>>>,
    server: std::sync::Arc<Server>,
}

impl FakeServer {
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

/// Servidor HTTP local que responde com `handler` e guarda os pedidos.
pub fn fake_server<F>(handler: F) -> FakeServer
where
    F: Fn(&Seen) -> Reply + Send + 'static,
{
    let server = std::sync::Arc::new(Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let (s2, seen2) = (server.clone(), seen.clone());
    thread::spawn(move || {
        for mut req in s2.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let rec = Seen {
                method: req.method().to_string(),
                url: req.url().to_string(),
                headers: req.headers().iter().map(|h| (h.field.to_string(), h.value.to_string())).collect(),
                body,
            };
            seen2.lock().unwrap().push(rec.clone());
            let reply = handler(&rec);
            let mut resp = Response::from_data(reply.body).with_status_code(reply.status);
            for (k, v) in reply.headers {
                resp.add_header(Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
            }
            let _ = req.respond(resp);
        }
    });
    FakeServer { base: format!("http://127.0.0.1:{port}"), seen, server }
}

