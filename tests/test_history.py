from isep_assistant import history


def rec(kind="digest", ok=True, trigger="manual", summary=None, **kw):
    return history.append(history.new_record(kind, trigger, ok, "2026-10-26T18:00:00", summary=summary, **kw))


def test_append_and_list_newest_first_without_summary_by_default(home):
    a = rec(summary="texto A")
    b = rec(kind="moodle_sync", details={"new_files": 2})
    items = history.list_records()
    assert [i["id"] for i in items] == [b["id"], a["id"]]
    assert items[1]["summary"] is None and items[1]["has_summary"] is True
    assert items[0]["has_summary"] is False
    assert history.list_records(include_summary=True)[1]["summary"] == "texto A"


def test_filter_by_kind_limit_and_offset(home):
    for _ in range(3):
        rec()
    rec(kind="moodle_sync")
    assert len(history.list_records(kind="digest")) == 3
    assert len(history.list_records(limit=2)) == 2
    assert len(history.list_records(limit=10, offset=3)) == 1


def test_get_by_id_returns_full_record(home):
    r = rec(summary="completo", error=None)
    assert history.get(r["id"])["summary"] == "completo"
    assert history.get("naoexiste") is None


def test_last_and_last_ok(home):
    good = rec(ok=True)
    bad = rec(ok=False, error="falhou")
    assert history.last("digest")["id"] == bad["id"]
    assert history.last("digest", only_ok=True)["id"] == good["id"]
    assert history.last("moodle_sync") is None


def test_duration_is_computed(home):
    r = history.new_record("digest", "manual", True, "2026-10-26T18:00:00")
    assert r["duration_s"] is not None and r["duration_s"] >= 0


def test_truncated_line_does_not_break_reading(home):
    rec()
    with open(home / "EmailNotifier" / "history.jsonl", "a", encoding="utf-8") as f:
        f.write('{"id": "cortado", "kind": "dige')
    rec()
    assert len(history.list_records()) == 2


def test_file_is_pruned_when_it_grows_too_much(home):
    for i in range(history._PRUNE_AT + 5):
        history.append({"id": str(i), "kind": "digest", "ok": True})
    lines = (home / "EmailNotifier" / "history.jsonl").read_text(encoding="utf-8").splitlines()
    assert len(lines) <= history._PRUNE_AT
    assert history.list_records(limit=1)[0]["id"] == str(history._PRUNE_AT + 4)
