"""Localização dos ficheiros de dados.

Mantém o layout original (config.json em EmailNotifier/ e em MoodleSync/) para que as instalações
existentes continuem a funcionar. Tudo é resolvido em tempo de chamada para poder ser redirecionado
nos testes com ISEP_ASSISTANT_ROOT (dados) e ISEP_ASSISTANT_STATE (token/porta da API).
"""
import os
from pathlib import Path


def root() -> Path:
    env = os.environ.get("ISEP_ASSISTANT_ROOT")
    return Path(env) if env else Path(__file__).resolve().parent.parent


def email_dir() -> Path:
    return root() / "EmailNotifier"


def sync_dir() -> Path:
    return root() / "MoodleSync"


def email_config() -> Path:
    return email_dir() / "config.json"


def last_run_file() -> Path:
    return email_dir() / "last_run.json"


def notifier_log() -> Path:
    return email_dir() / "notifier.log"


def history_file() -> Path:
    return email_dir() / "history.jsonl"


def session_script() -> Path:
    return email_dir() / "session_sender.cjs"


def digest_runner_bat() -> Path:
    return email_dir() / "executar_notificador.bat"


def sync_config(sync_directory: "str | os.PathLike | None" = None) -> Path:
    return Path(sync_directory or sync_dir()) / "config.json"


def sync_log(sync_directory: "str | os.PathLike | None" = None) -> Path:
    return Path(sync_directory or sync_dir()) / "sync_log.txt"


def new_files_log(sync_directory: "str | os.PathLike | None" = None) -> Path:
    return Path(sync_directory or sync_dir()) / "new_files.jsonl"


def sync_lock(sync_directory: "str | os.PathLike | None" = None) -> Path:
    return Path(sync_directory or sync_dir()) / "sync.lock"


def default_download_dir() -> Path:
    return Path.home() / "Downloads" / "Moodle_ISEP"


def state_dir() -> Path:
    """Pasta com a porta e o token da API local (fora do repositório)."""
    env = os.environ.get("ISEP_ASSISTANT_STATE")
    if env:
        return Path(env)
    base = os.environ.get("APPDATA") or str(Path.home() / ".config")
    return Path(base) / "ISEPAssistant"


def api_state_file() -> Path:
    return state_dir() / "api.json"
