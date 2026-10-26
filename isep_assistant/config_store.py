"""Leitura/escrita segura da configuração e validação das definições editáveis (usadas pela API/UI)."""
import json
import os
import re
import threading
from typing import Any, Dict, List, Tuple

from . import paths
from .ai_summarizer import API_DEFAULTS, CLI_PRESETS

# Segredos: nunca saem pela API; só se pode definir/limpar.
SECRET_KEYS = {"session_mnemonic", "ai_api_key", "email_password", "access_token"}

SUMMARY_MODES = {"rules", "ai"}
AI_PROVIDERS = {"cli", "api"}
AI_CLIS = set(CLI_PRESETS) | {"custom"}
AI_API_TYPES = set(API_DEFAULTS)
TIME_RE = re.compile(r"^([01]?\d|2[0-3]):[0-5]\d$")
SESSION_ID_RE = re.compile(r"^05[0-9a-fA-F]{64}$")

_lock = threading.Lock()


class ValidationError(ValueError):
    """Definições inválidas; 'errors' mapeia campo -> mensagem."""

    def __init__(self, errors: Dict[str, str]):
        super().__init__("; ".join(f"{k}: {v}" for k, v in errors.items()))
        self.errors = errors


# ---------------------------------------------------------------- ficheiros

def read_json(path, default=None):
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {} if default is None else default


def write_json_atomic(path, data: Dict[str, Any]) -> None:
    """Escreve para um ficheiro temporário e substitui (nunca deixa config.json a meio)."""
    path = os.fspath(path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    os.replace(tmp, path)


def load_config() -> Dict[str, Any]:
    return read_json(paths.email_config())


def save_config(config: Dict[str, Any]) -> None:
    with _lock:
        write_json_atomic(paths.email_config(), config)


# ---------------------------------------------------------------- vista pública (sem segredos)

def mask_session_id(value: str) -> str:
    value = value or ""
    return f"{value[:6]}…{value[-4:]}" if len(value) > 12 else value


def recipients_of(config: Dict[str, Any]) -> List[str]:
    rec = config.get("session_recipients")
    if isinstance(rec, str):
        rec = [rec]
    if not rec and config.get("session_group_id"):
        rec = [config["session_group_id"]]
    return [str(r).strip() for r in (rec or []) if str(r).strip()]


def public_settings(config: Dict[str, Any]) -> Dict[str, Any]:
    """Definições para mostrar na UI. Os segredos aparecem só como '..._set: true/false'."""
    api_type = config.get("ai_api_type") or "openai"
    env_name = API_DEFAULTS.get(api_type, {}).get("env", "")
    return {
        "summary": {
            "mode": config.get("summary_mode") or "rules",
            "ai_provider": config.get("ai_provider") or "cli",
            "ai_cli": config.get("ai_cli") or "claude",
            "ai_cli_command": config.get("ai_cli_command") or "",
            "ai_api_type": api_type,
            "ai_model": config.get("ai_model") or "",
            "ai_base_url": config.get("ai_base_url") or "",
            "ai_api_key_set": bool(config.get("ai_api_key")),
            "ai_api_key_env": env_name,
            "ai_api_key_env_set": bool(env_name and os.environ.get(env_name)),
        },
        "schedule": {"time": config.get("scheduled_time") or "18:00"},
        "destination": {
            "session_recipients": [mask_session_id(r) for r in recipients_of(config)],
            "session_configured": bool(recipients_of(config) and config.get("session_mnemonic")),
        },
        "email": {
            "method": config.get("email_method") or "thunderbird",
            "address": config.get("email_address") or "",
        },
        "moodle": {
            "sync_dir": config.get("moodle_sync_dir") or str(paths.sync_dir()),
            "download_dir": config.get("moodle_download_dir") or str(paths.default_download_dir()),
        },
    }


# ---------------------------------------------------------------- alterações validadas

def apply_settings_patch(config: Dict[str, Any], patch: Dict[str, Any]) -> Tuple[Dict[str, Any], List[str]]:
    """Aplica uma alteração parcial (mesma forma que public_settings) e devolve (config, efeitos).

    Efeitos possíveis: "reschedule" (a hora mudou: é preciso voltar a registar as tarefas do Windows).
    Levanta ValidationError com todos os campos inválidos de uma vez.
    """
    new = dict(config)
    errors: Dict[str, str] = {}
    effects: List[str] = []

    summary = patch.get("summary") or {}
    if "mode" in summary:
        if summary["mode"] in SUMMARY_MODES:
            new["summary_mode"] = summary["mode"]
        else:
            errors["summary.mode"] = f"deve ser um de {sorted(SUMMARY_MODES)}"
    if "ai_provider" in summary:
        if summary["ai_provider"] in AI_PROVIDERS:
            new["ai_provider"] = summary["ai_provider"]
        else:
            errors["summary.ai_provider"] = f"deve ser um de {sorted(AI_PROVIDERS)}"
    if "ai_cli" in summary:
        if summary["ai_cli"] in AI_CLIS:
            new["ai_cli"] = summary["ai_cli"]
        else:
            errors["summary.ai_cli"] = f"deve ser um de {sorted(AI_CLIS)}"
    if "ai_cli_command" in summary:
        cmd = summary["ai_cli_command"]
        if isinstance(cmd, (str, list)):
            new["ai_cli_command"] = cmd
        else:
            errors["summary.ai_cli_command"] = "texto ou lista"
    if "ai_api_type" in summary:
        if summary["ai_api_type"] in AI_API_TYPES:
            new["ai_api_type"] = summary["ai_api_type"]
        else:
            errors["summary.ai_api_type"] = f"deve ser um de {sorted(AI_API_TYPES)}"
    for key in ("ai_model", "ai_base_url"):
        if key in summary:
            val = summary[key]
            if not isinstance(val, str):
                errors[f"summary.{key}"] = "deve ser texto"
            elif key == "ai_base_url" and val and not re.match(r"^https?://", val):
                errors["summary.ai_base_url"] = "deve começar por http:// ou https://"
            else:
                new[key] = val.strip()
    if summary.get("clear_ai_api_key"):
        new.pop("ai_api_key", None)
    if summary.get("ai_api_key"):
        new["ai_api_key"] = str(summary["ai_api_key"]).strip()

    # coerência: modo IA exige ligação válida
    if new.get("summary_mode") == "ai":
        if (new.get("ai_provider") or "cli") == "cli" and new.get("ai_cli") == "custom" and not new.get("ai_cli_command"):
            errors["summary.ai_cli_command"] = "obrigatório quando a CLI é 'custom'"
        if new.get("ai_provider") == "api" and new.get("ai_api_type") == "openai_compatible":
            if not new.get("ai_base_url"):
                errors["summary.ai_base_url"] = "obrigatório para APIs compatíveis com OpenAI"
            if not new.get("ai_model"):
                errors["summary.ai_model"] = "obrigatório para APIs compatíveis com OpenAI"

    schedule = patch.get("schedule") or {}
    if "time" in schedule:
        if isinstance(schedule["time"], str) and TIME_RE.match(schedule["time"]):
            h, m = schedule["time"].split(":")
            normalized = f"{int(h):02d}:{int(m):02d}"
            if normalized != new.get("scheduled_time"):
                effects.append("reschedule")
            new["scheduled_time"] = normalized
        else:
            errors["schedule.time"] = "formato HH:MM"

    dest = patch.get("destination") or {}
    if "session_id" in dest:
        sid = str(dest["session_id"]).strip()
        if SESSION_ID_RE.match(sid):
            new["session_recipients"] = [sid]
            new["session_group_id"] = sid
        else:
            errors["destination.session_id"] = "deve ter 66 caracteres hexadecimais e começar por 05"

    moodle = patch.get("moodle") or {}
    for key, target in (("sync_dir", "moodle_sync_dir"), ("download_dir", "moodle_download_dir")):
        if key in moodle:
            if isinstance(moodle[key], str) and moodle[key].strip():
                new[target] = moodle[key].strip()
            else:
                errors[f"moodle.{key}"] = "caminho em falta"

    if errors:
        raise ValidationError(errors)
    return new, effects
