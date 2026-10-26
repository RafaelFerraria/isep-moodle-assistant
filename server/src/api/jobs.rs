//! Tarefas longas (resumo, sincronização) executadas em segundo plano, uma de cada vez.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const MAX_PROGRESS_LINES: usize = 300;
const MAX_JOBS_KEPT: usize = 30;

#[derive(Clone)]
struct Job {
    id: String,
    kind: String,
    status: &'static str, // running | done | failed
    started: String,
    finished: Option<String>,
    progress: Vec<String>,
    result: Option<Value>,
    error: Option<String>,
}

impl Job {
    fn to_json(&self, since_line: usize) -> Value {
        json!({
            "id": self.id, "kind": self.kind, "status": self.status, "started": self.started, "finished": self.finished,
            "progress": self.progress.iter().skip(since_line).collect::<Vec<_>>(),
            "progress_total": self.progress.len(), "result": self.result, "error": self.error,
        })
    }
}

#[derive(Default)]
struct Inner {
    jobs: HashMap<String, Job>,
    order: Vec<String>,
}

/// Função de progresso partilhável entre threads (cada linha de log chega à UI).
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;

/// Já existe uma tarefa em execução (devolve o id).
#[derive(Debug)]
pub struct Busy(pub String);

#[derive(Clone, Default)]
pub struct JobManager {
    inner: Arc<Mutex<Inner>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner.jobs.values().find(|j| j.status == "running").map(|j| j.to_json(0))
    }

    /// Arranca `work(progress)` numa thread. Só uma tarefa de cada vez (`Busy` se já houver uma).
    pub fn submit<F>(&self, kind: &str, work: F) -> Result<Value, Busy>
    where
        F: FnOnce(Progress) -> Result<Value, String> + Send + 'static,
    {
        let id = crate::history::new_id();
        let snapshot;
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(running) = inner.jobs.values().find(|j| j.status == "running") {
                return Err(Busy(running.id.clone()));
            }
            let job = Job {
                id: id.clone(), kind: kind.to_string(), status: "running", started: crate::util::now_iso(),
                finished: None, progress: vec![], result: None, error: None,
            };
            snapshot = job.to_json(0);
            inner.jobs.insert(id.clone(), job);
            inner.order.push(id.clone());
            while inner.order.len() > MAX_JOBS_KEPT {
                let old = inner.order.remove(0);
                inner.jobs.remove(&old);
            }
        }
        let shared = self.inner.clone();
        std::thread::spawn(move || {
            let (progress_shared, progress_id) = (shared.clone(), id.clone());
            let progress: Progress = Arc::new(move |line: &str| {
                if let Some(job) = progress_shared.lock().unwrap().jobs.get_mut(&progress_id) {
                    job.progress.push(line.to_string());
                    if job.progress.len() > MAX_PROGRESS_LINES {
                        let excess = job.progress.len() - MAX_PROGRESS_LINES;
                        job.progress.drain(..excess);
                    }
                }
            });
            // um pânico na tarefa não pode deixar o servidor (nem a tarefa) presos
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(progress)));
            let mut inner = shared.lock().unwrap();
            if let Some(job) = inner.jobs.get_mut(&id) {
                match outcome {
                    Ok(Ok(result)) => {
                        job.result = Some(result);
                        job.status = "done";
                    }
                    Ok(Err(e)) => {
                        job.error = Some(e);
                        job.status = "failed";
                    }
                    Err(_) => {
                        job.error = Some("Panic: a tarefa terminou de forma inesperada".into());
                        job.status = "failed";
                    }
                }
                job.finished = Some(crate::util::now_iso());
            }
        });
        Ok(snapshot)
    }

    pub fn get(&self, id: &str, since_line: usize) -> Option<Value> {
        self.inner.lock().unwrap().jobs.get(id).map(|j| j.to_json(since_line))
    }

    pub fn recent(&self, limit: usize) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        inner.order.iter().rev().take(limit).filter_map(|id| inner.jobs.get(id)).map(|j| j.to_json(0)).collect()
    }
}
