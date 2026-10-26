"""Resumo diário: lê emails, junta as novidades do Moodle, gera o resumo e envia para o Session."""
import datetime
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from typing import Any, Callable, Dict, List, Optional

from . import history, paths
from .config_store import load_config, read_json, write_json_atomic
from .mail_fetcher import get_emails
from .moodle_report import load_new_files, build_moodle_section
from .session import send_to_session
from .summarizer import summarize_emails

OVERLAP_HOURS = 48
MOODLE_SYNC_TIMEOUT = 20 * 60

Progress = Callable[[str], None]


@dataclass
class DigestResult:
    ok: bool = False
    sent: bool = False
    skipped: bool = False          # nada de novo para enviar
    preview: bool = False
    emails_found: int = 0
    emails_new: int = 0
    moodle_files: int = 0
    ai_used: bool = False
    ai_fallback: bool = False
    summary: Optional[str] = None
    error: Optional[str] = None
    log: List[str] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


def file_log(msg: str) -> None:
    line = f"[{datetime.datetime.now():%Y-%m-%d %H:%M:%S}] {msg}"
    try:
        print(line)
    except Exception:
        pass
    try:
        with open(paths.notifier_log(), "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except OSError:
        pass


# ---------------------------------------------------------------- estado entre execuções (last_run.json)

def last_run_value(key: str):
    return read_json(paths.last_run_file()).get(key)


def sent_ids() -> List[str]:
    return list(read_json(paths.last_run_file()).get("sent_ids", []))


def get_since_datetime(log: Progress = file_log) -> datetime.datetime:
    value = last_run_value("last_run")
    if value:
        try:
            return datetime.datetime.fromisoformat(value)
        except ValueError as err:
            log(f"Aviso ao ler last_run.json: {err}")
    return datetime.datetime.now() - datetime.timedelta(hours=24)


def update_last_run(new_ids=None, moodle_ts: Optional[str] = None) -> None:
    data = {
        "last_run": datetime.datetime.now().isoformat(),
        "last_moodle_ts": moodle_ts or last_run_value("last_moodle_ts") or datetime.datetime.now().isoformat(),
        "sent_ids": (sent_ids() + list(new_ids or []))[-1000:],
    }
    write_json_atomic(paths.last_run_file(), data)


# ---------------------------------------------------------------- Moodle

def moodle_dir_of(config: Dict[str, Any]) -> Optional[str]:
    """Pasta do MoodleSync a usar (config > pasta ao lado, se existir)."""
    configured = config.get("moodle_sync_dir")
    if configured:
        return configured
    default = paths.sync_dir()
    return str(default) if (default / "config.json").exists() else None


def ensure_moodle_synced(config: Dict[str, Any], log: Progress = file_log) -> None:
    """Garante que o MoodleSync já correu hoje (ou corre-o agora) para o resumo incluir o que foi descarregado."""
    sync_dir = moodle_dir_of(config)
    if not sync_dir:
        return
    lock = paths.sync_lock(sync_dir)
    waited = 0
    while lock.exists() and waited < MOODLE_SYNC_TIMEOUT:  # outra sincronização (arranque do Windows) a decorrer
        time.sleep(10)
        waited += 10
    last_sync = read_json(paths.sync_config(sync_dir)).get("last_sync_date")
    if last_sync == datetime.date.today().isoformat():
        return

    legacy_script = os.path.join(sync_dir, "moodle_sync.py")
    cmd = [sys.executable, legacy_script, "--run"] if os.path.exists(legacy_script) \
        else [sys.executable, "-m", "isep_assistant.moodle_sync", "--run"]
    log("A sincronizar o Moodle antes de gerar o resumo...")
    try:
        subprocess.run(cmd, cwd=str(paths.root()), timeout=MOODLE_SYNC_TIMEOUT,
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    except Exception as err:
        log(f"Aviso: falha na sincronização do Moodle: {err}")


def moodle_section(config: Dict[str, Any], fallback_since: datetime.datetime, log: Progress = file_log):
    sync_dir = moodle_dir_of(config)
    if not sync_dir:
        return [], 0
    since = datetime.datetime.fromisoformat(last_run_value("last_moodle_ts") or fallback_since.isoformat())
    new_files = load_new_files(str(paths.new_files_log(sync_dir)), since)
    download_dir = config.get("moodle_download_dir") or str(paths.default_download_dir())
    log(f"Ficheiros novos no Moodle: {len(new_files)}")
    return build_moodle_section(new_files, download_dir), len(new_files)


# ---------------------------------------------------------------- execução

def run_digest(send: bool = True, trigger: str = "manual", progress: Optional[Progress] = None,
               config: Optional[Dict[str, Any]] = None, sync_moodle: bool = True) -> DigestResult:
    """Gera (e, se send=True, envia) o resumo. Com send=False é só uma pré-visualização: não envia,
    não avança last_run.json e não entra no histórico."""
    res = DigestResult(preview=not send)
    started = datetime.datetime.now().isoformat(timespec="seconds")

    def log(msg: str) -> None:
        res.log.append(msg)
        file_log(msg)
        if progress:
            progress(msg)

    log("=== INÍCIO DA EXECUÇÃO DO NOTIFICADOR ISEP ===" if send else "=== PRÉ-VISUALIZAÇÃO DO RESUMO ===")
    config = config if config is not None else load_config()
    if not config:
        res.error = "config.json não encontrado. Executa primeiro o CONFIGURAR_TUDO.bat."
        log(f"ERRO: {res.error}")
        return _finish(res, send, trigger, started)

    # Emails sincronizados tarde (PC em suspensão) trazem uma data anterior ao último arranque,
    # por isso pesquisa com sobreposição e deduplica por Message-ID.
    since_dt = get_since_datetime(log) - datetime.timedelta(hours=OVERLAP_HOURS)
    log(f"A procurar emails recebidos desde: {since_dt:%d/%m/%Y %H:%M:%S}")
    try:
        emails = get_emails(config, since_dt)
        res.emails_found = len(emails)
        log(f"Emails encontrados: {len(emails)}")
    except Exception as err:
        res.error = str(err)
        log(f"ERRO ao obter emails: {err}")
        return _finish(res, send, trigger, started)

    if sync_moodle:
        ensure_moodle_synced(config, log)
    moodle_ts = datetime.datetime.now().isoformat()
    moodle_lines, res.moodle_files = moodle_section(config, since_dt, log)

    already = set(sent_ids())
    emails = [e for e in emails if not e.get("message_id") or e["message_id"] not in already]
    res.emails_new = len(emails)
    log(f"Emails novos (ainda não enviados): {len(emails)}")

    if not emails and not moodle_lines:
        log("Nenhum email novo recebido. Nenhuma mensagem necessária.")
        res.ok = res.skipped = True
        if send:
            update_last_run(moodle_ts=moodle_ts)
        log("=== EXECUÇÃO TERMINADA (SEM NOVOS EMAILS) ===")
        return _finish(res, send, trigger, started)

    log("A gerar resumo e priorização dos emails...")
    res.summary = summarize_emails(emails, config, moodle_lines)
    res.ai_used = (config.get("summary_mode") or "rules") == "ai"
    res.ai_fallback = "IA indisponível" in res.summary
    if res.ai_fallback:
        res.ai_used = False
    log("Resumo gerado com sucesso!")

    if not send:
        res.ok = True
        return res

    res.sent = send_to_session(res.summary, config, log)
    res.ok = res.sent
    if res.sent:
        update_last_run([e["message_id"] for e in emails if e.get("message_id")], moodle_ts=moodle_ts)
        log("Estado atualizado com sucesso no last_run.json.")
    else:
        res.error = "Falha no envio para o Session."
        log("Aviso: last_run.json não foi atualizado devido a falha no envio.")
    log("=== EXECUÇÃO TERMINADA ===")
    return _finish(res, send, trigger, started)


def _finish(res: DigestResult, send: bool, trigger: str, started: str) -> DigestResult:
    if send:  # pré-visualizações não entram no histórico
        history.append(history.new_record(
            "digest", trigger, res.ok, started, summary=res.summary, error=res.error,
            details={"emails_found": res.emails_found, "emails_new": res.emails_new, "moodle_files": res.moodle_files,
                     "sent": res.sent, "skipped": res.skipped, "ai_used": res.ai_used, "ai_fallback": res.ai_fallback}))
    return res


def main() -> int:
    """Ponto de entrada do Agendador de Tarefas (executar_notificador.bat)."""
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    result = run_digest(send=True, trigger="scheduled")
    return 0 if result.ok else 1
