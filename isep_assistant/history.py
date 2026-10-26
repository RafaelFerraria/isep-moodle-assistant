"""Histórico de execuções (resumos enviados e sincronizações do Moodle) em JSON Lines."""
import datetime
import json
import threading
import uuid
from typing import Any, Dict, List, Optional

from . import paths

MAX_RECORDS = 500
_PRUNE_AT = 700
_lock = threading.Lock()


def _now() -> str:
    return datetime.datetime.now().isoformat(timespec="seconds")


def new_record(kind: str, trigger: str, ok: bool, started: str, *, details: Optional[Dict[str, Any]] = None,
               summary: Optional[str] = None, error: Optional[str] = None) -> Dict[str, Any]:
    finished = _now()
    try:
        duration = round((datetime.datetime.fromisoformat(finished) - datetime.datetime.fromisoformat(started)).total_seconds(), 1)
    except ValueError:
        duration = None
    return {
        "id": uuid.uuid4().hex[:12],
        "kind": kind,          # "digest" | "moodle_sync"
        "trigger": trigger,    # "scheduled" | "manual" | "api"
        "ok": ok,
        "started": started,
        "finished": finished,
        "duration_s": duration,
        "error": error,
        "details": details or {},
        "summary": summary,
    }


def append(record: Dict[str, Any]) -> Dict[str, Any]:
    path = paths.history_file()
    with _lock:
        path.parent.mkdir(parents=True, exist_ok=True)
        # se a última escrita foi cortada a meio (sem \n final), começa numa linha nova para não corromper este registo
        needs_newline = False
        try:
            with open(path, "rb") as probe:
                probe.seek(0, 2)
                if probe.tell() > 0:
                    probe.seek(-1, 2)
                    needs_newline = probe.read(1) != b"\n"
        except OSError:
            pass
        with open(path, "a", encoding="utf-8") as f:
            f.write(("\n" if needs_newline else "") + json.dumps(record, ensure_ascii=False) + "\n")
        _prune_if_needed(path)
    return record


def _read_all(path) -> List[Dict[str, Any]]:
    records: List[Dict[str, Any]] = []
    try:
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                try:
                    records.append(json.loads(line))
                except ValueError:
                    continue  # linha truncada (corte de energia a meio de uma escrita)
    except OSError:
        pass
    return records


def _prune_if_needed(path) -> None:
    records = _read_all(path)
    if len(records) > _PRUNE_AT:
        with open(path, "w", encoding="utf-8") as f:
            for r in records[-MAX_RECORDS:]:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")


def list_records(limit: int = 50, offset: int = 0, kind: Optional[str] = None,
                 include_summary: bool = False) -> List[Dict[str, Any]]:
    """Mais recentes primeiro. A lista omite o texto do resumo (pesado) a menos que se peça."""
    with _lock:
        records = _read_all(paths.history_file())
    if kind:
        records = [r for r in records if r.get("kind") == kind]
    records.reverse()
    page = records[offset: offset + max(0, limit)]
    if include_summary:
        return page
    return [{**r, "summary": None, "has_summary": bool(r.get("summary"))} for r in page]


def get(record_id: str) -> Optional[Dict[str, Any]]:
    with _lock:
        for r in _read_all(paths.history_file()):
            if r.get("id") == record_id:
                return r
    return None


def last(kind: str, only_ok: bool = False) -> Optional[Dict[str, Any]]:
    for r in list_records(limit=MAX_RECORDS, kind=kind, include_summary=False):
        if not only_ok or r.get("ok"):
            return r
    return None
