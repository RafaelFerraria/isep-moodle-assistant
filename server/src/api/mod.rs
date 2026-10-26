//! API HTTP local usada pela interface gráfica. Mesmo contrato da versão Python (docs/API.md).
pub mod jobs;
pub mod security;

use crate::config;
use crate::digest::{moodle_dir_of, run_digest, Deps};
use crate::history;
use crate::moodle_sync;
use crate::service;
use crate::system;
use jobs::{Busy, JobManager};
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;
use tiny_http::{Header, Response, Server};

pub const PREFIX: &str = "/api/v1";
const MAX_BODY: usize = 1024 * 1024;

pub struct Req {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub host: Option<String>,
    pub authorization: Option<String>,
    pub body: Vec<u8>,
}

pub struct Resp {
    pub status: u16,
    pub body: Value,
}

fn resp(status: u16, body: Value) -> Resp {
    Resp { status, body }
}
fn detail(status: u16, msg: &str) -> Resp {
    resp(status, json!({ "detail": msg }))
}

pub type DepsFactory = Arc<dyn Fn() -> Deps + Send + Sync>;

pub struct App {
    pub token: String,
    pub jobs: JobManager,
    pub deps: DepsFactory,
}

impl App {
    pub fn new(token: &str) -> App {
        App { token: token.to_string(), jobs: JobManager::new(), deps: Arc::new(Deps::default) }
    }
}

fn parse_query(raw: &str) -> Vec<(String, String)> {
    raw.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            let dec = |s: &str| percent_decode_str(&s.replace('+', " ")).decode_utf8_lossy().into_owned();
            (dec(k), dec(v))
        })
        .collect()
}

fn q<'a>(req: &'a Req, name: &str) -> Option<&'a str> {
    req.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// Parâmetro inteiro com limites (como `Query(ge=, le=)` do FastAPI): 422 se inválido.
fn int_param(req: &Req, name: &str, default: usize, min: usize, max: usize) -> Result<usize, Resp> {
    match q(req, name) {
        None => Ok(default),
        Some(v) => match v.parse::<usize>() {
            Ok(n) if (min..=max).contains(&n) => Ok(n),
            _ => Err(detail(422, &format!("Parâmetro «{name}» inválido (entre {min} e {max})."))),
        },
    }
}

fn body_json(req: &Req) -> Result<Value, Resp> {
    serde_json::from_slice::<Value>(&req.body).map_err(|_| detail(422, "Corpo JSON inválido."))
}

fn busy(b: Busy) -> Resp {
    resp(409, json!({"detail": "Já existe uma tarefa em execução.", "active_job_id": b.0}))
}

/// Trata um pedido. Ordem igual à da versão Python: Host -> rota -> token -> validação.
pub fn handle(app: &App, req: &Req) -> Resp {
    if !security::host_allowed(req.host.as_deref()) {
        return detail(403, "Host não permitido.");
    }
    let Some(path) = req.path.strip_prefix(PREFIX) else { return detail(404, "Not Found") };
    let m = req.method.as_str();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    // (método permitido, requer token?) — None => rota desconhecida
    let known: Option<(&[&str], bool)> = match segments.as_slice() {
        ["health"] => Some((&["GET"], false)),
        ["status"] | ["settings"] | ["history"] | ["jobs"] | ["moodle", "new-files"] => Some((if segments[0] == "settings" { &["GET", "PUT"] } else { &["GET"] }, true)),
        ["summary", "test"] | ["schedule", "register"] | ["digest", "preview"] | ["digest", "run"] | ["moodle", "sync"] => Some((&["POST"], true)),
        ["moodle", "credentials"] => Some((&["PUT"], true)),
        ["jobs", _] | ["history", _] | ["logs", _] => Some((&["GET"], true)),
        _ => None,
    };
    let Some((methods, needs_auth)) = known else { return detail(404, "Not Found") };
    if !methods.contains(&m) {
        return detail(405, "Method Not Allowed");
    }
    if needs_auth && !security::token_matches(security::bearer(req.authorization.as_deref()), &app.token) {
        return detail(401, "Token em falta ou inválido.");
    }

    match (m, segments.as_slice()) {
        ("GET", ["health"]) => resp(200, json!({"ok": true, "version": crate::VERSION})),
        ("GET", ["status"]) => resp(200, service::get_status(app.jobs.active().unwrap_or(Value::Null))),
        ("GET", ["settings"]) => resp(200, service::get_settings()),
        ("PUT", ["settings"]) => {
            let patch = match body_json(req) {
                Ok(v) if v.is_object() => v,
                Ok(_) => return detail(422, "O corpo tem de ser um objeto JSON."),
                Err(e) => return e,
            };
            match service::update_settings(&patch) {
                Ok(v) => resp(200, v),
                Err(e) => resp(422, json!({"detail": "Definições inválidas.", "errors": e.errors})),
            }
        }
        ("POST", ["summary", "test"]) => resp(200, service::test_summary()),
        ("POST", ["schedule", "register"]) => {
            let time = config::public_settings(&config::load_config())["schedule"]["time"].as_str().unwrap_or("18:00").to_string();
            let (ok, message) = system::register_tasks(&time);
            resp(200, json!({"ok": ok, "message": message}))
        }
        ("PUT", ["moodle", "credentials"]) => {
            let body = match body_json(req) {
                Ok(v) => v,
                Err(e) => return e,
            };
            let (Some(user), Some(pass)) = (body["username"].as_str(), body["password"].as_str()) else {
                return detail(422, "username e password são obrigatórios.");
            };
            match service::set_moodle_credentials(user, pass) {
                Ok(username) => resp(200, json!({"ok": true, "username": username})),
                Err(e) => detail(400, &e),
            }
        }
        ("POST", ["digest", kind @ ("preview" | "run")]) => {
            let send = *kind == "run";
            let factory = app.deps.clone();
            let job_kind = if send { "digest" } else { "digest_preview" };
            match app.jobs.submit(job_kind, move |progress| {
                Ok(run_digest(send, "api", Some(&*progress), None, &factory(), true).to_json())
            }) {
                Ok(job) => resp(202, job),
                Err(b) => busy(b),
            }
        }
        ("POST", ["moodle", "sync"]) => {
            match app.jobs.submit("moodle_sync", move |progress| {
                let dir = moodle_dir_of(&config::load_config());
                let sink: moodle_sync::LogSink = Box::new(move |l| progress(l));
                Ok(moodle_sync::run_sync("api", dir.as_deref(), service::moodle_base_url().as_deref(), Some(sink)).to_json())
            }) {
                Ok(job) => resp(202, job),
                Err(b) => busy(b),
            }
        }
        ("GET", ["jobs"]) => match int_param(req, "limit", 10, 1, 30) {
            Ok(limit) => resp(200, json!({"jobs": app.jobs.recent(limit)})),
            Err(e) => e,
        },
        ("GET", ["jobs", id]) => match int_param(req, "since_line", 0, 0, usize::MAX) {
            Ok(since) => match app.jobs.get(id, since) {
                Some(j) => resp(200, j),
                None => detail(404, "Tarefa desconhecida."),
            },
            Err(e) => e,
        },
        ("GET", ["history"]) => {
            let kind = q(req, "kind");
            if let Some(k) = kind {
                if k != "digest" && k != "moodle_sync" {
                    return detail(422, "Parâmetro «kind» inválido.");
                }
            }
            let (limit, offset) = match (int_param(req, "limit", 30, 1, 200), int_param(req, "offset", 0, 0, usize::MAX)) {
                (Ok(l), Ok(o)) => (l, o),
                (Err(e), _) | (_, Err(e)) => return e,
            };
            resp(200, json!({"items": history::list_records(limit, offset, kind, false)}))
        }
        ("GET", ["history", id]) => match history::get(id) {
            Some(r) => resp(200, r),
            None => detail(404, "Registo desconhecido."),
        },
        ("GET", ["moodle", "new-files"]) => match int_param(req, "limit", 100, 1, 500) {
            Ok(limit) => resp(200, json!({"items": service::recent_new_files(limit)})),
            Err(e) => e,
        },
        ("GET", ["logs", name]) => {
            let lines = match int_param(req, "lines", 200, 1, 2000) {
                Ok(n) => n,
                Err(e) => return e,
            };
            match service::tail_log(name, lines) {
                Some(l) => resp(200, json!({"name": name, "lines": l})),
                None => detail(404, "Log desconhecido (notifier ou sync)."),
            }
        }
        _ => detail(404, "Not Found"),
    }
}

// ---------------------------------------------------------------- servidor HTTP

pub struct ApiServer {
    pub server: Arc<Server>,
    pub app: Arc<App>,
    pub port: u16,
}

impl ApiServer {
    /// Escuta só em 127.0.0.1 (`port = 0` escolhe uma porta livre).
    pub fn bind(port: u16, app: App) -> Result<ApiServer, String> {
        let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("não foi possível abrir a porta {port}: {e}"))?;
        let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
        Ok(ApiServer { server: Arc::new(server), app: Arc::new(app), port })
    }

    /// Atende pedidos até `unblock()`; cada pedido numa thread própria.
    pub fn serve(&self) {
        for request in self.server.incoming_requests() {
            let app = self.app.clone();
            std::thread::spawn(move || respond(&app, request));
        }
    }
}

fn respond(app: &App, mut request: tiny_http::Request) {
    fn header(headers: &[Header], name: &'static str) -> Option<String> {
        headers.iter().find(|h| h.field.equiv(name)).map(|h| h.value.to_string())
    }
    let (host, authorization) = (header(request.headers(), "Host"), header(request.headers(), "Authorization"));
    let method = request.method().to_string();
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').map(|(p, q)| (p.to_string(), parse_query(q))).unwrap_or((url.clone(), vec![]));

    let mut body = Vec::new();
    let too_big = {
        let mut limited = request.as_reader().take(MAX_BODY as u64 + 1);
        let _ = limited.read_to_end(&mut body);
        body.len() > MAX_BODY
    };
    let out = if too_big {
        detail(413, "Corpo demasiado grande.")
    } else {
        let req = Req { method, path: urldecode_path(&path), query, host, authorization, body };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(app, &req))).unwrap_or_else(|_| detail(500, "Erro interno."))
    };
    let mut response = Response::from_string(out.body.to_string()).with_status_code(out.status);
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    // sem CORS de propósito: nenhuma outra origem consegue ler estas respostas
    let _ = request.respond(response);
}

fn urldecode_path(p: &str) -> String {
    percent_decode_str(p).decode_utf8_lossy().into_owned()
}
