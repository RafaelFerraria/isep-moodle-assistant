import os
import sys
import json
import datetime
import subprocess
import time

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

from mail_fetcher import get_emails
from summarizer import summarize_emails
from moodle_report import load_new_files, build_moodle_section

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")
LAST_RUN_FILE = os.path.join(BASE_DIR, "last_run.json")
LOG_FILE = os.path.join(BASE_DIR, "notifier.log")
OVERLAP_HOURS = 48
MOODLE_SYNC_TIMEOUT = 20 * 60

def log(msg: str):
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    formatted = f"[{timestamp}] {msg}"
    print(formatted)
    try:
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(formatted + "\n")
    except Exception:
        pass

def load_config():
    if not os.path.exists(CONFIG_FILE):
        log("ERRO: Ficheiro config.json não encontrado. Executa primeiro o 'configurar.bat'!")
        sys.exit(1)
    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        return json.load(f)

def get_since_datetime():
    if os.path.exists(LAST_RUN_FILE):
        try:
            with open(LAST_RUN_FILE, "r", encoding="utf-8-sig") as f:
                data = json.load(f)
                last_dt_str = data.get("last_run")
                if last_dt_str:
                    return datetime.datetime.fromisoformat(last_dt_str)
        except Exception as e:
            log(f"Aviso ao ler last_run.json: {e}")

    # Default to 24 hours ago
    return datetime.datetime.now() - datetime.timedelta(hours=24)

def load_sent_ids() -> list:
    try:
        with open(LAST_RUN_FILE, "r", encoding="utf-8-sig") as f:
            return list(json.load(f).get("sent_ids", []))
    except Exception:
        return []

def load_last_run_value(key: str):
    try:
        with open(LAST_RUN_FILE, "r", encoding="utf-8-sig") as f:
            return json.load(f).get(key)
    except Exception:
        return None

def update_last_run(new_ids=None, moodle_ts=None):
    data = {
        "last_run": datetime.datetime.now().isoformat(),
        "last_moodle_ts": (moodle_ts or load_last_run_value("last_moodle_ts") or datetime.datetime.now().isoformat()),
        "sent_ids": (load_sent_ids() + list(new_ids or []))[-1000:]
    }
    with open(LAST_RUN_FILE, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

def send_to_session(text: str, config: dict) -> bool:
    session_script = os.path.join(BASE_DIR, "session_sender.cjs")
    recipients = config.get("session_recipients")
    if not recipients:
        single = config.get("session_group_id")
        recipients = [single] if single else []
    elif isinstance(recipients, str):
        recipients = [recipients]

    mnemonic = config.get("session_mnemonic")

    if not recipients or not mnemonic:
        log("ERRO: Nenhum destinatário ou session_mnemonic não configurados no config.json!")
        return False

    temp_msg_file = os.path.join(BASE_DIR, "_temp_msg.txt")
    with open(temp_msg_file, "w", encoding="utf-8") as f:
        f.write(text)

    all_success = True
    try:
        for target in recipients:
            target = str(target).strip()
            if not target:
                continue
            log(f"A enviar resumo para o Session (Destino: {target[:10]}...)...")
            cmd = [
                "node",
                session_script,
                "--to", target,
                "--file", temp_msg_file,
                "--mnemonic", mnemonic
            ]
            try:
                res = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", check=True)
                log(f"Mensagem Session enviada com sucesso para {target[:10]}...!")
            except subprocess.CalledProcessError as err:
                log(f"FALHA ao enviar para {target[:10]}...: {err.stderr or err.stdout}")
                all_success = False
        return all_success
    finally:
        if os.path.exists(temp_msg_file):
            try:
                os.remove(temp_msg_file)
            except Exception:
                pass

def ensure_moodle_synced(config: dict) -> None:
    """Garante que o MoodleSync já correu hoje (ou corre-o agora) para o resumo incluir o que foi descarregado."""
    sync_dir = config.get("moodle_sync_dir")
    if not sync_dir:
        return
    script = os.path.join(sync_dir, "moodle_sync.py")
    lock = os.path.join(sync_dir, "sync.lock")
    if not os.path.exists(script):
        log(f"Aviso: moodle_sync.py não encontrado em {sync_dir}.")
        return
    # outra sincronização (arranque do Windows) pode estar a decorrer: espera que termine
    waited = 0
    while os.path.exists(lock) and waited < MOODLE_SYNC_TIMEOUT:
        time.sleep(10)
        waited += 10
    try:
        with open(os.path.join(sync_dir, "config.json"), "r", encoding="utf-8") as f:
            last_sync = json.load(f).get("last_sync_date")
    except Exception:
        last_sync = None
    if last_sync == datetime.date.today().isoformat():
        return
    log("A sincronizar o Moodle antes de gerar o resumo...")
    try:
        subprocess.run([sys.executable, script, "--run"], cwd=sync_dir, timeout=MOODLE_SYNC_TIMEOUT,
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    except Exception as e:
        log(f"Aviso: falha na sincronização do Moodle: {e}")

def main():
    log("=== INÍCIO DA EXECUÇÃO DO NOTIFICADOR ISEP ===")
    config = load_config()
    since_dt = get_since_datetime()
    # Emails sincronizados tarde (PC em suspensão) trazem uma data anterior ao último
    # arranque, por isso pesquisa com sobreposição e deduplica por Message-ID.
    since_dt -= datetime.timedelta(hours=OVERLAP_HOURS)
    log(f"A procurar emails recebidos desde: {since_dt.strftime('%d/%m/%Y %H:%M:%S')}")

    try:
        emails = get_emails(config, since_dt)
        log(f"Emails encontrados: {len(emails)}")
    except Exception as e:
        log(f"ERRO ao obter emails: {e}")
        return

    ensure_moodle_synced(config)
    moodle_lines = []
    moodle_ts = datetime.datetime.now().isoformat()
    if config.get("moodle_sync_dir"):
        since_moodle = datetime.datetime.fromisoformat(load_last_run_value("last_moodle_ts") or since_dt.isoformat())
        new_files = load_new_files(os.path.join(config["moodle_sync_dir"], "new_files.jsonl"), since_moodle)
        download_dir = config.get("moodle_download_dir") or os.path.expanduser(os.path.join("~", "Downloads", "Moodle_ISEP"))
        moodle_lines = build_moodle_section(new_files, download_dir)
        log(f"Ficheiros novos no Moodle: {len(new_files)}")

    sent_ids = set(load_sent_ids())
    emails = [e for e in emails if not e.get("message_id") or e["message_id"] not in sent_ids]
    log(f"Emails novos (ainda não enviados): {len(emails)}")

    if not emails and not moodle_lines:
        log("Nenhum email novo recebido. Nenhuma mensagem necessária.")
        update_last_run(moodle_ts=moodle_ts)
        log("=== EXECUÇÃO TERMINADA (SEM NOVOS EMAILS) ===")
        return

    log("A gerar resumo e priorização dos emails...")
    summary = summarize_emails(emails, config, moodle_lines)
    log("Resumo gerado com sucesso!")

    success = send_to_session(summary, config)
    if success:
        update_last_run([e["message_id"] for e in emails if e.get("message_id")], moodle_ts=moodle_ts)
        log("Estado atualizado com sucesso no last_run.json.")
    else:
        log("Aviso: last_run.json não foi atualizado devido a falha no envio.")

    log("=== EXECUÇÃO TERMINADA ===")

if __name__ == "__main__":
    main()
