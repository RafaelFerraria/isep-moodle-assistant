//! Estado da sincronização do Moodle em curso (ou da última): ficheiros pendentes, a descarregar, feitos e com erro.
//!
//! O sincronizador escreve `sync_progress.json` a cada mudança e a API lê-o. O cliente HTTP (WinHTTP) devolve cada
//! ficheiro inteiro, por isso o progresso é por ficheiro e por disciplina, não em bytes.
use crate::paths;
use crate::util::{now_iso, read_json, write_json_atomic};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MAX_ITEMS: usize = 300;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Pending,
    Downloading,
    Done,
    Unchanged,
    Error,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Downloading => "downloading",
            State::Done => "done",
            State::Unchanged => "unchanged",
            State::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub key: String,
    pub course: String,
    pub section: String,
    pub name: String,
    pub state: State,
    pub size: u64,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Progress {
    sync_dir: Option<PathBuf>,
    running: bool,
    started: String,
    finished: Option<String>,
    courses_total: usize,
    courses_done: usize,
    current_course: String,
    items: Vec<Item>,
}

impl Progress {
    pub fn new(sync_dir: Option<&Path>) -> Progress {
        Progress { sync_dir: sync_dir.map(Path::to_path_buf), ..Default::default() }
    }

    pub fn start(&mut self) {
        self.running = true;
        self.started = now_iso();
        self.finished = None;
        self.courses_total = 0;
        self.courses_done = 0;
        self.current_course.clear();
        self.items.clear();
        self.flush();
    }

    pub fn set_courses(&mut self, total: usize) {
        self.courses_total = total;
        self.flush();
    }

    pub fn begin_course(&mut self, name: &str) {
        self.current_course = name.to_string();
        self.flush();
    }

    pub fn end_course(&mut self) {
        self.courses_done += 1;
        self.flush();
    }

    pub fn finish(&mut self) {
        self.running = false;
        self.finished = Some(now_iso());
        self.current_course.clear();
        // o que ficou por fazer (sincronização abortada) não deve parecer ainda em curso
        for it in self.items.iter_mut().filter(|i| i.state == State::Downloading) {
            it.state = State::Pending;
        }
        self.flush();
    }

    /// Regista um ficheiro como pendente (conhecido mas ainda não descarregado).
    pub fn queue(&mut self, key: &Path, course: &str, section: &str, name: &str) {
        let key = key.to_string_lossy().into_owned();
        if self.items.iter().any(|i| i.key == key) {
            return;
        }
        self.items.push(Item { key, course: course.into(), section: section.into(), name: name.into(), state: State::Pending, size: 0, error: None });
        self.trim();
        self.flush();
    }

    /// Atualiza (ou cria) o ficheiro identificado por `key`.
    pub fn update(&mut self, key: &Path, course: &str, section: &str, name: &str, state: State, size: u64, error: Option<&str>) {
        let key = key.to_string_lossy().into_owned();
        match self.items.iter_mut().find(|i| i.key == key) {
            Some(it) => {
                it.state = state;
                it.size = size;
                it.error = error.map(str::to_owned);
                if !name.is_empty() {
                    it.name = name.to_string();
                }
            }
            None => {
                self.items.push(Item { key, course: course.into(), section: section.into(), name: name.into(), state, size, error: error.map(str::to_owned) });
                self.trim();
            }
        }
        self.flush();
    }

    /// Mantém o ficheiro de estado pequeno: descarta primeiro os já concluídos mais antigos.
    fn trim(&mut self) {
        while self.items.len() > MAX_ITEMS {
            match self.items.iter().position(|i| matches!(i.state, State::Done | State::Unchanged)) {
                Some(pos) => {
                    self.items.remove(pos);
                }
                None => {
                    self.items.remove(0);
                }
            }
        }
    }

    pub fn to_json(&self) -> Value {
        render(self.running, &self.started, self.finished.as_deref(), self.courses_total, self.courses_done, &self.current_course, &self.items)
    }

    fn flush(&self) {
        let Value::Object(map) = self.to_json() else { return };
        let _ = write_json_atomic(&paths::sync_progress(self.sync_dir.as_deref()), &map);
    }
}

fn render(running: bool, started: &str, finished: Option<&str>, total: usize, done: usize, current: &str, items: &[Item]) -> Value {
    let count = |s: State| items.iter().filter(|i| i.state == s).count();
    let (pending, downloading, ok, unchanged, error) =
        (count(State::Pending), count(State::Downloading), count(State::Done), count(State::Unchanged), count(State::Error));
    // percentagem por disciplina (mais fiável do que por ficheiro, que só se conhecem à medida que a disciplina é lida)
    let percent = if total == 0 { 0 } else { ((done * 100) / total).min(100) };
    json!({
        "running": running,
        "started": started,
        "finished": finished,
        "courses_total": total,
        "courses_done": done,
        "current_course": current,
        "percent": percent,
        "counts": {"pending": pending, "downloading": downloading, "done": ok, "unchanged": unchanged, "error": error},
        "items": items.iter().map(|i| json!({
            "course": i.course, "section": i.section, "name": i.name, "state": i.state.as_str(), "size": i.size, "error": i.error,
        })).collect::<Vec<_>>(),
    })
}

/// Lê o estado gravado. Se o processo que o escreveu morreu a meio (sem `sync.lock` recente), deixa de contar como em curso.
pub fn load(sync_dir: Option<&Path>) -> Value {
    let mut state = Value::Object(read_json(&paths::sync_progress(sync_dir)));
    if state.get("running").and_then(Value::as_bool).unwrap_or(false) {
        let lock = paths::sync_lock(sync_dir);
        let fresh = std::fs::metadata(&lock).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age.as_secs() < 3 * 3600).unwrap_or(false);
        if !fresh {
            state["running"] = json!(false);
        }
    }
    if state.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return render(false, "", None, 0, 0, "", &[]);
    }
    state
}
