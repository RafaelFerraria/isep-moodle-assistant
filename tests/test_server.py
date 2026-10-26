"""Arranque real do servidor (processo separado) e pedidos HTTP reais."""
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent


def http(method, url, token=None, body=None, host=None):
    req = urllib.request.Request(url, method=method, data=json.dumps(body).encode() if body is not None else None)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    if host:
        req.add_header("Host", host)
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.status, json.loads(resp.read().decode())
    except urllib.error.HTTPError as err:
        return err.code, json.loads(err.read().decode() or "{}")


@pytest.fixture
def server(tmp_path):
    (tmp_path / "EmailNotifier").mkdir()
    (tmp_path / "MoodleSync").mkdir()
    env = dict(os.environ, ISEP_ASSISTANT_ROOT=str(tmp_path), ISEP_ASSISTANT_STATE=str(tmp_path / "state"),
               PYTHONIOENCODING="utf-8")
    env["PYTHONPATH"] = os.pathsep.join(filter(None, [str(REPO), str(REPO / ".deps"), env.get("PYTHONPATH", "")]))
    proc = subprocess.Popen([sys.executable, "-m", "isep_assistant.api.server"], cwd=str(tmp_path), env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    state_file = tmp_path / "state" / "api.json"
    deadline = time.time() + 20
    while time.time() < deadline and not state_file.exists():
        if proc.poll() is not None:
            pytest.fail(f"servidor terminou: {proc.stdout.read()}")
        time.sleep(0.1)
    state = json.loads(state_file.read_text(encoding="utf-8"))
    base = f"http://127.0.0.1:{state['port']}/api/v1"
    for _ in range(100):  # espera que aceite ligações
        try:
            http("GET", base + "/health")
            break
        except OSError:
            time.sleep(0.1)
    yield base, state, tmp_path
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()


def test_real_server_end_to_end(server):
    base, state, root = server
    assert state["port"] > 0 and len(state["token"]) >= 32

    assert http("GET", base + "/health")[0] == 200
    assert http("GET", base + "/status")[0] == 401
    assert http("GET", base + "/status", token="errado")[0] == 401
    assert http("GET", base + "/health", host="evil.example.com")[0] == 403

    code, status = http("GET", base + "/status", token=state["token"])
    assert code == 200 and status["summary_mode"]["mode"] == "rules"

    code, saved = http("PUT", base + "/settings", token=state["token"],
                       body={"summary": {"mode": "ai", "ai_provider": "cli", "ai_cli": "claude"}})
    assert code == 200 and saved["settings"]["summary"]["ai_cli"] == "claude"
    persisted = json.loads((root / "EmailNotifier" / "config.json").read_text(encoding="utf-8"))
    assert persisted["summary_mode"] == "ai"

    code, err = http("PUT", base + "/settings", token=state["token"], body={"summary": {"mode": "x"}})
    assert code == 422 and "summary.mode" in err["errors"]


def test_server_listens_only_on_loopback(server):
    base, state, _ = server
    import socket
    # o endereço de bind é 127.0.0.1: ligar por outro IP local da máquina tem de falhar
    local_ips = {i[4][0] for i in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET)} - {"127.0.0.1"}
    for ip in local_ips:
        with pytest.raises(OSError):
            socket.create_connection((ip, state["port"]), timeout=1).close()
