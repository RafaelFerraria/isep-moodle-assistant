"""Testes de contrato: o MESMO conjunto de pedidos HTTP corre contra o servidor Python e o servidor Rust.

Garantem que as duas implementações respeitam docs/API.md e que a interface (React) funciona com qualquer uma.
O servidor Rust só é testado se existir o executável (cargo build em server/).
"""
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
RUST_EXE = REPO / "server" / "target" / "debug" / "isep-assistant-api.exe"
SID = "05" + "ab" * 32

IMPLEMENTATIONS = ["python"] + (["rust"] if RUST_EXE.exists() else [])


def http(method, url, token=None, body=None, host=None):
    req = urllib.request.Request(url, method=method, data=json.dumps(body).encode() if body is not None else None)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    if host:
        req.add_header("Host", host)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status, json.loads(resp.read().decode() or "null"), dict(resp.headers)
    except urllib.error.HTTPError as err:
        text = err.read().decode() or "null"
        try:
            data = json.loads(text)
        except ValueError:
            data = text
        return err.code, data, dict(err.headers)


def shape(value):
    """Forma de um JSON: tipos em vez de valores (listas pela forma do primeiro elemento)."""
    if isinstance(value, dict):
        return {k: shape(v) for k, v in sorted(value.items())}
    if isinstance(value, list):
        return [shape(value[0])] if value else []
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, (int, float)):
        return "number"
    if value is None:
        return "null"
    return "str"


@pytest.fixture(params=IMPLEMENTATIONS)
def server(request, tmp_path):
    impl = request.param
    (tmp_path / "EmailNotifier").mkdir()
    (tmp_path / "MoodleSync").mkdir()
    (tmp_path / "EmailNotifier" / "config.json").write_text(json.dumps({
        "session_recipients": [SID], "session_mnemonic": "segredo secreto", "scheduled_time": "18:00",
        "email_address": "n@isep.ipp.pt", "email_method": "imap", "moodle_download_dir": str(tmp_path / "dl"),
    }), encoding="utf-8")
    env = dict(os.environ, ISEP_ASSISTANT_ROOT=str(tmp_path), ISEP_ASSISTANT_STATE=str(tmp_path / "state"),
               ISEP_FAKE_SCHEDULER="1", PYTHONIOENCODING="utf-8")
    for var in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "AI_API_KEY"):
        env.pop(var, None)
    if impl == "python":
        env["PYTHONPATH"] = os.pathsep.join(filter(None, [str(REPO), str(REPO / ".deps"), env.get("PYTHONPATH", "")]))
        cmd = [sys.executable, "-m", "isep_assistant.api.server"]
    else:
        cmd = [str(RUST_EXE), "serve"]
    proc = subprocess.Popen(cmd, cwd=str(tmp_path), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    state_file = tmp_path / "state" / "api.json"
    deadline = time.time() + 30
    while time.time() < deadline and not state_file.exists():
        if proc.poll() is not None:
            pytest.fail(f"{impl}: servidor terminou: {proc.stdout.read()}")
        time.sleep(0.1)
    state = json.loads(state_file.read_text(encoding="utf-8"))
    base = f"http://127.0.0.1:{state['port']}/api/v1"
    for _ in range(100):
        try:
            http("GET", base + "/health")
            break
        except OSError:
            time.sleep(0.1)
    yield type("Server", (), {"impl": impl, "base": base, "token": state["token"], "root": tmp_path, "state": state})
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()


def get(srv, path, **kw):
    return http("GET", srv.base + path, token=srv.token, **kw)


def wait_job(srv, job_id):
    end = time.time() + 20
    while time.time() < end:
        code, job, _ = get(srv, f"/jobs/{job_id}")
        assert code == 200
        if job["status"] != "running":
            return job
        time.sleep(0.05)
    raise AssertionError("a tarefa não terminou")


def test_state_file_and_health(server):
    assert server.state["port"] > 0 and len(server.state["token"]) >= 32 and server.state["pid"] > 0
    code, body, _ = http("GET", server.base + "/health")
    assert code == 200 and body["ok"] is True and isinstance(body["version"], str)


def test_auth_host_and_no_cors(server):
    assert http("GET", server.base + "/status")[0] == 401
    assert http("GET", server.base + "/status", token="errado")[0] == 401
    assert http("POST", server.base + "/digest/run")[0] == 401
    assert http("PUT", server.base + "/settings", body={})[0] == 401
    assert http("GET", server.base + "/health", host="evil.example.com")[0] == 403
    code, _, headers = http("GET", server.base + "/health")
    assert not any(k.lower().startswith("access-control") for k in headers)
    assert http("GET", server.base + "/nao-existe", token=server.token)[0] == 404


def test_status_shape_and_content(server):
    code, st, _ = get(server, "/status")
    assert code == 200
    assert {"version", "now", "ok", "checks", "summary_mode", "digest", "moodle", "tasks", "active_job"} <= set(st)
    assert {c["id"] for c in st["checks"]} == {"config", "session", "thunderbird", "node", "moodle", "tasks", "internet", "ai"}
    for c in st["checks"]:
        assert set(c) == {"id", "label", "ok", "detail", "required"}
    assert st["digest"]["next_run"] == "2026-10-27T18:00:00" and st["digest"]["scheduled_time"] == "18:00"
    assert st["summary_mode"]["mode"] == "rules"
    assert "segredo" not in json.dumps(st)


def test_settings_shape_masking_and_roundtrip(server):
    code, s, _ = get(server, "/settings")
    assert code == 200
    assert set(s) == {"summary", "schedule", "destination", "email", "moodle"}
    assert set(s["summary"]) == {"mode", "ai_provider", "ai_cli", "ai_cli_command", "ai_api_type", "ai_model", "ai_base_url",
                                 "ai_api_key_set", "ai_api_key_env", "ai_api_key_env_set"}
    assert s["destination"]["session_configured"] is True
    assert SID not in json.dumps(s) and "segredo" not in json.dumps(s)

    code, r, _ = http("PUT", server.base + "/settings", token=server.token, body={"summary": {
        "mode": "ai", "ai_provider": "api", "ai_api_type": "anthropic", "ai_model": "claude-haiku-5-5", "ai_api_key": "sk-ant-SEGREDO"}})
    assert code == 200 and "sk-ant-SEGREDO" not in json.dumps(r)
    assert set(r) >= {"settings", "effects"}
    assert r["settings"]["summary"]["mode"] == "ai" and r["settings"]["summary"]["ai_api_key_set"] is True
    saved = json.loads((server.root / "EmailNotifier" / "config.json").read_text(encoding="utf-8-sig"))
    assert saved["ai_api_key"] == "sk-ant-SEGREDO" and saved["session_mnemonic"] == "segredo secreto"
    assert "sk-ant-SEGREDO" not in json.dumps(get(server, "/settings")[1])
    cleared = http("PUT", server.base + "/settings", token=server.token, body={"summary": {"clear_ai_api_key": True}})[1]
    assert cleared["settings"]["summary"]["ai_api_key_set"] is False


def test_settings_validation(server):
    code, r, _ = http("PUT", server.base + "/settings", token=server.token, body={
        "summary": {"mode": "x", "ai_cli": "y", "ai_base_url": "ftp://a"}, "schedule": {"time": "25:99"}, "destination": {"session_id": "1"}})
    assert code == 422
    assert set(r["errors"]) == {"summary.mode", "summary.ai_cli", "summary.ai_base_url", "schedule.time", "destination.session_id"}
    code, r, _ = http("PUT", server.base + "/settings", token=server.token,
                      body={"summary": {"mode": "ai", "ai_provider": "api", "ai_api_type": "openai_compatible"}})
    assert code == 422 and {"summary.ai_base_url", "summary.ai_model"} <= set(r["errors"])
    code, _, _ = http("PUT", server.base + "/settings", token=server.token, body=[1])
    assert code == 422


def test_time_change_effect_and_session_id(server):
    code, r, _ = http("PUT", server.base + "/settings", token=server.token, body={"schedule": {"time": "9:30"}, "destination": {"session_id": SID}})
    assert code == 200 and r["effects"] == ["reschedule"] and r["reschedule"]["ok"] is True
    assert r["settings"]["schedule"]["time"] == "09:30"
    code, r, _ = http("PUT", server.base + "/settings", token=server.token, body={"schedule": {"time": "09:30"}})
    assert r["effects"] == []


def test_summary_test_endpoint(server):
    code, r, _ = http("POST", server.base + "/summary/test", token=server.token)
    assert code == 200 and set(r) == {"text", "ai_requested", "ai_fallback", "ok"}
    assert r["ok"] is True and r["ai_requested"] is False and r["text"].startswith("📬 RESUMO ISEP")
    http("PUT", server.base + "/settings", token=server.token, body={"summary": {"mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}})
    code, r, _ = http("POST", server.base + "/summary/test", token=server.token)
    assert r["ai_requested"] is True and r["ai_fallback"] is True and r["ok"] is False and "IA indisponível" in r["text"]


def test_digest_preview_job_lifecycle_and_shapes(server):
    code, job, _ = http("POST", server.base + "/digest/preview", token=server.token)
    assert code == 202
    assert set(job) == {"id", "kind", "status", "started", "finished", "progress", "progress_total", "result", "error"}
    done = wait_job(server, job["id"])
    assert done["status"] == "done" and done["finished"]
    res = done["result"]
    assert set(res) == {"ok", "sent", "skipped", "preview", "emails_found", "emails_new", "moodle_files",
                        "ai_used", "ai_fallback", "summary", "error", "log"}
    assert res["preview"] is True and res["ok"] is False and res["error"]  # email_method "imap" sem credenciais falha depressa
    assert any("PRÉ-VISUALIZAÇÃO" in line for line in done["progress"])
    assert get(server, "/history")[1]["items"] == [], "a pré-visualização não entra no histórico"


def test_digest_run_failure_is_recorded_in_history(server):
    code, job, _ = http("POST", server.base + "/digest/run", token=server.token)
    done = wait_job(server, job["id"])
    assert done["status"] == "done" and done["result"]["ok"] is False
    code, h, _ = get(server, "/history")
    assert len(h["items"]) == 1
    item = h["items"][0]
    assert set(item) >= {"id", "kind", "trigger", "ok", "started", "finished", "duration_s", "error", "details", "summary", "has_summary"}
    assert item["kind"] == "digest" and item["trigger"] == "api" and item["ok"] is False and item["summary"] is None
    detail = get(server, f"/history/{item['id']}")
    assert detail[0] == 200 and detail[1]["id"] == item["id"]
    assert set(detail[1]["details"]) == {"emails_found", "emails_new", "moodle_files", "sent", "skipped", "ai_used", "ai_fallback"}


def test_jobs_endpoints_and_validation(server):
    assert get(server, "/jobs/naoexiste")[0] == 404
    assert get(server, "/jobs")[1] == {"jobs": []}
    assert get(server, "/jobs?limit=0")[0] == 422
    assert get(server, "/history?kind=outro")[0] == 422
    assert get(server, "/history?limit=0")[0] == 422
    assert get(server, "/history?limit=201")[0] == 422
    assert get(server, "/history/xxxx")[0] == 404
    assert get(server, "/moodle/new-files")[1] == {"items": []}
    assert get(server, "/moodle/new-files?limit=0")[0] == 422


def test_logs_endpoint(server):
    (server.root / "EmailNotifier" / "notifier.log").write_text("\n".join(f"linha {i}" for i in range(50)), encoding="utf-8")
    assert get(server, "/logs/notifier?lines=3")[1]["lines"] == ["linha 47", "linha 48", "linha 49"]
    assert get(server, "/logs/sync")[1]["lines"] == []
    assert get(server, "/logs/passwords")[0] == 404


def test_new_files_listing_latest_first(server):
    (server.root / "MoodleSync" / "config.json").write_text("{}", encoding="utf-8")
    lines = [json.dumps({"ts": f"2026-10-2{i}T10:00:00", "kind": "novo", "course": "C", "section": "S", "name": f"f{i}.pdf",
                         "path": f"f{i}.pdf", "size": 1}) for i in range(1, 4)]
    (server.root / "MoodleSync" / "new_files.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
    items = get(server, "/moodle/new-files?limit=2")[1]["items"]
    assert [i["name"] for i in items] == ["f3.pdf", "f2.pdf"]
    assert set(items[0]) == {"ts", "kind", "course", "section", "name", "path", "size"}


def test_moodle_credentials_validation(server):
    assert http("PUT", server.base + "/moodle/credentials", token=server.token, body={"username": "x"})[0] == 422
    # sem rede para o Moodle real nos testes: um login inválido tem de dar 400 com mensagem, nunca 500
    code, r, _ = http("PUT", server.base + "/moodle/credentials", token=server.token, body={"username": "", "password": ""})
    assert code == 400 and isinstance(r["detail"], str)
