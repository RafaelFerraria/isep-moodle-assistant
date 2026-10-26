import os
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO))


@pytest.fixture
def home(tmp_path, monkeypatch):
    """Raiz de dados isolada (EmailNotifier/ e MoodleSync/ vazios) e pasta de estado da API própria."""
    (tmp_path / "EmailNotifier").mkdir()
    (tmp_path / "MoodleSync").mkdir()
    monkeypatch.setenv("ISEP_ASSISTANT_ROOT", str(tmp_path))
    monkeypatch.setenv("ISEP_ASSISTANT_STATE", str(tmp_path / "state"))
    for var in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "AI_API_KEY"):
        monkeypatch.delenv(var, raising=False)
    return tmp_path


# ---------------------------------------------------------------- emails de exemplo (formatos reais, dados inventados)

def mail(subject, sender, body, received="2026-10-26T18:00:00", message_id=None):
    return {"subject": subject, "sender": sender, "sender_email": sender, "received_time": received,
            "body": body, "message_id": message_id or f"<{abs(hash((subject, body)))}@test>"}


MOODLE = '"Não responda para este endereço  (via MoodleISEP)" <no-reply@moodle.isep.ipp.pt>'


def forum_mail(course, topic, author, text, forum="Anúncios"):
    head = (f"{course}<https://moodle.isep.ipp.pt/course/view.php?id=1> » Fóruns<https://moodle.isep.ipp.pt/mod/forum/index.php?id=1> » "
            f"{forum}<https://moodle.isep.ipp.pt/mod/forum/view.php?f=2> » {topic}<https://moodle.isep.ipp.pt/mod/forum/discuss.php?d=3>")
    body = (f"{head}\n[{author}]<https://moodle.isep.ipp.pt/user/view.php?id=9&course=1>\n{topic}\n"
            f"por {author}<https://moodle.isep.ipp.pt/user/view.php?id=9> - segunda, 26 outubro 2026, 18:18\n{text}\n"
            "Responder<https://moodle.isep.ipp.pt/mod/forum/post.php?reply=1>\n")
    return mail(f"{course.split('<')[0]}: {topic}", f'"{author} (via MoodleISEP)" <doc@isep.ipp.pt>', body)


def reminder_mail(title, due_text, course="DEI - Laboratório / Projeto III - 1º Semestre 2026/2027"):
    return mail(f"Data limite em {due_text.lower()}: {title}", MOODLE,
                f"\nOlá ALUNO,\n\nO trabalho {title.upper()} na disciplina {course} atingirá a data\nlimite brevemente.\n\n"
                f"DATA LIMITE: {due_text.upper()}\n\nIr para a atividade [1]\n\nLinks:\n------\n[1] https://moodle.isep.ipp.pt/mod/assign/view.php?id=1\n")


def overdue_mail(title="First Sprint Submission", ended="DOMINGO, 25 DE OUTUBRO DE 2026 ÀS 20:00",
                 late="DOMINGO, 25 DE OUTUBRO DE 2026 ÀS 23:59", course="DEI - Estruturas de Informação - 1º Semestre 2026/2027"):
    return mail(f"Atrasado: {title}", MOODLE,
                f"\nOlá ALUNO,\n\n{title.upper()} na disciplina '{course}' terminou o prazo {ended}.\n\n"
                f"Ainda poderá submeter o seu trabalho {late}, mas será marcado como 'Atrasado'.\n\nIr para a atividade [1]\n")
