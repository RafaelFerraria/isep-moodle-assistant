"""Tarefas longas (resumo, sincronização) executadas em segundo plano, uma de cada vez."""
import datetime
import threading
import uuid
from typing import Any, Callable, Dict, List, Optional

MAX_PROGRESS_LINES = 300
MAX_JOBS_KEPT = 30


class Busy(Exception):
    def __init__(self, active_job_id: str):
        super().__init__("Já existe uma tarefa em execução.")
        self.active_job_id = active_job_id


class Job:
    def __init__(self, kind: str):
        self.id = uuid.uuid4().hex[:12]
        self.kind = kind
        self.status = "running"          # running | done | failed
        self.started = datetime.datetime.now().isoformat(timespec="seconds")
        self.finished: Optional[str] = None
        self.progress: List[str] = []
        self.result: Optional[Dict[str, Any]] = None
        self.error: Optional[str] = None

    def add_progress(self, line: str) -> None:
        self.progress.append(line)
        if len(self.progress) > MAX_PROGRESS_LINES:
            del self.progress[: len(self.progress) - MAX_PROGRESS_LINES]

    def to_dict(self, since_line: int = 0) -> Dict[str, Any]:
        return {
            "id": self.id, "kind": self.kind, "status": self.status,
            "started": self.started, "finished": self.finished,
            "progress": self.progress[since_line:], "progress_total": len(self.progress),
            "result": self.result, "error": self.error,
        }


class JobManager:
    def __init__(self) -> None:
        self._jobs: Dict[str, Job] = {}
        self._order: List[str] = []
        self._lock = threading.Lock()

    def active(self) -> Optional[Job]:
        with self._lock:
            return next((j for j in self._jobs.values() if j.status == "running"), None)

    def submit(self, kind: str, fn: Callable[[Callable[[str], None]], Dict[str, Any]]) -> Job:
        """Arranca 'fn(progress)' numa thread. Só uma tarefa de cada vez (Busy se já houver uma)."""
        with self._lock:
            running = next((j for j in self._jobs.values() if j.status == "running"), None)
            if running:
                raise Busy(running.id)
            job = Job(kind)
            self._jobs[job.id] = job
            self._order.append(job.id)
            while len(self._order) > MAX_JOBS_KEPT:
                self._jobs.pop(self._order.pop(0), None)

        def runner() -> None:
            try:
                job.result = fn(job.add_progress)
                job.status = "done"
            except Exception as err:  # a UI mostra o erro; o servidor não cai
                job.error = f"{type(err).__name__}: {err}"
                job.status = "failed"
            finally:
                job.finished = datetime.datetime.now().isoformat(timespec="seconds")

        threading.Thread(target=runner, name=f"job-{job.kind}-{job.id}", daemon=True).start()
        return job

    def get(self, job_id: str) -> Optional[Job]:
        with self._lock:
            return self._jobs.get(job_id)

    def recent(self, limit: int = 10) -> List[Dict[str, Any]]:
        with self._lock:
            return [self._jobs[i].to_dict() for i in reversed(self._order[-limit:])]
