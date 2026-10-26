import re
import sys
import datetime
from collections import OrderedDict
from typing import List, Dict, Any, Optional

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

LINE = "━" * 24

MESES = {
    "janeiro": 1, "fevereiro": 2, "março": 3, "marco": 3, "abril": 4, "maio": 5, "junho": 6,
    "julho": 7, "agosto": 8, "setembro": 9, "outubro": 10, "novembro": 11, "dezembro": 12,
}
DIAS_SEMANA = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"]


# ---------------------------------------------------------------- utilitários de texto

def norm(text: str) -> str:
    """Junta quebras de linha e espaços repetidos numa só linha."""
    return " ".join((text or "").split())


def parse_pt_datetime(text: str) -> Optional[datetime.datetime]:
    """Lê '26 de outubro de 2026 às 23:59' (qualquer capitalização) de dentro de um texto."""
    m = re.search(r"(\d{1,2})\s+de\s+([a-zçã]+)\s+de\s+(\d{4})\s+[àa]s\s+(\d{1,2}):(\d{2})", text, re.IGNORECASE)
    if not m:
        return None
    mes = MESES.get(m.group(2).lower())
    if not mes:
        return None
    try:
        return datetime.datetime(int(m.group(3)), mes, int(m.group(1)), int(m.group(4)), int(m.group(5)))
    except ValueError:
        return None


def fmt_dt(dt: datetime.datetime) -> str:
    return f"{DIAS_SEMANA[dt.weekday()]} {dt:%d/%m} {dt:%H:%M}"


def relative_label(dt: datetime.datetime, now: datetime.datetime) -> str:
    """'HOJE 23:59', 'AMANHÃ 23:59', 'ter 27/10 23:59 (em 2 dias)'."""
    days = (dt.date() - now.date()).days
    if dt < now:
        return f"terminou {fmt_dt(dt)}"
    if days == 0:
        return f"HOJE {dt:%H:%M}"
    if days == 1:
        return f"AMANHÃ {dt:%H:%M}"
    return f"{fmt_dt(dt)} (em {days} dias)"


def short_course(name: str) -> str:
    """'DEI - Laboratório / Projeto III - 1º Semestre 2026/2027' -> 'Laboratório / Projeto III'."""
    name = norm(name).strip("'\"")
    name = re.sub(r"^DEI\s*-\s*", "", name)
    name = re.sub(r"\s*-\s*\d+º\s*Semestre.*$", "", name, flags=re.IGNORECASE)
    name = re.sub(r"\s*\(\d+\)", "", name)          # 'LAPR3(85519)' -> 'LAPR3'
    name = re.sub(r"\s*20\d\d\s*/\s*20\d\d", "", name)
    return name.strip(" -:") or "Disciplina"


def clean_sender(sender: str) -> str:
    sender = re.sub(r"<[^>]*>", "", sender or "")
    sender = re.sub(r"\(via [^)]*\)", "", sender)
    sender = sender.replace('"', "").replace("\\", "")
    return norm(sender) or "Desconhecido"


def clean_subject(subj: str) -> str:
    subj = re.sub(r"^((re|fwd|fw|enc)\s*:\s*)+", "", subj or "", flags=re.IGNORECASE)
    return norm(subj)


def first_sentences(text: str, max_chars: int = 240) -> str:
    """Corta em fim de frase (nunca a meio de uma palavra) até max_chars."""
    text = norm(text)
    if len(text) <= max_chars:
        return text
    cut = text[:max_chars]
    end = max(cut.rfind(". "), cut.rfind("! "), cut.rfind("? "))
    if end > max_chars * 0.4:
        return cut[: end + 1]
    return cut.rsplit(" ", 1)[0].rstrip(",;:") + "…"


BOILERPLATE_STOP = re.compile(
    r"^(responder|ver o t[óo]pico|ver a mensagem|cumprimentos|atentamente|-{4,}|_{4,}|links:|"
    r"para (cancelar|alterar)|you can reply|this message)", re.IGNORECASE)
GREETING = re.compile(r"^(ol[áa]|caro|cara|bom dia|boa tarde|boa noite|ex\.mos)\b.{0,30}[,.]?$", re.IGNORECASE)


def body_excerpt(body: str, skip_header_lines: int = 0, max_chars: int = 240) -> str:
    """Primeiro texto útil do corpo, sem URLs, referências [1], saudações nem rodapés."""
    useful = []
    for raw in (body or "").splitlines()[skip_header_lines:]:
        line = raw.strip(" >\t")
        if not line:
            continue
        if BOILERPLATE_STOP.match(line):
            break
        line = re.sub(r"<?https?://\S+>?", "", line)
        line = re.sub(r"\[\d+\]", "", line).strip()
        if len(line) < 3 or GREETING.match(line) or re.search(r"[{};]|/\*|\*/|font-family|border-spacing|@media", line):
            continue
        useful.append(line)
    return first_sentences(" ".join(useful), max_chars)


# ---------------------------------------------------------------- classificação

def classify(e: Dict[str, Any]) -> str:
    subj = e.get("subject", "")
    sender = e.get("sender", "").lower()
    body = e.get("body", "")
    low_subj = subj.lower()
    if "moodle" in sender.replace("\\", ""):
        if "» fóruns" in body.lower().split("\n", 1)[0]:
            return "forum"
        if low_subj.startswith(("data limite", "atrasado")) or "data limite:" in body.lower():
            return "prazo"
        if low_subj.startswith(("novo início de sessão", "novo inicio de sessao")):
            return "seguranca"
        return "moodle"
    if re.match(r"^faltas\s*-\s*semana", low_subj):
        return "faltas"
    if re.search(r"\[help\s?desk\s*#\d+\]", low_subj):
        return "helpdesk"
    return "geral"


# ---------------------------------------------------------------- extração por tipo

def parse_forum(e: Dict[str, Any]) -> Dict[str, Any]:
    body = e.get("body", "")
    head = body.split("\n", 1)[0]
    course = short_course(head.split("<", 1)[0]) if "<" in head else "Moodle"
    parts = re.findall(r"»\s*([^»<]+?)\s*<", head)
    topic = parts[-1].strip() if parts else clean_subject(e.get("subject", ""))
    # o texto da mensagem vem depois do cabeçalho (breadcrumb, autor, título, 'por X - data')
    start = 0
    for i, line in enumerate(body.splitlines()[:12]):
        if re.match(r"^\s*por\s+.+", line, re.IGNORECASE):
            start = i + 1
            break
    return {
        "course": course, "topic": topic,
        "author": clean_sender(e.get("sender", "")),
        "text": body_excerpt(body, start, 260),
    }


def parse_prazo(e: Dict[str, Any]) -> Dict[str, Any]:
    subj = clean_subject(e.get("subject", ""))
    body = norm(e.get("body", ""))
    overdue = subj.lower().startswith("atrasado")
    course_m = re.search(r"na disciplina\s+'?(.+?)'?\s+(?:atingirá|terminou|vence|está)", body, re.IGNORECASE)
    course = short_course(course_m.group(1)) if course_m else "Moodle"

    if overdue:
        title = re.sub(r"^atrasado\s*:\s*", "", subj, flags=re.IGNORECASE)
        ended = parse_pt_datetime(body.split("terminou o prazo", 1)[1]) if "terminou o prazo" in body else None
        late = re.search(r"Ainda poderá submeter.*?(\d{1,2}\s+de\s+\w+\s+de\s+\d{4}\s+[àa]s\s+\d{1,2}:\d{2})", body, re.IGNORECASE)
        return {"overdue": True, "course": course, "title": title, "due": ended,
                "late_until": parse_pt_datetime(late.group(1)) if late else None}

    title_m = re.match(r"^data limite em .*?\d{1,2}:\d{2}\s*:\s*(.+)$", subj, re.IGNORECASE)
    title = title_m.group(1).strip() if title_m else subj
    due = parse_pt_datetime(body.split("DATA LIMITE:", 1)[1]) if "DATA LIMITE:" in body else parse_pt_datetime(subj)
    return {"overdue": False, "course": course, "title": title, "due": due, "late_until": None}


def parse_faltas(e: Dict[str, Any]) -> Dict[str, Any]:
    body = norm(e.get("body", ""))
    per_course: "OrderedDict[str, int]" = OrderedDict()
    for m in re.finditer(r"\b([A-Z]{1,3})\s*-\s*(\w+)\s*\([^)]*\)\s*no dia\s*\d{2}/\d{2}/\d{4}", body):
        per_course[m.group(2)] = per_course.get(m.group(2), 0) + 1
    week = re.search(r"semana de\s*(\d{2}/\d{2})/\d{4}\s*a\s*(\d{2}/\d{2})/\d{4}", e.get("subject", ""), re.IGNORECASE)
    return {"week": f"{week.group(1)}–{week.group(2)}" if week else "", "per_course": per_course}


# ---------------------------------------------------------------- construção das secções

def section_prazos(items: List[Dict[str, Any]], now: datetime.datetime) -> List[str]:
    if not items:
        return []
    # junta entregas do mesmo curso e prazo (ex.: várias turmas com o mesmo sprint)
    grouped: "OrderedDict[tuple, List[Dict[str, Any]]]" = OrderedDict()
    for it in items:
        grouped.setdefault((it["course"], it["overdue"], it["due"]), []).append(it)

    def sort_key(key):
        _, overdue, due = key
        return (0 if overdue else 1, due or datetime.datetime.max)

    lines = ["⏰ PRAZOS"]
    for (course, overdue, due), group in sorted(grouped.items(), key=lambda kv: sort_key(kv[0])):
        titles = [g["title"] for g in group]
        if len(titles) > 3:
            what = f"{len(titles)} entregas ({'; '.join(titles[:2])}; +{len(titles) - 2})"
        elif len(titles) > 1:
            what = f"{len(titles)} entregas: " + "; ".join(titles)
        else:
            what = titles[0]
        if overdue:
            late = group[0].get("late_until")
            tail = f" Ainda podes entregar até {fmt_dt(late)} (fica como atrasado)." if late and late > now else ""
            lines.append(f"🔴 {course}: {what}\n   O prazo terminou {fmt_dt(due) if due else ''}.{tail}")
        else:
            when = relative_label(due, now) if due else "data não indicada"
            urgent = due is not None and 0 <= (due - now).total_seconds() < 48 * 3600
            lines.append(f"{'🟠' if urgent else '🟡'} {course}: {what}\n   Entrega: {when}")
    return lines


def section_avisos(items: List[Dict[str, Any]]) -> List[str]:
    if not items:
        return []
    # o mesmo aviso chega várias vezes (fórum geral + fórum da turma): junta pelo título
    by_topic: "OrderedDict[str, Dict[str, Any]]" = OrderedDict()
    for it in items:
        key = it["topic"].lower()
        if key in by_topic:
            if it["course"] not in by_topic[key]["courses"]:
                by_topic[key]["courses"].append(it["course"])
            if len(it["text"]) > len(by_topic[key]["text"]):
                by_topic[key]["text"] = it["text"]
        else:
            by_topic[key] = dict(it, courses=[it["course"]])

    lines = ["📢 FÓRUNS & AVISOS"]
    for it in by_topic.values():
        lines.append(f"• {' / '.join(it['courses'])} — {it['topic']}")
        who = it["author"] + (" (colega)" if re.search(r"\d{7}", it["author"]) else "")
        lines.append(f"  {who}: “{it['text']}”" if it["text"] else f"  {who} (sem texto; vê no fórum)")
    return lines


def section_faltas(items: List[Dict[str, Any]]) -> List[str]:
    if not items:
        return []
    lines = ["🗓 FALTAS"]
    for f in items:
        total = sum(f["per_course"].values())
        detail = ", ".join(f"{c} ×{n}" for c, n in f["per_course"].items())
        lines.append(f"• Semana {f['week']}: {total} aula(s) com falta — {detail}" if total
                     else f"• Semana {f['week']}: ver o detalhe no email")
    return lines


def section_helpdesk(emails: List[Dict[str, Any]]) -> List[str]:
    if not emails:
        return []
    tickets: "OrderedDict[str, List[Dict[str, Any]]]" = OrderedDict()
    for e in emails:
        m = re.search(r"#(\d+)", e.get("subject", ""))
        tickets.setdefault(m.group(1) if m else "?", []).append(e)
    lines = ["🛠 HELPDESK"]
    for tid, group in tickets.items():
        # a resposta humana (não a auto-resposta) é a que interessa
        human = [g for g in group if not re.search(r"auto-?resposta|autoreply|respondido e fechado", g.get("subject", ""), re.IGNORECASE)]
        pick = human[0] if human else group[0]
        subj = re.sub(r"^\[[^\]]*\]\s*", "", clean_subject(pick.get("subject", "")))
        closed = any(re.search(r"fechado|closed", g.get("subject", ""), re.IGNORECASE) for g in group)
        lines.append(f"• #{tid} {subj}{' (pedido fechado)' if closed else ''}")
        txt = body_excerpt(pick.get("body", ""), 0, 200) if human else ""
        if txt:
            lines.append(f"  {txt}")
    return lines


def section_geral(emails: List[Dict[str, Any]]) -> List[str]:
    if not emails:
        return []
    lines = ["ℹ️ OUTROS"]
    for e in emails:
        subj = re.sub(r"^\[[^\]]*\]\s*", "", clean_subject(e.get("subject", "Sem assunto")))
        txt = body_excerpt(e.get("body", ""), 0, 140)
        lines.append(f"• {subj}" + (f" — {txt}" if txt and txt.lower() not in subj.lower() else ""))
    return lines


def build_summary(emails: List[Dict[str, Any]], moodle_lines: Optional[List[str]] = None,
                  now: Optional[datetime.datetime] = None) -> str:
    now = now or datetime.datetime.now()
    buckets: Dict[str, List[Dict[str, Any]]] = {
        k: [] for k in ("forum", "prazo", "seguranca", "moodle", "faltas", "helpdesk", "geral")}
    for e in emails:
        buckets[classify(e)].append(e)

    prazos = [p for p in (parse_prazo(e) for e in buckets["prazo"]) if p["overdue"] or not p["due"] or p["due"] >= now]
    sections = [
        section_prazos(prazos, now),
        section_avisos([parse_forum(e) for e in buckets["forum"]]),
        moodle_lines or [],
        section_faltas([parse_faltas(e) for e in buckets["faltas"]]),
        section_helpdesk(buckets["helpdesk"]),
        section_geral(buckets["geral"] + buckets["moodle"]),
    ]
    out = [f"📬 RESUMO ISEP — {DIAS_SEMANA[now.weekday()]} {now:%d/%m} {now:%H:%M}", f"{len(emails)} emails novos"]
    for sec in sections:
        if sec:
            out += ["", LINE, *sec]
    if buckets["seguranca"]:
        out += ["", LINE, f"🔐 Moodle: {len(buckets['seguranca'])} aviso(s) de novo início de sessão (se foste tu, ignora)."]
    return "\n".join(out)


def summarize_emails(emails: List[Dict[str, Any]], config: Dict[str, Any],
                     moodle_lines: Optional[List[str]] = None) -> str:
    """Resumo agrupado por assunto: prazos, avisos, novidades do Moodle, faltas, helpdesk e outros."""
    if not emails and not moodle_lines:
        return "📬 ISEP: sem emails novos nem ficheiros novos no Moodle desde a última verificação."
    return build_summary(emails, moodle_lines)
