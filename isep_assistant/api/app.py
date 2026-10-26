"""API HTTP local (FastAPI) usada pela interface gráfica. Contrato em /openapi.json."""
from typing import Any, Dict, Optional

from fastapi import Body, Depends, FastAPI, HTTPException, Query
from pydantic import BaseModel
from starlette.responses import JSONResponse

from .. import __version__, config_store, history, service, system
from ..digest import run_digest
from .jobs import Busy, JobManager
from .security import host_guard, make_auth_dependency

API_PREFIX = "/api/v1"


class MoodleCredentials(BaseModel):
    username: str
    password: str


def create_app(token: str, jobs: Optional[JobManager] = None) -> FastAPI:
    app = FastAPI(title="ISEP Moodle Assistant (API local)", version=__version__,
                  docs_url=None, redoc_url=None)  # sem /docs: não há UI web exposta
    app.middleware("http")(host_guard)
    jobs = jobs or JobManager()
    auth = [Depends(make_auth_dependency(token))]

    @app.exception_handler(config_store.ValidationError)
    async def validation_handler(_request, exc: config_store.ValidationError):
        return JSONResponse({"detail": "Definições inválidas.", "errors": exc.errors}, status_code=422)

    @app.exception_handler(Busy)
    async def busy_handler(_request, exc: Busy):
        return JSONResponse({"detail": str(exc), "active_job_id": exc.active_job_id}, status_code=409)

    # --- sem autenticação: só para a UI saber se o servidor está de pé
    @app.get(f"{API_PREFIX}/health")
    def health() -> Dict[str, Any]:
        return {"ok": True, "version": __version__}

    # --- estado e definições
    @app.get(f"{API_PREFIX}/status", dependencies=auth)
    def status() -> Dict[str, Any]:
        data = service.get_status()
        data["active_job"] = (jobs.active().to_dict() if jobs.active() else None)
        return data

    @app.get(f"{API_PREFIX}/settings", dependencies=auth)
    def get_settings() -> Dict[str, Any]:
        return service.get_settings()

    @app.put(f"{API_PREFIX}/settings", dependencies=auth)
    def put_settings(patch: Dict[str, Any] = Body(...)) -> Dict[str, Any]:
        return service.update_settings(patch)

    @app.post(f"{API_PREFIX}/summary/test", dependencies=auth)
    def summary_test() -> Dict[str, Any]:
        return service.test_summary()

    @app.post(f"{API_PREFIX}/schedule/register", dependencies=auth)
    def schedule_register() -> Dict[str, Any]:
        time_value = config_store.public_settings(config_store.load_config())["schedule"]["time"]
        ok, message = system.register_tasks(time_value)
        return {"ok": ok, "message": message}

    @app.put(f"{API_PREFIX}/moodle/credentials", dependencies=auth)
    def moodle_credentials(body: MoodleCredentials) -> Dict[str, Any]:
        result = service.set_moodle_credentials(body.username, body.password)
        if not result["ok"]:
            raise HTTPException(status_code=400, detail=result["error"])
        return result

    # --- tarefas em segundo plano
    @app.post(f"{API_PREFIX}/digest/preview", dependencies=auth, status_code=202)
    def digest_preview() -> Dict[str, Any]:
        job = jobs.submit("digest_preview", lambda progress: run_digest(send=False, trigger="api", progress=progress).to_dict())
        return job.to_dict()

    @app.post(f"{API_PREFIX}/digest/run", dependencies=auth, status_code=202)
    def digest_run() -> Dict[str, Any]:
        job = jobs.submit("digest", lambda progress: run_digest(send=True, trigger="api", progress=progress).to_dict())
        return job.to_dict()

    @app.post(f"{API_PREFIX}/moodle/sync", dependencies=auth, status_code=202)
    def moodle_sync_now() -> Dict[str, Any]:
        def work(progress):
            from .. import moodle_sync  # importação tardia (requests/bs4)
            return moodle_sync.run_sync(trigger="api", progress=progress).to_dict()
        return jobs.submit("moodle_sync", work).to_dict()

    @app.get(f"{API_PREFIX}/jobs", dependencies=auth)
    def list_jobs(limit: int = Query(10, ge=1, le=30)) -> Dict[str, Any]:
        return {"jobs": jobs.recent(limit)}

    @app.get(f"{API_PREFIX}/jobs/{{job_id}}", dependencies=auth)
    def get_job(job_id: str, since_line: int = Query(0, ge=0)) -> Dict[str, Any]:
        job = jobs.get(job_id)
        if not job:
            raise HTTPException(status_code=404, detail="Tarefa desconhecida.")
        return job.to_dict(since_line)

    # --- histórico e dados
    @app.get(f"{API_PREFIX}/history", dependencies=auth)
    def get_history(kind: Optional[str] = Query(None, pattern="^(digest|moodle_sync)$"),
                    limit: int = Query(30, ge=1, le=200), offset: int = Query(0, ge=0)) -> Dict[str, Any]:
        return {"items": history.list_records(limit=limit, offset=offset, kind=kind)}

    @app.get(f"{API_PREFIX}/history/{{record_id}}", dependencies=auth)
    def get_history_item(record_id: str) -> Dict[str, Any]:
        record = history.get(record_id)
        if not record:
            raise HTTPException(status_code=404, detail="Registo desconhecido.")
        return record

    @app.get(f"{API_PREFIX}/moodle/new-files", dependencies=auth)
    def new_files(limit: int = Query(100, ge=1, le=500)) -> Dict[str, Any]:
        return {"items": service.recent_new_files(limit)}

    @app.get(f"{API_PREFIX}/logs/{{name}}", dependencies=auth)
    def logs(name: str, lines: int = Query(200, ge=1, le=2000)) -> Dict[str, Any]:
        content = service.tail_log(name, lines)
        if content is None:
            raise HTTPException(status_code=404, detail="Log desconhecido (notifier ou sync).")
        return {"name": name, "lines": content}

    return app
