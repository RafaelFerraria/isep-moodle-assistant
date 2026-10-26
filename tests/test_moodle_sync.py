import json

import pytest

from isep_assistant import history, moodle_sync as ms

PDF_URL = "https://moodle.isep.ipp.pt/pluginfile.php/1/mod_resource/content/1/Enunciado%20LP3.pdf"


class FakeResponse:
    def __init__(self, content=b"", status=200, headers=None, url=PDF_URL, text=""):
        self.content, self.status_code, self.headers, self.url, self.text = content, status, headers or {}, url, text

    def iter_content(self, chunk_size=1):
        yield self.content

    def close(self):
        pass


class FakeSession:
    def __init__(self, responses):
        self.responses = list(responses)
        self.headers = {}
        self.cookies = {}

    def get(self, url, **kw):
        return self.responses.pop(0)


@pytest.fixture
def engine(home, tmp_path):
    e = ms.MoodleISEPSync("1234567", "pw", str(tmp_path / "dl"))
    e._ctx = {"course": "LAPR3", "section": "Secção 01"}
    return e


def logged_entries(home):
    path = home / "MoodleSync" / "new_files.jsonl"
    return [json.loads(l) for l in path.read_text(encoding="utf-8").splitlines()] if path.exists() else []


def test_real_name_from_content_disposition_utf8():
    r = FakeResponse(headers={"Content-Disposition": "attachment; filename*=UTF-8''Enunciado%20LP3%20v1.3.pdf"})
    assert ms.MoodleISEPSync._real_name(r, "Enunciado LP3 V1.3Ficheiro") == "Enunciado LP3 v1.3.pdf"


def test_real_name_from_plain_content_disposition():
    r = FakeResponse(headers={"Content-Disposition": 'inline; filename="Guia.pdf"'})
    assert ms.MoodleISEPSync._real_name(r, "x") == "Guia.pdf"


def test_real_name_falls_back_to_url_then_link_text_plus_extension():
    assert ms.MoodleISEPSync._real_name(FakeResponse(), "x") == "Enunciado LP3.pdf"
    r2 = FakeResponse(url="https://moodle.isep.ipp.pt/mod/resource/view.php?id=1", headers={"Content-Type": "application/pdf"})
    assert ms.MoodleISEPSync._real_name(r2, "Texto do link") == "Texto do link.pdf"
    r3 = FakeResponse(url="https://moodle.isep.ipp.pt/x", headers={"Content-Type": "text/html; charset=utf-8"})
    assert ms.MoodleISEPSync._real_name(r3, "Pagina") == "Pagina"


def test_sanitize_filename():
    assert ms.sanitize_filename('a<b>:"c"/d\\e|f?g*') == "a_b___c__d_e_f_g_"
    assert ms.sanitize_filename("   ") == "sem_nome"
    assert len(ms.sanitize_filename("x" * 300)) == 120


def test_download_new_unchanged_and_updated(engine, home, tmp_path):
    dest = str(tmp_path / "dl" / "Lab" / "Enunciado LP3 V1.3Ficheiro")
    engine.session = FakeSession([FakeResponse(b"versao 1")])
    assert engine.download_file("u", dest)
    final = tmp_path / "dl" / "Lab" / "Enunciado LP3.pdf"
    assert final.read_bytes() == b"versao 1"
    assert [e["kind"] for e in logged_entries(home)] == ["novo"]

    engine.session = FakeSession([FakeResponse(b"versao 1")])  # igual, sem Content-Length: não é "novo"
    assert engine.download_file("u", dest)
    assert len(logged_entries(home)) == 1

    engine.session = FakeSession([FakeResponse(b"versao 1", headers={"Content-Length": "8"})])  # tamanho igual
    assert engine.download_file("u", dest)
    assert len(logged_entries(home)) == 1

    engine.session = FakeSession([FakeResponse(b"versao 2 maior")])
    assert engine.download_file("u", dest)
    assert final.read_bytes() == b"versao 2 maior"
    assert [e["kind"] for e in logged_entries(home)] == ["novo", "atualizado"]
    assert logged_entries(home)[0]["course"] == "LAPR3" and not list((tmp_path / "dl").rglob("*.tmp"))


def test_download_migrates_legacy_extensionless_file(engine, tmp_path):
    folder = tmp_path / "dl" / "Lab"
    folder.mkdir(parents=True)
    legacy = folder / "Enunciado LP3Ficheiro"
    legacy.write_bytes(b"antigo")
    engine.session = FakeSession([FakeResponse(b"novo conteudo")])
    assert engine.download_file("u", str(legacy))
    assert not legacy.exists() and (folder / "Enunciado LP3.pdf").exists()


def test_download_http_error_counts_but_does_not_raise(engine, tmp_path):
    engine.session = FakeSession([FakeResponse(status=404)])
    assert engine.download_file("u", str(tmp_path / "x")) is False and engine.errors == 1


def test_download_network_error_counts(engine, tmp_path):
    class Boom:
        def get(self, *a, **k):
            raise ConnectionError("DNS")
    engine.session = Boom()
    assert engine.download_file("u", str(tmp_path / "x")) is False and engine.errors == 1


def test_save_page_records_new_then_updated_then_silent(engine, home, tmp_path):
    def page(t):
        return FakeResponse(text=f'<html><div role="main"><h2>Enunciado</h2><p>{t}</p></div></html>')
    folder = str(tmp_path / "dl" / "Paginas")
    for version in ("v1", "v1", "v2"):
        engine.session = FakeSession([page(version)])
        engine.save_page("u", "Enunciado Sprint 2", folder)
    assert [e["kind"] for e in logged_entries(home)] == ["novo", "atualizado"]
    assert "v2" in (tmp_path / "dl" / "Paginas" / "Enunciado Sprint 2.txt").read_text(encoding="utf-8")


def test_run_all_skips_when_another_sync_holds_the_lock(engine, home):
    (home / "MoodleSync" / "sync.lock").write_text("123")
    assert engine.run_all() is False and engine.busy is True


def test_run_all_releases_lock_and_reports_errors(engine, home, monkeypatch):
    monkeypatch.setattr(engine, "login", lambda: True)
    monkeypatch.setattr(engine, "get_enrolled_courses", lambda: [{"id": "1", "name": "A"}, {"id": "2", "name": "B"}])
    visited = []

    def fake_sync(cid, name):
        visited.append(cid)
        if cid == "1":
            raise RuntimeError("falhou")
    monkeypatch.setattr(engine, "sync_course", fake_sync)
    assert engine.run_all() is False and visited == ["1", "2"] and engine.errors == 1
    assert not (home / "MoodleSync" / "sync.lock").exists()


def test_run_sync_requires_credentials(home):
    res = ms.run_sync()
    assert not res.ok and "Configuração em falta" in res.error and history.last("moodle_sync") is None


def test_run_sync_records_history_and_marks_the_day(home, monkeypatch):
    ms.save_config({"username": "1", "encrypted_password": "00", "download_dir": str(home / "dl")})
    monkeypatch.setattr(ms, "decrypt_dpapi", lambda x: "pw")

    class FakeEngine:
        busy = False

        def __init__(self, *a, **k):
            self.new_files = [{"course": "LAPR3", "name": "a.pdf", "kind": "novo"}]
            self.errors = 0

        def run_all(self):
            ms.log("a sincronizar...")
            return True
    monkeypatch.setattr(ms, "MoodleISEPSync", FakeEngine)
    seen = []
    res = ms.run_sync(trigger="api", progress=seen.append)
    assert res.ok and res.new_files == 1 and "a sincronizar..." in seen
    assert ms.load_config()["last_sync_date"] == ms.date.today().isoformat()
    rec = history.last("moodle_sync")
    assert rec["trigger"] == "api" and rec["details"]["new_files"] == 1 and rec["details"]["files"][0]["name"] == "a.pdf"
    assert ms._log_sink is None  # o sink de progresso é sempre removido


def _fake_engine(busy, errors, ok):
    class Fake:
        def __init__(self, *a, **k):
            self.busy, self.new_files, self.errors = busy, [], errors

        def run_all(self):
            return ok
    return Fake


def test_run_sync_busy_is_not_recorded(home, monkeypatch):
    ms.save_config({"username": "1", "encrypted_password": "00"})
    monkeypatch.setattr(ms, "decrypt_dpapi", lambda x: "pw")
    monkeypatch.setattr(ms, "MoodleISEPSync", _fake_engine(busy=True, errors=0, ok=False))
    res = ms.run_sync()
    assert res.busy and not res.ok and history.last("moodle_sync") is None


def test_run_sync_failure_has_error_message_and_does_not_mark_the_day(home, monkeypatch):
    ms.save_config({"username": "1", "encrypted_password": "00"})
    monkeypatch.setattr(ms, "decrypt_dpapi", lambda x: "pw")
    monkeypatch.setattr(ms, "MoodleISEPSync", _fake_engine(busy=False, errors=3, ok=False))
    res = ms.run_sync()
    assert not res.ok and "3 erro(s)" in res.error and history.last("moodle_sync")["ok"] is False
    assert ms.load_config().get("last_sync_date") in (None, "")


def test_wait_for_internet_returns_quickly_when_dns_works(monkeypatch):
    monkeypatch.setattr(ms.socket, "gethostbyname", lambda h: "1.2.3.4")
    assert ms.wait_for_internet(5) is True


def test_wait_for_internet_gives_up(monkeypatch):
    def fail(h):
        raise OSError
    monkeypatch.setattr(ms.socket, "gethostbyname", fail)
    monkeypatch.setattr(ms.time, "sleep", lambda s: None)
    ticks = iter(range(0, 1000, 100))
    monkeypatch.setattr(ms.time, "time", lambda: next(ticks))
    assert ms.wait_for_internet(250) is False
