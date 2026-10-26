import json
import threading
import time

import pytest
from fastapi.testclient import TestClient

from isep_assistant import config_store, history, service, system
from isep_assistant.api import app as api_app
from isep_assistant.api.app import API_PREFIX, create_app
from isep_assistant.api.jobs import JobManager
from isep_assistant.api.security import host_allowed

TOKEN = "token-de-teste"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
SID = "05" + "ef" * 32


@pytest.fixture
def jobs():
    return JobManager()


@pytest.fixture
def client(home, jobs, monkeypatch):
    # nada de PowerShell/rede nos testes
    monkeypatch.setattr(system, "query_tasks", lambda *a, **k: [
        {"name": "ISEP_Email_Session_Daily", "state": "Ready", "last_run": "2026-10-26T18:00:00",
         "next_run": "2026-10-27T18:00:00", "last_result": 0}])
    monkeypatch.setattr(system, "internet_ok", lambda *a, **k: True)
    monkeypatch.setattr(system, "find_thunderbird_path", lambda: "C:/tb/thunderbird.exe")
    monkeypatch.setattr(system, "node_path", lambda: "C:/node.exe")
    config_store.save_config({"session_recipients": [SID], "session_mnemonic": "segredo", "scheduled_time": "18:00",
                              "email_address": "n@isep.ipp.pt"})
    return TestClient(create_app(TOKEN, jobs), base_url="http://127.0.0.1:8765")


def url(path):
    return API_PREFIX + path


def wait_job(client, job_id, timeout=5.0):
    end = time.time() + timeout
    while time.time() < end:
        data = client.get(url(f"/jobs/{job_id}"), headers=AUTH).json()
        if data["status"] != "running":
            return data
        time.sleep(0.02)
    raise AssertionError("tarefa não terminou")


# ---------------------------------------------------------------- segurança

def test_health_is_public_but_everything_else_needs_the_token(client):
    assert client.get(url("/health")).json()["ok"] is True
    for path in ("/status", "/settings", "/history", "/jobs", "/moodle/new-files", "/logs/notifier"):
        assert client.get(url(path)).status_code == 401
        assert client.get(url(path), headers={"Authorization": "Bearer errado"}).status_code == 401
    for path in ("/digest/run", "/digest/preview", "/moodle/sync", "/summary/test", "/schedule/register"):
        assert client.post(url(path)).status_code == 401
    assert client.put(url("/settings"), json={}).status_code == 401


def test_foreign_host_header_is_rejected(client):
    # protege contra DNS rebinding: um site com outro nome apontado a 127.0.0.1
    r = client.get(url("/health"), headers={"Host": "evil.example.com"})
    assert r.status_code == 403


def test_host_allowlist():
    for ok in ("127.0.0.1:8765", "localhost:1", "localhost", "[::1]:9"):
        assert host_allowed(ok)
    for bad in ("evil.com", "127.0.0.1.evil.com:80", "", None, "0.0.0.0:80"):
        assert not host_allowed(bad)


def test_no_cors_headers_and_no_docs(client):
    r = client.get(url("/health"), headers={"Origin": "https://evil.example"})
    assert "access-control-allow-origin" not in r.headers
    assert client.get("/docs").status_code == 404


# ---------------------------------------------------------------- estado

def test_status_reports_checks_next_run_and_mode(client):
    data = client.get(url("/status"), headers=AUTH).json()
    checks = {c["id"]: c for c in data["checks"]}
    assert checks["session"]["ok"] and checks["thunderbird"]["ok"] and checks["tasks"]["ok"]
    assert checks["moodle"]["ok"] is False and checks["moodle"]["required"] is False
    assert data["digest"]["next_run"] == "2026-10-27T18:00:00" and data["digest"]["scheduled_time"] == "18:00"
    assert data["summary_mode"]["mode"] == "rules" and data["ok"] is True and data["active_job"] is None
    assert "segredo" not in json.dumps(data)


def test_status_flags_missing_pieces(client, monkeypatch):
    monkeypatch.setattr(system, "node_path", lambda: None)
    config_store.save_config({})
    data = client.get(url("/status"), headers=AUTH).json()
    failing = {c["id"] for c in data["checks"] if not c["ok"] and c["required"]}
    assert {"config", "session", "node"} <= failing and data["ok"] is False


def test_status_reports_ai_readiness(client):
    config_store.save_config({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"})
    ai = {c["id"]: c for c in client.get(url("/status"), headers=AUTH).json()["checks"]}["ai"]
    assert ai["ok"] is False and "OPENAI_API_KEY" in ai["detail"]
    config_store.save_config({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai", "ai_api_key": "k"})
    assert {c["id"]: c for c in client.get(url("/status"), headers=AUTH).json()["checks"]}["ai"]["ok"] is True
    config_store.save_config({"summary_mode": "ai", "ai_provider": "cli", "ai_cli": "custom",
                              "ai_cli_command": ["comando-inexistente-xyz"]})
    assert {c["id"]: c for c in client.get(url("/status"), headers=AUTH).json()["checks"]}["ai"]["ok"] is False


# ---------------------------------------------------------------- definições

def test_get_settings_is_masked(client):
    s = client.get(url("/settings"), headers=AUTH).json()
    assert s["destination"]["session_configured"] is True and SID not in json.dumps(s) and "segredo" not in json.dumps(s)


def test_put_settings_persists_and_never_echoes_the_api_key(client):
    r = client.put(url("/settings"), headers=AUTH, json={"summary": {
        "mode": "ai", "ai_provider": "api", "ai_api_type": "anthropic", "ai_model": "claude-haiku-5-5", "ai_api_key": "sk-ant-segredo"}})
    assert r.status_code == 200
    assert "sk-ant-segredo" not in r.text
    body = r.json()["settings"]["summary"]
    assert body["mode"] == "ai" and body["ai_api_key_set"] is True and body["ai_model"] == "claude-haiku-5-5"
    saved = config_store.load_config()
    assert saved["ai_api_key"] == "sk-ant-segredo" and saved["session_mnemonic"] == "segredo"  # o resto ficou intacto
    again = client.get(url("/settings"), headers=AUTH)
    assert "sk-ant-segredo" not in again.text
    cleared = client.put(url("/settings"), headers=AUTH, json={"summary": {"clear_ai_api_key": True}}).json()
    assert cleared["settings"]["summary"]["ai_api_key_set"] is False


def test_put_settings_validation_errors_are_422_with_fields(client):
    r = client.put(url("/settings"), headers=AUTH, json={"summary": {"mode": "x"}, "schedule": {"time": "99:99"}})
    assert r.status_code == 422 and set(r.json()["errors"]) == {"summary.mode", "schedule.time"}
    assert config_store.load_config().get("summary_mode") is None  # nada foi gravado


def test_changing_the_time_reregisters_the_windows_tasks(client, monkeypatch):
    calls = []
    monkeypatch.setattr(system, "register_tasks", lambda t, *a, **k: calls.append(t) or (True, "ok"))
    r = client.put(url("/settings"), headers=AUTH, json={"schedule": {"time": "9:30"}}).json()
    assert calls == ["09:30"] and r["effects"] == ["reschedule"] and r["reschedule"]["ok"] is True
    client.put(url("/settings"), headers=AUTH, json={"schedule": {"time": "09:30"}})
    assert calls == ["09:30"]  # sem alteração, sem novo registo


def test_schedule_register_endpoint(client, monkeypatch):
    monkeypatch.setattr(system, "register_tasks", lambda t, *a, **k: (True, f"registado {t}"))
    assert client.post(url("/schedule/register"), headers=AUTH).json() == {"ok": True, "message": "registado 18:00"}


def test_summary_test_uses_current_mode_and_reports_fallback(client):
    ok = client.post(url("/summary/test"), headers=AUTH).json()
    assert ok["ok"] and not ok["ai_requested"] and ok["text"].startswith("📬 RESUMO ISEP")
    config_store.save_config({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"})
    bad = client.post(url("/summary/test"), headers=AUTH).json()
    assert bad["ai_requested"] and bad["ai_fallback"] and not bad["ok"] and "IA indisponível" in bad["text"]


def test_moodle_credentials_validate_login_and_store_encrypted(client, home, monkeypatch):
    from isep_assistant import moodle_sync as ms
    monkeypatch.setattr(ms.MoodleISEPSync, "login", lambda self: self.password == "boa")
    monkeypatch.setattr(ms, "encrypt_dpapi", lambda text: "CIFRADO:" + text[::-1])
    bad = client.put(url("/moodle/credentials"), headers=AUTH, json={"username": "1234567@isep.ipp.pt", "password": "ma"})
    assert bad.status_code == 400 and not (home / "MoodleSync" / "config.json").exists()
    good = client.put(url("/moodle/credentials"), headers=AUTH, json={"username": "1234567@isep.ipp.pt", "password": "boa"})
    assert good.json() == {"ok": True, "username": "1234567"} and "boa" not in good.text
    stored = json.loads((home / "MoodleSync" / "config.json").read_text(encoding="utf-8"))
    assert stored["username"] == "1234567" and stored["encrypted_password"] == "CIFRADO:aob"
    assert "boa" not in json.dumps({k: v for k, v in stored.items() if k != "encrypted_password"})
    status = client.get(url("/status"), headers=AUTH).json()
    assert status["moodle"]["configured"] is True and status["moodle"]["username"] == "1234567"


# ---------------------------------------------------------------- tarefas em segundo plano

def test_digest_preview_runs_in_background_and_returns_the_summary(client, monkeypatch):
    from isep_assistant.digest import DigestResult
    seen = {}

    def fake_run(send, trigger, progress):
        seen.update(send=send, trigger=trigger)
        progress("a ler emails")
        return DigestResult(ok=True, preview=True, summary="RESUMO DE TESTE", emails_new=2)
    monkeypatch.setattr(api_app, "run_digest", fake_run)
    r = client.post(url("/digest/preview"), headers=AUTH)
    assert r.status_code == 202
    job = wait_job(client, r.json()["id"])
    assert job["status"] == "done" and job["result"]["summary"] == "RESUMO DE TESTE"
    assert job["progress"] == ["a ler emails"] and seen == {"send": False, "trigger": "api"}


def test_digest_run_sends_for_real(client, monkeypatch):
    from isep_assistant.digest import DigestResult
    seen = {}
    monkeypatch.setattr(api_app, "run_digest", lambda send, trigger, progress: seen.setdefault("send", send) and DigestResult(ok=True, sent=True))
    job = wait_job(client, client.post(url("/digest/run"), headers=AUTH).json()["id"])
    assert job["status"] == "done" and job["result"]["sent"] is True and seen["send"] is True


def test_failed_job_reports_error_and_server_survives(client, monkeypatch):
    def boom(send, trigger, progress):
        raise RuntimeError("explodiu")
    monkeypatch.setattr(api_app, "run_digest", boom)
    job = wait_job(client, client.post(url("/digest/run"), headers=AUTH).json()["id"])
    assert job["status"] == "failed" and "explodiu" in job["error"]
    assert client.get(url("/health")).status_code == 200


def test_only_one_job_at_a_time_returns_409(client, monkeypatch):
    from isep_assistant.digest import DigestResult
    gate = threading.Event()

    def slow(send, trigger, progress):
        gate.wait(5)
        return DigestResult(ok=True)
    monkeypatch.setattr(api_app, "run_digest", slow)
    first = client.post(url("/digest/run"), headers=AUTH)
    second = client.post(url("/digest/preview"), headers=AUTH)
    assert first.status_code == 202 and second.status_code == 409
    assert second.json()["active_job_id"] == first.json()["id"]
    assert client.get(url("/status"), headers=AUTH).json()["active_job"]["id"] == first.json()["id"]
    gate.set()
    wait_job(client, first.json()["id"])
    assert client.post(url("/digest/preview"), headers=AUTH).status_code == 202


def test_moodle_sync_job(client, monkeypatch):
    from isep_assistant import moodle_sync as ms
    monkeypatch.setattr(ms, "run_sync", lambda trigger, progress: (progress("a sincronizar"), ms.SyncResult(ok=True, new_files=3))[1])
    job = wait_job(client, client.post(url("/moodle/sync"), headers=AUTH).json()["id"])
    assert job["status"] == "done" and job["result"]["new_files"] == 3 and job["progress"] == ["a sincronizar"]


def test_job_progress_can_be_polled_incrementally(client, monkeypatch):
    from isep_assistant.digest import DigestResult

    def work(send, trigger, progress):
        for i in range(5):
            progress(f"linha {i}")
        return DigestResult(ok=True)
    monkeypatch.setattr(api_app, "run_digest", work)
    job = wait_job(client, client.post(url("/digest/run"), headers=AUTH).json()["id"])
    tail = client.get(url(f"/jobs/{job['id']}?since_line=3"), headers=AUTH).json()
    assert tail["progress"] == ["linha 3", "linha 4"] and tail["progress_total"] == 5
    assert client.get(url("/jobs/naoexiste"), headers=AUTH).status_code == 404
    assert client.get(url("/jobs"), headers=AUTH).json()["jobs"][0]["id"] == job["id"]


# ---------------------------------------------------------------- histórico, ficheiros novos, logs

def test_history_listing_filtering_and_detail(client):
    a = history.append(history.new_record("digest", "scheduled", True, "2026-10-26T18:00:00", summary="O RESUMO COMPLETO"))
    history.append(history.new_record("moodle_sync", "manual", False, "2026-10-26T18:10:00", error="DNS"))
    items = client.get(url("/history"), headers=AUTH).json()["items"]
    assert [i["kind"] for i in items] == ["moodle_sync", "digest"] and all(i["summary"] is None for i in items)
    only = client.get(url("/history?kind=digest"), headers=AUTH).json()["items"]
    assert len(only) == 1 and only[0]["has_summary"] is True
    assert client.get(url(f"/history/{a['id']}"), headers=AUTH).json()["summary"] == "O RESUMO COMPLETO"
    assert client.get(url("/history/xxxx"), headers=AUTH).status_code == 404
    assert client.get(url("/history?kind=outro"), headers=AUTH).status_code == 422
    assert client.get(url("/history?limit=0"), headers=AUTH).status_code == 422


def test_new_files_endpoint_lists_latest_first(client, home):
    (home / "MoodleSync" / "config.json").write_text("{}", encoding="utf-8")
    lines = [json.dumps({"ts": f"2026-10-2{i}T10:00:00", "kind": "novo", "course": "C", "section": "S",
                         "name": f"f{i}.pdf", "path": f"f{i}.pdf", "size": 1}) for i in range(1, 4)]
    (home / "MoodleSync" / "new_files.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
    items = client.get(url("/moodle/new-files?limit=2"), headers=AUTH).json()["items"]
    assert [i["name"] for i in items] == ["f3.pdf", "f2.pdf"]


def test_logs_tail_and_unknown_name(client, home):
    (home / "EmailNotifier" / "notifier.log").write_text("\n".join(f"linha {i}" for i in range(50)), encoding="utf-8")
    lines = client.get(url("/logs/notifier?lines=3"), headers=AUTH).json()["lines"]
    assert lines == ["linha 47", "linha 48", "linha 49"]
    assert client.get(url("/logs/sync"), headers=AUTH).json()["lines"] == []
    assert client.get(url("/logs/passwords"), headers=AUTH).status_code == 404
    assert client.get(url("/logs/../config"), headers=AUTH).status_code == 404
