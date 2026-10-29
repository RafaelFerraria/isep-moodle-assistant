import datetime

import pytest

from conftest import MOODLE, forum_mail, mail, overdue_mail, reminder_mail
from isep_assistant import summarizer as s

NOW = datetime.datetime(2026, 10, 26, 19, 40)


def test_classify_recognises_each_kind():
    assert s.classify(forum_mail("LAPR3(85519)", "Requisitos", "Docente X", "texto")) == "forum"
    assert s.classify(reminder_mail("Sprint 1", "segunda-feira, 26 de outubro de 2026 às 23:59")) == "prazo"
    assert s.classify(overdue_mail()) == "prazo"
    assert s.classify(mail("Novo início de sessão com a sua conta 'Moodle'", MOODLE, "x")) == "seguranca"
    assert s.classify(mail("Faltas - Semana de 18/10/2026 a 24/10/2026", "seraa@isep.ipp.pt", "x")) == "faltas"
    assert s.classify(mail("[Helpdesk #1] Cartão", "hd@isep.ipp.pt", "x")) == "helpdesk"
    assert s.classify(mail("Ementa semanal", "secretariado@isep.ipp.pt", "x")) == "geral"


def test_parse_pt_datetime_and_relative_labels():
    dt = s.parse_pt_datetime("SEGUNDA-FEIRA, 26 DE OUTUBRO DE 2026 ÀS 23:59")
    assert dt == datetime.datetime(2026, 10, 26, 23, 59)
    assert s.relative_label(dt, NOW) == "HOJE 23:59"
    assert s.relative_label(dt + datetime.timedelta(days=1), NOW) == "AMANHÃ 23:59"
    assert "em 3 dias" in s.relative_label(dt + datetime.timedelta(days=3), NOW)
    assert s.relative_label(dt - datetime.timedelta(days=2), NOW).startswith("terminou")
    assert s.parse_pt_datetime("sem data") is None
    assert s.parse_pt_datetime("31 de smarch de 2026 às 10:00") is None


def test_first_sentences_never_cuts_a_word():
    text = "Primeira frase completa e bastante longa para passar do limiar. " + "palavra " * 60
    out = s.first_sentences(text, 90)
    assert out == "Primeira frase completa e bastante longa para passar do limiar."
    out2 = s.first_sentences("palavra " * 60, 50)
    assert out2.endswith("…") and not out2[:-1].endswith("palav")


def test_body_excerpt_drops_urls_refs_css_and_greetings():
    body = ("Olá Aluno,\n\n.cls { font-family: Arial; }\n/* css */\nO teste é dia 5 [1] veja https://x.pt/a\n"
            "Cumprimentos,\nDocente")
    assert s.body_excerpt(body) == "O teste é dia 5 veja"


def test_prazos_groups_same_due_date_and_sorts_overdue_first():
    emails = [
        reminder_mail("2DO-2DP-C - SPRINT 1 - Status Report", "segunda-feira, 26 de outubro de 2026 às 23:59"),
        reminder_mail("2NA-2NB - SPRINT 1 - Status Report", "segunda-feira, 26 de outubro de 2026 às 23:59"),
        reminder_mail("Outro", "quinta-feira, 29 de outubro de 2026 às 12:00"),
        overdue_mail(),
    ]
    text = s.build_summary(emails, now=NOW)
    prazos = text.split("⏰ PRAZOS")[1].split("━")[0]
    assert prazos.index("🔴") < prazos.index("Laboratório / Projeto III")
    assert "2 entregas: 2DO-2DP-C - SPRINT 1 - Status Report; 2NA-2NB - SPRINT 1 - Status Report" in prazos
    assert "HOJE 23:59" in prazos and "em 3 dias" in prazos
    assert "Estruturas de Informação: First Sprint Submission" in prazos
    assert "O prazo terminou dom 25/10 20:00." in prazos


def test_overdue_mentions_late_window_only_while_open():
    open_window = s.build_summary([overdue_mail(late="SEGUNDA-FEIRA, 26 DE OUTUBRO DE 2026 ÀS 23:59")], now=NOW)
    assert "Ainda podes entregar até seg 26/10 23:59" in open_window
    closed = s.build_summary([overdue_mail()], now=NOW)
    assert "Ainda podes entregar" not in closed


def test_past_reminders_are_dropped():
    out = s.build_summary([reminder_mail("Velho", "domingo, 25 de outubro de 2026 às 20:00")], now=NOW)
    assert "⏰ PRAZOS" not in out


def test_forum_announcements_are_deduplicated_across_courses():
    a = forum_mail("LAPR3(85519)", "Requisitos para o Sprint 2", "Docente Teste", "Grupos pequenos simplificam a US10.")
    b = forum_mail("SEM_3_PI 2026/2027", "Requisitos para o Sprint 2", "Docente Teste", "Grupos pequenos simplificam a US10.")
    out = s.build_summary([a, b], now=NOW)
    assert out.count("Requisitos para o Sprint 2") == 1
    assert "LAPR3 / SEM_3_PI — Requisitos para o Sprint 2" in out
    assert "Docente Teste: “Grupos pequenos simplificam a US10.”" in out


def test_student_forum_posts_are_labelled_as_colleagues():
    out = s.build_summary([forum_mail("ARQCP", "Dúvida", "SILVA 1234567", "Como funciona?")], now=NOW)
    assert "SILVA 1234567 (colega)" in out


def test_faltas_counts_per_course():
    body = ("Caro/a estudante, Segue-se o resumo das aulas: - T - AMATA (Análise Matemática) no dia 19/10/2026, das 18:10 às 19:00. "
            "- TP - AMATA (Análise Matemática) no dia 19/10/2026, das 20:40 às 22:00. "
            "- PL - LAPR3 (Laboratório / Projeto III) no dia 19/10/2026, das 20:40 às 22:00.")
    out = s.build_summary([mail("Faltas - Semana de 18/10/2026 a 24/10/2026", "seraa@isep.ipp.pt", body)], now=NOW)
    assert "Semana 18/10–24/10: 3 aula(s) com falta — AMATA ×2, LAPR3 ×1" in out


def test_helpdesk_collapses_one_ticket_into_one_item():
    mails = [
        mail("[HelpDesk #77] Auto-resposta/AutoReply: Cartão", "hd@isep.ipp.pt", "Recebemos o seu pedido!"),
        mail("[Helpdesk #77] Cartão definitivo", "pessoa@isep.ipp.pt", "O seu cartão já se encontra ativo. Use-o nas aulas."),
        mail("[Helpdesk #77] Respondido e Fechado/Answered and Closed: Cartão", "hd@isep.ipp.pt", "Pedido fechado."),
    ]
    out = s.build_summary(mails, now=NOW)
    assert out.count("#77") == 1
    assert "(pedido fechado)" in out and "O seu cartão já se encontra ativo." in out


def test_security_notices_are_aggregated_and_geral_is_listed():
    mails = [mail("Novo início de sessão com a sua conta 'Moodle'", MOODLE, "x")] * 2 + \
            [mail("Ementa Semanal - Bar", "secretariado@isep.ipp.pt", "Segue a ementa da semana.")]
    out = s.build_summary(mails, now=NOW)
    assert "2 aviso(s) de novo início de sessão" in out
    assert "• Ementa Semanal - Bar — Segue a ementa da semana." in out


def test_moodle_lines_are_inserted_between_forum_and_faltas():
    out = s.build_summary([forum_mail("LAPR3", "T", "D", "x")], ["📥 NOVO NO MOODLE (1 ficheiro(s))"], now=NOW)
    assert out.index("📢") < out.index("📥")


def test_summarize_emails_rules_by_default_and_empty_case():
    assert "sem emails novos" in s.summarize_emails([], {})
    assert s.summarize_emails([mail("Aviso", "x@isep.ipp.pt", "Texto do aviso.")], {}).startswith("📬 RESUMO ISEP")
    # só novidades do Moodle também justificam um resumo
    assert "NOVO NO MOODLE" in s.summarize_emails([], {}, ["📥 NOVO NO MOODLE (1 ficheiro(s))"])


def test_ai_mode_uses_ai_text_and_falls_back_with_notice(monkeypatch):
    from isep_assistant import ai_summarizer
    emails = [mail("Aviso", "x@isep.ipp.pt", "Texto do aviso.")]
    cfg = {"summary_mode": "ai"}
    monkeypatch.setattr(ai_summarizer, "summarize_with_ai", lambda *a, **k: "TEXTO DA IA " * 5)
    assert s.summarize_emails(emails, cfg) == "TEXTO DA IA " * 5

    def boom(*a, **k):
        raise RuntimeError("sem rede")
    monkeypatch.setattr(ai_summarizer, "summarize_with_ai", boom)
    out = s.summarize_emails(emails, cfg)
    assert out.startswith("📬 RESUMO ISEP") and "IA indisponível (sem rede)" in out
