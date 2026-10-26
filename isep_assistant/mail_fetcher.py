import os
import sys
import json
import datetime
import html
import re
import glob
import subprocess
import socket
import time
from typing import List, Dict, Any, Optional

def clean_html(raw_html: str) -> str:
    """Removes HTML tags and normalizes whitespace."""
    if not raw_html:
        return ""
    cleanr = re.compile(r'<.*?>')
    text = re.sub(cleanr, ' ', raw_html)
    text = html.unescape(text)
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return "\n".join(lines)

def decode_part(part) -> str:
    """Decodifica o corpo respeitando o charset declarado (emails antigos vêm em latin-1/cp1252)."""
    payload = part.get_payload(decode=True)
    if not payload:
        return ""
    charset = part.get_content_charset() or "utf-8"
    try:
        return payload.decode(charset, errors="replace")
    except LookupError:
        return payload.decode("utf-8", errors="replace")

def find_thunderbird_path() -> Optional[str]:
    candidates = [
        os.path.join(os.environ.get("LOCALAPPDATA", ""), "Programs", "Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("ProgramFiles", "C:\\Program Files"), "Mozilla Thunderbird", "thunderbird.exe"),
        os.path.join(os.environ.get("ProgramFiles(x86)", "C:\\Program Files (x86)"), "Mozilla Thunderbird", "thunderbird.exe"),
    ]
    for c in candidates:
        if c and os.path.exists(c):
            return c
    return None

def is_thunderbird_running() -> bool:
    try:
        res = subprocess.run(["tasklist", "/FI", "IMAGENAME eq thunderbird.exe", "/NH"], capture_output=True, text=True)
        return "thunderbird.exe" in res.stdout.lower()
    except Exception:
        return False

def wait_for_internet(max_wait_seconds: int = 35) -> bool:
    """Waits for active internet connectivity (useful right after boot/logon when Wi-Fi connects)."""
    start = time.time()
    while time.time() - start < max_wait_seconds:
        try:
            socket.create_connection(("1.1.1.1", 53), timeout=2).close()
            return True
        except OSError:
            time.sleep(3)
    return False

def find_thunderbird_inboxes() -> List[str]:
    tb_profiles = os.path.join(os.environ.get("APPDATA", ""), "Thunderbird", "Profiles")
    matches = []
    if os.path.exists(tb_profiles):
        for root, dirs, files in os.walk(tb_profiles):
            for file in files:
                if file.upper() in ["INBOX", "ENTRADA"] and not file.endswith(".msf"):
                    matches.append(os.path.join(root, file))
    return matches

def wait_for_inbox_to_settle(min_wait: int = 25, quiet_seconds: int = 20, max_wait: int = 180) -> None:
    """
    Waits at least min_wait seconds, then until the inbox file has stopped changing for
    quiet_seconds (IMAP sync after waking from sleep can take well over a fixed 25s).
    """
    def snapshot():
        try:
            return tuple((p, os.path.getmtime(p), os.path.getsize(p)) for p in find_thunderbird_inboxes())
        except OSError:
            return ()

    start = time.time()
    time.sleep(min_wait)
    last = snapshot()
    last_change = time.time()
    while time.time() - start < max_wait:
        time.sleep(3)
        cur = snapshot()
        if cur != last:
            last, last_change = cur, time.time()
        elif time.time() - last_change >= quiet_seconds:
            return

def sync_thunderbird_headless(timeout_seconds: int = 25) -> bool:
    """
    If Thunderbird is not running, launches it in headless mode in the background
    so it automatically connects to IMAP and downloads all new emails to disk,
    then terminates it cleanly without requiring any user interaction.
    """
    tb_path = find_thunderbird_path()
    if not tb_path:
        return False

    if is_thunderbird_running():
        # Already running in the background or GUI: wait for its sync to settle
        wait_for_inbox_to_settle(min_wait=3)
        return True

    try:
        proc = subprocess.Popen(
            [tb_path, "--headless"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)
        )
        wait_for_inbox_to_settle(min_wait=timeout_seconds)
        try:
            proc.terminate()
            proc.wait(timeout=5)
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass
        time.sleep(2)
        return True
    except Exception:
        return False

def fetch_emails_thunderbird(since_dt: datetime.datetime) -> List[Dict[str, Any]]:
    """Fetches emails from local Mozilla Thunderbird Mbox storage."""
    import mailbox
    import email
    from email.header import decode_header
    from email.utils import parsedate_to_datetime

    # 1. Ensure internet connection is ready (up to 30s)
    has_net = wait_for_internet(max_wait_seconds=30)
    if not has_net:
        raise ConnectionError("Sem ligação à Internet. A abortar sem avançar a data de última execução.")

    # 2. Automatically sync Thunderbird in headless background mode
    sync_thunderbird_headless(timeout_seconds=25)

    appdata = os.environ.get("APPDATA", "")
    tb_profiles = os.path.join(appdata, "Thunderbird", "Profiles")
    
    matches = []
    if os.path.exists(tb_profiles):
        for root, dirs, files in os.walk(tb_profiles):
            for file in files:
                if file.upper() in ["INBOX", "ENTRADA"] and not file.endswith(".msf"):
                    matches.append(os.path.join(root, file))

    if not matches:
        raise RuntimeError(
            "Caixa de entrada do Thunderbird não encontrada.\n"
            "Por favor abre o Thunderbird, adiciona a tua conta institucional (número@isep.ipp.pt) e aguarda 1 minuto até os emails sincronizarem."
        )

    # Pick the most recently modified inbox file
    matches.sort(key=lambda p: os.path.getmtime(p), reverse=True)
    inbox_path = matches[0]

    mbox = mailbox.mbox(inbox_path)
    emails = []

    for msg in mbox:
        try:
            if not msg:
                continue

            # Parse date
            date_str = msg.get("Date", "")
            item_dt = datetime.datetime.now()
            if date_str:
                try:
                    item_dt = parsedate_to_datetime(date_str)
                    if item_dt.tzinfo:
                        item_dt = item_dt.astimezone().replace(tzinfo=None)
                except Exception:
                    pass

            if item_dt < since_dt:
                continue

            # Decode subject
            raw_subj = msg.get("Subject", "Sem assunto")
            decoded_subject = ""
            for part, enc in decode_header(raw_subj):
                if isinstance(part, bytes):
                    decoded_subject += part.decode(enc or "utf-8", errors="ignore")
                else:
                    decoded_subject += str(part)

            # Sender
            raw_from = msg.get("From", "Desconhecido")
            decoded_from = ""
            for part, enc in decode_header(raw_from):
                if isinstance(part, bytes):
                    decoded_from += part.decode(enc or "utf-8", errors="ignore")
                else:
                    decoded_from += str(part)

            # Extract clean body text
            body = ""
            if msg.is_multipart():
                for p in msg.walk():
                    ctype = p.get_content_type()
                    if ctype == "text/plain":
                        body = decode_part(p)
                        break
                    elif ctype == "text/html" and not body:
                        body = clean_html(decode_part(p))
            else:
                body = decode_part(msg)
                if msg.get_content_type() == "text/html":
                    body = clean_html(body)

            emails.append({
                "id": str(len(emails) + 1),
                "message_id": (msg.get("Message-ID", "") or "").strip(),
                "subject": decoded_subject or "Sem assunto",
                "sender": decoded_from,
                "sender_email": decoded_from,
                "received_time": item_dt.isoformat(),
                "body": body[:4000]
            })
        except Exception:
            continue

    emails.sort(key=lambda x: x["received_time"], reverse=True)
    return emails

def fetch_emails_outlook_desktop(since_dt: datetime.datetime) -> List[Dict[str, Any]]:
    """Fetches emails from local Microsoft Outlook desktop client using Windows COM."""
    import win32com.client
    outlook = win32com.client.Dispatch("Outlook.Application")
    namespace = outlook.GetNamespace("MAPI")
    try:
        namespace.Logon("", "", False, False)
    except Exception:
        pass

    inbox = None
    try:
        inbox = namespace.GetDefaultFolder(6)
    except Exception:
        pass

    if not inbox:
        try:
            for folder in namespace.Folders:
                for sub in folder.Folders:
                    sub_name = sub.Name.lower()
                    if "inbox" in sub_name or "entrada" in sub_name or "receb" in sub_name:
                        inbox = sub
                        break
                if inbox:
                    break
        except Exception:
            pass

    if not inbox:
        raise RuntimeError("Nenhuma caixa de entrada encontrada no Outlook. Abre a aplicação Microsoft Outlook no Windows e adiciona a tua conta institucional (número@isep.ipp.pt).")

    messages = inbox.Items
    try:
        messages.Sort("[ReceivedTime]", True)
    except Exception:
        pass

    time_str = since_dt.strftime("%d/%m/%Y %H:%M")
    filter_expr = f"[ReceivedTime] >= '{time_str}'"
    
    try:
        filtered_items = messages.Restrict(filter_expr)
    except Exception:
        filtered_items = messages

    emails = []
    for item in filtered_items:
        try:
            received_time = item.ReceivedTime
            if hasattr(received_time, "year"):
                item_dt = datetime.datetime(
                    received_time.year, received_time.month, received_time.day,
                    received_time.hour, received_time.minute, received_time.second
                )
            else:
                item_dt = datetime.datetime.now()

            if item_dt < since_dt:
                continue

            subject = getattr(item, "Subject", "Sem assunto") or "Sem assunto"
            sender_name = getattr(item, "SenderName", "Desconhecido") or "Desconhecido"
            sender_email = getattr(item, "SenderEmailAddress", "") or ""
            body = getattr(item, "Body", "") or ""

            emails.append({
                "id": getattr(item, "EntryID", subject),
                "subject": subject,
                "sender": sender_name,
                "sender_email": sender_email,
                "received_time": item_dt.isoformat(),
                "body": body[:4000]
            })
        except Exception:
            continue

    return emails

def fetch_emails_graph(config: Dict[str, Any], since_dt: datetime.datetime) -> List[Dict[str, Any]]:
    """Fetches emails using Microsoft Graph API with direct token, token file, or MSAL token cache."""
    import msal
    import requests

    access_token = config.get("access_token")
    token_file = os.path.join(os.path.dirname(__file__), "token.txt")
    if not access_token and os.path.exists(token_file):
        with open(token_file, "r", encoding="utf-8") as f:
            t = f.read().strip()
            if t and not t.startswith("#") and len(t) > 30:
                access_token = t

    if not access_token:
        client_id = config.get("ms_client_id") or "de8bc8b5-d9f9-48b1-a8ad-b748da725064"
        tenant_id = config.get("ms_tenant_id") or "7a60a629-e61f-430c-9c63-266653b2b899"
        authority = f"https://login.microsoftonline.com/{tenant_id}"
        cache_path = os.path.join(os.path.dirname(__file__), "token_cache.bin")
        
        token_cache = msal.SerializableTokenCache()
        if os.path.exists(cache_path):
            with open(cache_path, "r", encoding="utf-8") as f:
                token_cache.deserialize(f.read())

        app = msal.PublicClientApplication(
            client_id=client_id,
            authority=authority,
            token_cache=token_cache
        )

        scopes = ["Mail.Read", "User.Read"]
        accounts = app.get_accounts()
        result = None

        if accounts:
            result = app.acquire_token_silent(scopes, account=accounts[0])

        if not result:
            flow = app.initiate_device_flow(scopes=scopes)
            if "user_code" not in flow:
                raise RuntimeError(f"Erro ao iniciar Device Flow: {flow.get('error_description')}")
            result = app.acquire_token_by_device_flow(flow)

        if token_cache.has_state_changed:
            with open(cache_path, "w", encoding="utf-8") as f:
                f.write(token_cache.serialize())

        if not result or "access_token" not in result:
            err = result.get("error_description") if result else "Sem resposta de autenticação"
            raise RuntimeError(f"Falha na autenticação Microsoft: {err}")

        access_token = result["access_token"]

    since_iso = since_dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    url = f"https://graph.microsoft.com/v1.0/me/messages?$filter=receivedDateTime ge {since_iso}&$orderby=receivedDateTime desc&$top=50&$select=id,subject,from,receivedDateTime,bodyPreview,body"

    resp = requests.get(url, headers={"Authorization": f"Bearer {access_token}"})
    if resp.status_code != 200:
        raise RuntimeError(f"Graph API Error {resp.status_code}: {resp.text}")

    data = resp.json()
    emails = []
    for item in data.get("value", []):
        sender_info = item.get("from", {}).get("emailAddress", {})
        body_content = item.get("body", {}).get("content", "")
        clean_text = clean_html(body_content) if body_content else item.get("bodyPreview", "")

        emails.append({
            "id": item.get("id"),
            "subject": item.get("subject", "Sem assunto"),
            "sender": sender_info.get("name", "Desconhecido"),
            "sender_email": sender_info.get("address", ""),
            "received_time": item.get("receivedDateTime"),
            "body": clean_text[:4000]
        })

    return emails

def fetch_emails_imap(config: Dict[str, Any], since_dt: datetime.datetime) -> List[Dict[str, Any]]:
    """Fetches emails using IMAP SSL with App Password."""
    import imaplib
    import email
    from email.header import decode_header

    host = config.get("imap_host", "outlook.office365.com")
    port = int(config.get("imap_port", 993))
    user = config.get("email_address")
    password = config.get("email_password")

    if not user or not password:
        raise ValueError("Endereço de email e palavra-passe são obrigatórios para IMAP.")

    mail = imaplib.IMAP4_SSL(host, port)
    mail.login(user, password)
    mail.select("INBOX", readonly=True)

    date_str = since_dt.strftime("%d-%b-%Y")
    status, messages = mail.search(None, f'(SINCE "{date_str}")')
    if status != "OK":
        mail.close()
        mail.logout()
        return []

    msg_ids = messages[0].split()
    emails = []

    for msg_id in reversed(msg_ids[-30:]):
        res, msg_data = mail.fetch(msg_id, "(RFC822)")
        if res != "OK":
            continue
        raw_email = msg_data[0][1]
        msg = email.message_from_bytes(raw_email)

        raw_subject = msg.get("Subject", "Sem assunto")
        decoded_subject = ""
        for part, encoding in decode_header(raw_subject):
            if isinstance(part, bytes):
                decoded_subject += part.decode(encoding or "utf-8", errors="ignore")
            else:
                decoded_subject += str(part)

        raw_from = msg.get("From", "Desconhecido")
        
        body = ""
        if msg.is_multipart():
            for p in msg.walk():
                ctype = p.get_content_type()
                if ctype == "text/plain":
                    body = decode_part(p)
                    break
                elif ctype == "text/html" and not body:
                    body = clean_html(decode_part(p))
        else:
            body = decode_part(msg)
            if msg.get_content_type() == "text/html":
                body = clean_html(body)

        emails.append({
            "id": msg_id.decode("utf-8"),
            "subject": decoded_subject or "Sem assunto",
            "sender": raw_from,
            "sender_email": raw_from,
            "received_time": msg.get("Date", ""),
            "body": body[:4000]
        })

    mail.close()
    mail.logout()
    return emails

def get_emails(config: Dict[str, Any], since_dt: datetime.datetime) -> List[Dict[str, Any]]:
    """Master function that chooses the configured retrieval method."""
    method = config.get("email_method", "thunderbird").lower()

    if method == "thunderbird":
        return fetch_emails_thunderbird(since_dt)
    elif method == "outlook_desktop":
        return fetch_emails_outlook_desktop(since_dt)
    elif method == "graph_api":
        return fetch_emails_graph(config, since_dt)
    elif method == "imap":
        return fetch_emails_imap(config, since_dt)
    else:
        return fetch_emails_thunderbird(since_dt)
