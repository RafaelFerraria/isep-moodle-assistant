"""Envio de mensagens para o Session (via session_sender.cjs, Node.js)."""
import os
import subprocess
import tempfile
from typing import Any, Callable, Dict

from . import paths
from .config_store import recipients_of

SEND_TIMEOUT = 120


def send_to_session(text: str, config: Dict[str, Any], log: Callable[[str], None] = print) -> bool:
    """Envia 'text' para todos os destinatários configurados; True só se todos receberam."""
    recipients = recipients_of(config)
    mnemonic = config.get("session_mnemonic")
    if not recipients or not mnemonic:
        log("ERRO: Nenhum destinatário ou session_mnemonic não configurados no config.json!")
        return False

    script = paths.session_script()
    fd, temp_msg_file = tempfile.mkstemp(prefix="_msg_", suffix=".txt", dir=str(paths.email_dir()))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
        all_success = True
        for target in recipients:
            log(f"A enviar resumo para o Session (Destino: {target[:10]}...)...")
            cmd = ["node", str(script), "--to", target, "--file", temp_msg_file, "--mnemonic", mnemonic]
            try:
                subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", check=True, timeout=SEND_TIMEOUT)
                log(f"Mensagem Session enviada com sucesso para {target[:10]}...!")
            except subprocess.CalledProcessError as err:
                # a mnemónica vai na linha de comandos: nunca registar 'cmd', só a saída do processo
                log(f"FALHA ao enviar para {target[:10]}...: {(err.stderr or err.stdout or '').strip()[:300]}")
                all_success = False
            except (OSError, subprocess.TimeoutExpired) as err:
                log(f"FALHA ao enviar para {target[:10]}...: {type(err).__name__}")
                all_success = False
        return all_success
    finally:
        try:
            os.remove(temp_msg_file)
        except OSError:
            pass
