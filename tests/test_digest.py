import datetime
import json

import pytest

from conftest import forum_mail, mail
from isep_assistant import config_store, digest, history

CFG = {"session_recipients": ["05" + "cd" * 32], "session_mnemonic": "m", "summary_mode": "rules"}


@pytest.fixture
def env(home, monkeypatch):
    """Digest sem rede nem Session: emails e envio são simulados."""
    state = {"emails": [], "sent": [], "send_ok": True, "fetch_error": None, "since": []}

    def fake_get_emails(config, since):
        state["since"].append(since)
        if state["fetch_error"]:
            raise state["fetch_error"]
        return list(state["emails"])

    def fake_send(text, config, log=print):
        state["sent"].append(text)
        return state["send_ok"]

    monkeypatch.setattr(digest, "get_emails", fake_get_emails)
    monkeypatch.setattr(digest, "send_to_session", fake_send)
    monkeypatch.setattr(digest, "ensure_moodle_synced", lambda *a, **k: None)
    config_store.save_config(dict(CFG))
    return state


def last_run():
    return json.loads((digest.paths.last_run_file()).read_text(encoding="utf-8"))


def test_sends_summary_and_records_state_and_history(env):
    env["emails"] = [forum_mail("LAPR3", "Aviso", "Docente", "Texto do aviso.")]
    res = digest.run_digest(trigger="scheduled")
    assert res.ok and res.sent and res.emails_new == 1 and "Aviso" in env["sent"][0]
    assert env["emails"][0]["message_id"] in last_run()["sent_ids"]
    rec = history.last("digest")
    assert rec["ok"] and rec["trigger"] == "scheduled" and rec["details"]["emails_new"] == 1
    assert "Aviso" in history.get(rec["id"])["summary"]


def test_already_sent_emails_are_not_sent_again(env):
    env["emails"] = [mail("Aviso", "x@isep.ipp.pt", "Texto.")]
    digest.run_digest()
    env["sent"].clear()
    res = digest.run_digest()
    assert res.skipped and res.ok and env["sent"] == []
    assert history.last("digest")["details"]["skipped"] is True


def test_failed_send_keeps_emails_for_the_next_run(env):
    env["emails"] = [mail("Aviso", "x@isep.ipp.pt", "Texto.")]
    env["send_ok"] = False
    res = digest.run_digest()
    assert not res.ok and res.error and not digest.paths.last_run_file().exists()
    env["send_ok"] = True
    assert digest.run_digest().sent


def test_fetch_error_is_reported_and_state_is_not_advanced(env):
    env["fetch_error"] = ConnectionError("Sem ligação à Internet.")
    res = digest.run_digest()
    assert not res.ok and "Sem ligação" in res.error and env["sent"] == []
    assert not digest.paths.last_run_file().exists()
    assert history.last("digest")["ok"] is False


def test_preview_never_sends_or_persists(env):
    env["emails"] = [mail("Aviso", "x@isep.ipp.pt", "Texto.")]
    res = digest.run_digest(send=False)
    assert res.ok and res.preview and not res.sent and "Aviso" in res.summary
    assert env["sent"] == [] and not digest.paths.last_run_file().exists() and history.last("digest") is None


def test_missing_config_gives_clear_error(home):
    res = digest.run_digest()
    assert not res.ok and "config.json" in res.error


def test_search_window_overlaps_previous_run(env):
    digest.update_last_run()
    digest.run_digest()
    since = env["since"][0]
    expected = datetime.datetime.now() - datetime.timedelta(hours=digest.OVERLAP_HOURS)
    assert abs((since - expected).total_seconds()) < 5


def test_moodle_files_alone_trigger_a_summary(env, home):
    (home / "MoodleSync" / "config.json").write_text("{}", encoding="utf-8")
    entry = {"ts": datetime.datetime.now().isoformat(timespec="seconds"), "kind": "novo", "course": "LAPR3",
             "section": "S", "name": "Enunciado.pdf", "path": "x/Enunciado.pdf", "size": 1}
    (home / "MoodleSync" / "new_files.jsonl").write_text(json.dumps(entry) + "\n", encoding="utf-8")
    digest.update_last_run(moodle_ts=(datetime.datetime.now() - datetime.timedelta(hours=1)).isoformat())
    res = digest.run_digest()
    assert res.sent and res.moodle_files == 1 and "Enunciado.pdf" in env["sent"][0]
    # a próxima execução já não repete os mesmos ficheiros
    env["sent"].clear()
    assert digest.run_digest().skipped


def test_ai_fallback_is_reported(env, monkeypatch):
    config_store.save_config(dict(CFG, summary_mode="ai", ai_provider="api", ai_api_type="openai"))
    env["emails"] = [mail("Aviso", "x@isep.ipp.pt", "Texto.")]
    res = digest.run_digest()
    assert res.sent and res.ai_fallback and not res.ai_used and "IA indisponível" in env["sent"][0]
    assert history.last("digest")["details"]["ai_fallback"] is True


def test_moodle_dir_prefers_config_then_sibling_folder(home):
    assert digest.moodle_dir_of({}) is None
    (home / "MoodleSync" / "config.json").write_text("{}", encoding="utf-8")
    assert digest.moodle_dir_of({}) == str(home / "MoodleSync")
    assert digest.moodle_dir_of({"moodle_sync_dir": "X:/outro"}) == "X:/outro"


def test_progress_callback_receives_log_lines(env):
    lines = []
    digest.run_digest(progress=lines.append)
    assert any("INÍCIO" in l for l in lines) and any("Nenhum email novo" in l for l in lines)
