import datetime
import json

from isep_assistant import moodle_report as mr


def entry(name, ts="2026-10-26T19:00:00", kind="novo", course="DEI - Laboratório / Projeto III - 1º Semestre 2026/2027",
          section="Secção 01", path=None):
    return {"ts": ts, "kind": kind, "course": course, "section": section, "name": name, "path": path or name, "size": 10}


def write_log(home, entries):
    path = home / "MoodleSync" / "new_files.jsonl"
    path.write_text("\n".join(json.dumps(e) for e in entries) + "\n", encoding="utf-8")
    return path


def test_classify_file_by_name_and_section():
    assert mr.classify_file("Enunciado Sprint 2.pdf", "") == "enunciado"
    assert mr.classify_file("LAPR3 Evaluation Guide", "") == "avaliação"
    assert mr.classify_file("Ficha de trabalho 3.pdf", "") == "ficha"
    assert mr.classify_file("T3.pdf", "Slides teóricos") == "slides"
    assert mr.classify_file("coisa.zip", "Outros") == "material"


def test_load_new_files_filters_by_time_and_keeps_latest_per_path(home):
    path = write_log(home, [
        entry("a.pdf", ts="2026-10-20T10:00:00"),
        entry("b.pdf", ts="2026-10-26T10:00:00"),
        entry("b.pdf", ts="2026-10-26T12:00:00", kind="atualizado"),
        "linha inválida",
    ][:3])
    with open(path, "a", encoding="utf-8") as f:
        f.write("isto não é json\n")
    found = mr.load_new_files(str(path), datetime.datetime(2026, 10, 25))
    assert [e["name"] for e in found] == ["b.pdf"] and found[0]["kind"] == "atualizado"
    assert mr.load_new_files(str(home / "naoexiste.jsonl"), datetime.datetime.min) == []


def test_section_groups_by_course_and_reads_text_hint(home):
    downloads = home / "dl"
    (downloads / "Lab").mkdir(parents=True)
    (downloads / "Lab" / "Enunciado Sprint 2.txt").write_text(
        "Enunciado do Sprint 2\nA entrega do relatório tem prazo até 8 de novembro de 2026 no Moodle.\n", encoding="utf-8")
    entries = [entry("Enunciado Sprint 2.txt", path="Lab/Enunciado Sprint 2.txt"),
               entry("Slides T3.pdf", course="DEI - Estruturas de Informação - 1º Semestre 2026/2027", kind="atualizado")]
    lines = mr.build_moodle_section(entries, str(downloads))
    text = "\n".join(lines)
    assert lines[0] == "📥 NOVO NO MOODLE (2 ficheiro(s))"
    assert "• Laboratório / Projeto III" in text and "• Estruturas de Informação" in text
    assert "Enunciado Sprint 2.txt [enunciado, novo]" in text
    assert "menciona prazo" in text and "8 de novembro" in text
    assert "Slides T3.pdf [slides, atualizado]" in text


def test_section_caps_files_per_course(home):
    entries = [entry(f"f{i}.pdf") for i in range(9)]
    text = "\n".join(mr.build_moodle_section(entries, str(home), max_per_course=4))
    assert "f3.pdf" in text and "f4.pdf" not in text and "e mais 5" in text


def test_empty_entries_give_no_section(home):
    assert mr.build_moodle_section([], str(home)) == []


def test_unreadable_files_only_lose_the_hint(home):
    lines = mr.build_moodle_section([entry("Enunciado.pdf", path="nao/existe.pdf")], str(home))
    assert any("Enunciado.pdf [enunciado, novo]" in l for l in lines) and len(lines) == 3
