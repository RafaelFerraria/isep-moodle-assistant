"""Operações de alto nível usadas pela API (e reutilizáveis por outros fronts): estado, definições, testes."""
import datetime
import os
import shutil
from collections import deque
from typing import Any, Dict, List, Optional

from . import __version__, config_store, digest, history, paths, system
from .ai_summarizer import API_DEFAULTS, CLI_PRESETS, describe, resolve_cli_command
from .moodle_report import load_new_files
from .summarizer import SAMPLE_EMAILS, summarize_emails

LOG_NAMES = ("notifier", "sync")


# ---------------------------------------------------------------- verificações

def _check(check_id: str, label: str, ok: bool, detail: str = "", required: bool = True) -> Dict[str, Any]:
    return {"id": check_id, "label": label, "ok": bool(ok), "detail": detail, "required": required}


def ai_ready(config: Dict[str, Any]) -> Dict[str, Any]:
    """A IA está pronta a usar? (CLI instalada / token disponível)"""
    if (config.get("summary_mode") or "rules") != "ai":
        return _check("ai", "Resumo por IA", True, "Modo 'Regras' (local) ativo.", required=False)
    if (config.get("ai_provider") or "cli") == "cli":
        try:
            exe = resolve_cli_command(config)[0]
        except RuntimeError as err:
            return _check("ai", "Resumo por IA", False, str(err))
        found = shutil.which(exe)
        return _check("ai", "Resumo por IA", bool(found),
                      f"CLI '{exe}' encontrada." if found else f"CLI '{exe}' não encontrada no PATH.")
    api_type = config.get("ai_api_type") or "openai"
    env_name = API_DEFAULTS.get(api_type, {}).get("env", "")
    has_key = bool(config.get("ai_api_key") or (env_name and os.environ.get(env_name)))
    return _check("ai", "Resumo por IA", has_key,
                  "Token de API disponível." if has_key else f"Sem token (define-o ou usa a variável {env_name}).")


def moodle_state(config: Dict[str, Any]) -> Dict[str, Any]:
    sync_dir = digest.moodle_dir_of(config)
    sync_cfg = config_store.read_json(paths.sync_config(sync_dir)) if sync_dir else {}
    return {
        "configured": bool(sync_cfg.get("username") and sync_cfg.get("encrypted_password")),
        "username": sync_cfg.get("username") or "",
        "last_sync_date": sync_cfg.get("last_sync_date") or None,
        "run_after_hour": sync_cfg.get("run_after_hour", 18),
        "download_dir": sync_cfg.get("download_dir") or config.get("moodle_download_dir") or str(paths.default_download_dir()),
        "sync_dir": sync_dir,
    }


def get_status() -> Dict[str, Any]:
    config = config_store.load_config()
    tasks = system.query_tasks()
    mood = moodle_state(config)
    settings = config_store.public_settings(config)

    checks = [
        _check("config", "Configuração criada", bool(config), "" if config else "Corre o CONFIGURAR_TUDO.bat."),
        _check("session", "Destino Session configurado", settings["destination"]["session_configured"]),
        _check("thunderbird", "Thunderbird instalado", bool(system.find_thunderbird_path()),
               "" if system.find_thunderbird_path() else "Necessário para ler os emails do ISEP.",
               required=(config.get("email_method") or "thunderbird") == "thunderbird"),
        _check("node", "Node.js instalado", bool(system.node_path()), "Necessário para enviar para o Session."),
        _check("moodle", "Credenciais do Moodle", mood["configured"],
               "" if mood["configured"] else "Define o utilizador e a palavra-passe do Moodle.", required=False),
        _check("tasks", "Tarefas agendadas no Windows", len(tasks) >= 1,
               f"{len(tasks)} tarefa(s) registada(s)." if tasks else "Sem tarefas: o resumo não corre sozinho.",
               required=system.IS_WINDOWS),
        _check("internet", "Ligação ao Moodle", system.internet_ok()),
        ai_ready(config),
    ]
    next_runs = sorted(t["next_run"] for t in tasks if t.get("next_run"))
    last_digest = history.last("digest")
    last_sync = history.last("moodle_sync")
    return {
        "version": __version__,
        "now": datetime.datetime.now().isoformat(timespec="seconds"),
        "ok": all(c["ok"] for c in checks if c["required"]),
        "checks": checks,
        "summary_mode": {"mode": settings["summary"]["mode"], "description": describe(config)},
        "digest": {"last": last_digest, "last_ok": history.last("digest", only_ok=True),
                   "next_run": next_runs[0] if next_runs else None, "scheduled_time": settings["schedule"]["time"]},
        "moodle": {**mood, "last": last_sync},
        "tasks": tasks,
    }


# ---------------------------------------------------------------- definições

def get_settings() -> Dict[str, Any]:
    return config_store.public_settings(config_store.load_config())


def update_settings(patch: Dict[str, Any]) -> Dict[str, Any]:
    """Valida e grava. Se a hora mudou, volta a registar as tarefas do Windows."""
    config = config_store.load_config()
    new_config, effects = config_store.apply_settings_patch(config, patch)
    config_store.save_config(new_config)
    result: Dict[str, Any] = {"settings": config_store.public_settings(new_config), "effects": effects}
    if "reschedule" in effects:
        ok, message = system.register_tasks(new_config["scheduled_time"])
        result["reschedule"] = {"ok": ok, "message": message}
    return result


def test_summary() -> Dict[str, Any]:
    """Resumo de exemplo com a configuração atual (verifica a ligação à IA sem ler emails reais)."""
    config = config_store.load_config()
    text = summarize_emails(SAMPLE_EMAILS, config)
    fallback = "IA indisponível" in text
    return {"text": text, "ai_requested": (config.get("summary_mode") or "rules") == "ai",
            "ai_fallback": fallback, "ok": not fallback}


def set_moodle_credentials(username: str, password: str) -> Dict[str, Any]:
    """Valida o login no Moodle e guarda a palavra-passe cifrada (DPAPI). Nunca a devolve."""
    from . import moodle_sync  # importação tardia: requests/bs4 só quando preciso
    user = moodle_sync.clean_username(username)
    if not user or not password:
        return {"ok": False, "error": "Utilizador e palavra-passe são obrigatórios."}
    config = config_store.load_config()
    download_dir = config.get("moodle_download_dir") or moodle_sync.DEFAULT_DOWNLOAD_DIR
    if not moodle_sync.MoodleISEPSync(user, password, download_dir).login():
        return {"ok": False, "error": "O Moodle recusou o login (confirma o número e a palavra-passe)."}
    sync_cfg = moodle_sync.load_config()
    sync_cfg.update({"username": user, "encrypted_password": moodle_sync.encrypt_dpapi(password),
                     "download_dir": sync_cfg.get("download_dir") or download_dir})
    sync_cfg.setdefault("run_after_hour", 18)
    sync_cfg.setdefault("last_sync_date", "")
    moodle_sync.save_config(sync_cfg)
    return {"ok": True, "username": user}


# ---------------------------------------------------------------- dados

def recent_new_files(limit: int = 100) -> List[Dict[str, Any]]:
    config = config_store.load_config()
    sync_dir = digest.moodle_dir_of(config)
    if not sync_dir:
        return []
    files = load_new_files(str(paths.new_files_log(sync_dir)), datetime.datetime.min)
    return list(reversed(files))[: max(0, limit)]


def tail_log(name: str, lines: int = 200) -> Optional[List[str]]:
    if name not in LOG_NAMES:
        return None
    lines = max(1, min(lines, 2000))
    path = paths.notifier_log() if name == "notifier" else paths.sync_log(digest.moodle_dir_of(config_store.load_config()))
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return [l.rstrip("\n") for l in deque(f, maxlen=lines)]
    except OSError:
        return []
