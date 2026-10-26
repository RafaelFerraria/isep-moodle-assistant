import json

from isep_assistant import system


def test_parse_tasks_json_accepts_list_and_single_object_and_hides_never():
    one = json.dumps({"name": "T", "state": "Ready", "last_run": "1999-11-30T00:00:00",
                      "next_run": "2026-10-27T18:00:00", "last_result": 0})
    parsed = system.parse_tasks_json(one)
    assert parsed == [{"name": "T", "state": "Ready", "last_run": None, "next_run": "2026-10-27T18:00:00", "last_result": 0}]
    many = system.parse_tasks_json(json.dumps([json.loads(one), json.loads(one)]))
    assert len(many) == 2
    assert system.parse_tasks_json("") == []


def test_query_tasks_is_empty_off_windows(monkeypatch):
    monkeypatch.setattr(system, "IS_WINDOWS", False)
    assert system.query_tasks() == []
    ok, message = system.register_tasks("18:00")
    assert not ok and "Windows" in message


def test_query_tasks_survives_powershell_errors(monkeypatch):
    monkeypatch.setattr(system, "IS_WINDOWS", True)

    def boom(*a, **k):
        raise OSError("sem powershell")
    monkeypatch.setattr(system.subprocess, "run", boom)
    assert system.query_tasks() == []
