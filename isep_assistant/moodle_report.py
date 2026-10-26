"""Análise dos ficheiros que o MoodleSync descarregou, para entrar no resumo diário."""
import os
import re
import json
import datetime
from collections import OrderedDict
from typing import List, Dict, Any

from .summarizer import short_course, first_sentences, norm

# (tipo, palavras-chave no nome do ficheiro/secção) por ordem de prioridade
KINDS = [
    ("enunciado", ("enunciado", "requisitos", "requirements", "trabalho prático", "projeto", "project", "sprint", "assignment")),
    ("avaliação", ("avalia", "evaluation", "teste", "exame", "frequência", "grading", "rubric", "critérios")),
    ("ficha", ("ficha", "exercício", "exercicio", "worksheet", "guião", "guiao")),
    ("slides", ("slides", "apresenta", "presentation", "teórica", "teorica", "lecture")),
]
DEADLINE_RE = re.compile(
    r"([^.\n]{0,60}\b(?:prazo|entrega|submiss[ãa]o|deadline|due|data limite)\b[^.\n]{0,40}"
    r"\d{1,2}\s*(?:[/-]\s*\d{1,2}|de\s+[a-zç]+|\s+[A-Z][a-z]+)[^.\n]{0,25})", re.IGNORECASE)


def classify_file(name: str, section: str) -> str:
    probe = f"{name} {section}".lower()
    for kind, words in KINDS:
        if any(w in probe for w in words):
            return kind
    return "material"


def read_text_preview(path: str, max_chars: int = 4000) -> str:
    """Texto do início do ficheiro (PDF via pypdf, se instalado; .txt/.md diretamente)."""
    ext = os.path.splitext(path)[1].lower()
    try:
        if ext in (".txt", ".md"):
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                return f.read(max_chars)
        if ext == ".pdf":
            from pypdf import PdfReader  # opcional: pip install pypdf
            reader = PdfReader(path)
            text = ""
            for page in reader.pages[:3]:
                text += (page.extract_text() or "") + "\n"
                if len(text) >= max_chars:
                    break
            return text[:max_chars]
    except Exception:
        pass
    return ""


def analyse(entry: Dict[str, Any], download_dir: str) -> Dict[str, Any]:
    path = os.path.join(download_dir, entry.get("path", ""))
    kind = classify_file(entry.get("name", ""), entry.get("section", ""))
    text = read_text_preview(path)
    hint = ""
    if text:
        m = DEADLINE_RE.search(norm(text))
        if m:
            hint = f"menciona prazo: “{first_sentences(m.group(1).strip(), 110)}”"
        elif kind != "material" or entry["name"].lower().endswith(".txt"):
            body = [l.strip() for l in text.splitlines() if len(l.strip()) > 25]
            if body:
                hint = f"começa por: “{first_sentences(body[0], 110)}”"
    return {"kind": kind, "hint": hint}


def load_new_files(jsonl_path: str, since: datetime.datetime) -> List[Dict[str, Any]]:
    """Entradas do new_files.jsonl com data posterior a 'since' (mais recentes por último, sem repetidos)."""
    if not os.path.exists(jsonl_path):
        return []
    seen: "OrderedDict[str, Dict[str, Any]]" = OrderedDict()
    with open(jsonl_path, "r", encoding="utf-8") as f:
        for line in f:
            try:
                e = json.loads(line)
                if datetime.datetime.fromisoformat(e["ts"]) > since:
                    seen[e["path"]] = e
            except Exception:
                continue
    return list(seen.values())


def build_moodle_section(entries: List[Dict[str, Any]], download_dir: str, max_per_course: int = 6) -> List[str]:
    """Secção '📥 NOVO NO MOODLE' agrupada por disciplina, com tipo de cada ficheiro e uma pista do conteúdo."""
    if not entries:
        return []
    by_course: "OrderedDict[str, List[Dict[str, Any]]]" = OrderedDict()
    for e in entries:
        by_course.setdefault(short_course(e.get("course", "")), []).append(e)

    lines = [f"📥 NOVO NO MOODLE ({len(entries)} ficheiro(s))"]
    for course, items in by_course.items():
        lines.append(f"• {course}")
        for e in items[:max_per_course]:
            a = analyse(e, download_dir)
            tag = "atualizado" if e.get("kind") == "atualizado" else "novo"
            lines.append(f"  – {e['name']} [{a['kind']}, {tag}]")
            if a["hint"]:
                lines.append(f"    {a['hint']}")
        if len(items) > max_per_course:
            lines.append(f"  … e mais {len(items) - max_per_course}")
    return lines
