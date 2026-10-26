mod common;
use chrono::NaiveDate;
use common::*;
use isep_assistant::model::{Email, NewFile};
use isep_assistant::moodle_report as mr;
use isep_assistant::summarizer as s;
use serde_json::json;

const MOODLE: &str = "\"Não responda para este endereço  (via MoodleISEP)\" <no-reply@moodle.isep.ipp.pt>";

fn now() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(19, 40, 0).unwrap()
}

fn mail(subject: &str, sender: &str, body: &str) -> Email {
    Email::new(subject, sender, body)
}

fn forum_mail(course: &str, topic: &str, author: &str, text: &str) -> Email {
    let head = format!(
        "{course}<https://moodle.isep.ipp.pt/course/view.php?id=1> » Fóruns<https://moodle.isep.ipp.pt/mod/forum/index.php?id=1> » \
         Anúncios<https://moodle.isep.ipp.pt/mod/forum/view.php?f=2> » {topic}<https://moodle.isep.ipp.pt/mod/forum/discuss.php?d=3>"
    );
    let body = format!(
        "{head}\n[{author}]<https://moodle.isep.ipp.pt/user/view.php?id=9&course=1>\n{topic}\n\
         por {author}<https://moodle.isep.ipp.pt/user/view.php?id=9> - segunda, 26 outubro 2026, 18:18\n{text}\n\
         Responder<https://moodle.isep.ipp.pt/mod/forum/post.php?reply=1>\n"
    );
    mail(&format!("{}: {topic}", course.split('<').next().unwrap()), &format!("\"{author} (via MoodleISEP)\" <doc@isep.ipp.pt>"), &body)
}

fn reminder_mail(title: &str, due_text: &str) -> Email {
    let course = "DEI - Laboratório / Projeto III - 1º Semestre 2026/2027";
    mail(
        &format!("Data limite em {}: {title}", due_text.to_lowercase()),
        MOODLE,
        &format!(
            "\nOlá ALUNO,\n\nO trabalho {} na disciplina {course} atingirá a data\nlimite brevemente.\n\nDATA LIMITE: {}\n\nIr para a atividade [1]\n\nLinks:\n------\n[1] https://moodle.isep.ipp.pt/mod/assign/view.php?id=1\n",
            title.to_uppercase(),
            due_text.to_uppercase()
        ),
    )
}

fn overdue_mail(ended: &str, late: &str) -> Email {
    let title = "First Sprint Submission";
    mail(
        &format!("Atrasado: {title}"),
        MOODLE,
        &format!(
            "\nOlá ALUNO,\n\n{} na disciplina 'DEI - Estruturas de Informação - 1º Semestre 2026/2027' terminou o prazo {ended}.\n\n\
             Ainda poderá submeter o seu trabalho {late}, mas será marcado como 'Atrasado'.\n\nIr para a atividade [1]\n",
            title.to_uppercase()
        ),
    )
}

const ENDED: &str = "DOMINGO, 25 DE OUTUBRO DE 2026 ÀS 20:00";
const LATE_PASSED: &str = "DOMINGO, 25 DE OUTUBRO DE 2026 ÀS 23:59";

#[test]
fn classify_recognises_each_kind() {
    use s::Kind::*;
    assert_eq!(s::classify(&forum_mail("LAPR3(85519)", "Requisitos", "Docente X", "texto")), Forum);
    assert_eq!(s::classify(&reminder_mail("Sprint 1", "segunda-feira, 26 de outubro de 2026 às 23:59")), Prazo);
    assert_eq!(s::classify(&overdue_mail(ENDED, LATE_PASSED)), Prazo);
    assert_eq!(s::classify(&mail("Novo início de sessão com a sua conta 'Moodle'", MOODLE, "x")), Seguranca);
    assert_eq!(s::classify(&mail("Faltas - Semana de 18/10/2026 a 24/10/2026", "seraa@isep.ipp.pt", "x")), Faltas);
    assert_eq!(s::classify(&mail("[Helpdesk #1] Cartão", "hd@isep.ipp.pt", "x")), Helpdesk);
    assert_eq!(s::classify(&mail("Ementa semanal", "secretariado@isep.ipp.pt", "x")), Geral);
}

#[test]
fn parse_pt_datetime_and_relative_labels() {
    let dt = s::parse_pt_datetime("SEGUNDA-FEIRA, 26 DE OUTUBRO DE 2026 ÀS 23:59").unwrap();
    assert_eq!(dt, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(23, 59, 0).unwrap());
    assert_eq!(s::relative_label(dt, now()), "HOJE 23:59");
    assert_eq!(s::relative_label(dt + chrono::Duration::days(1), now()), "AMANHÃ 23:59");
    assert!(s::relative_label(dt + chrono::Duration::days(3), now()).contains("em 3 dias"));
    assert!(s::relative_label(dt - chrono::Duration::days(2), now()).starts_with("terminou"));
    assert!(s::parse_pt_datetime("sem data").is_none());
    assert!(s::parse_pt_datetime("31 de smarch de 2026 às 10:00").is_none());
}

#[test]
fn first_sentences_never_cuts_a_word() {
    let text = format!("Primeira frase completa e bastante longa para passar do limiar. {}", "palavra ".repeat(60));
    assert_eq!(s::first_sentences(&text, 90), "Primeira frase completa e bastante longa para passar do limiar.");
    let out = s::first_sentences(&"palavra ".repeat(60), 50);
    assert!(out.ends_with('…') && !out.trim_end_matches('…').ends_with("palav"));
    assert_eq!(s::first_sentences("curto", 50), "curto");
}

#[test]
fn body_excerpt_drops_urls_refs_css_and_greetings() {
    let body = "Olá Aluno,\n\n.cls { font-family: Arial; }\n/* css */\nO teste é dia 5 [1] veja https://x.pt/a\nCumprimentos,\nDocente";
    assert_eq!(s::body_excerpt(body, 0, 240), "O teste é dia 5 veja");
}

#[test]
fn short_course_cleans_names() {
    assert_eq!(s::short_course("DEI - Laboratório / Projeto III - 1º Semestre 2026/2027"), "Laboratório / Projeto III");
    assert_eq!(s::short_course("LAPR3(85519)"), "LAPR3");
    assert_eq!(s::short_course("SEM_3_PI 2026/2027"), "SEM_3_PI");
    assert_eq!(s::short_course("  "), "Disciplina");
}

#[test]
fn prazos_groups_same_due_date_and_sorts_overdue_first() {
    let emails = vec![
        reminder_mail("2DO-2DP-C - SPRINT 1 - Status Report", "segunda-feira, 26 de outubro de 2026 às 23:59"),
        reminder_mail("2NA-2NB - SPRINT 1 - Status Report", "segunda-feira, 26 de outubro de 2026 às 23:59"),
        reminder_mail("Outro", "quinta-feira, 29 de outubro de 2026 às 12:00"),
        overdue_mail(ENDED, LATE_PASSED),
    ];
    let text = s::build_summary(&emails, &[], now());
    let prazos = text.split("⏰ PRAZOS").nth(1).unwrap().split('━').next().unwrap().to_string();
    assert!(prazos.find('🔴').unwrap() < prazos.find("Laboratório / Projeto III").unwrap());
    assert!(prazos.contains("2 entregas: 2DO-2DP-C - SPRINT 1 - Status Report; 2NA-2NB - SPRINT 1 - Status Report"));
    assert!(prazos.contains("HOJE 23:59") && prazos.contains("em 3 dias"));
    assert!(prazos.contains("Estruturas de Informação: First Sprint Submission"));
    assert!(prazos.contains("O prazo terminou dom 25/10 20:00."));
}

#[test]
fn many_deliveries_are_collapsed() {
    let emails: Vec<Email> = (1..=5).map(|i| reminder_mail(&format!("T{i}"), "segunda-feira, 26 de outubro de 2026 às 23:59")).collect();
    let text = s::build_summary(&emails, &[], now());
    assert!(text.contains("5 entregas (T1; T2; +3)"), "{text}");
}

#[test]
fn overdue_mentions_late_window_only_while_open() {
    let open = s::build_summary(&[overdue_mail(ENDED, "SEGUNDA-FEIRA, 26 DE OUTUBRO DE 2026 ÀS 23:59")], &[], now());
    assert!(open.contains("Ainda podes entregar até seg 26/10 23:59"), "{open}");
    let closed = s::build_summary(&[overdue_mail(ENDED, LATE_PASSED)], &[], now());
    assert!(!closed.contains("Ainda podes entregar"));
}

#[test]
fn past_reminders_are_dropped() {
    let out = s::build_summary(&[reminder_mail("Velho", "domingo, 25 de outubro de 2026 às 20:00")], &[], now());
    assert!(!out.contains("⏰ PRAZOS"));
}

#[test]
fn forum_announcements_are_deduplicated_across_courses() {
    let a = forum_mail("LAPR3(85519)", "Requisitos para o Sprint 2", "Docente Teste", "Grupos pequenos simplificam a US10.");
    let b = forum_mail("SEM_3_PI 2026/2027", "Requisitos para o Sprint 2", "Docente Teste", "Grupos pequenos simplificam a US10.");
    let out = s::build_summary(&[a, b], &[], now());
    assert_eq!(out.matches("Requisitos para o Sprint 2").count(), 1);
    assert!(out.contains("LAPR3 / SEM_3_PI — Requisitos para o Sprint 2"));
    assert!(out.contains("Docente Teste: “Grupos pequenos simplificam a US10.”"));
}

#[test]
fn student_forum_posts_are_labelled_as_colleagues() {
    let out = s::build_summary(&[forum_mail("ARQCP", "Dúvida", "DIAS 1251344", "Como funciona?")], &[], now());
    assert!(out.contains("DIAS 1251344 (colega)"), "{out}");
}

#[test]
fn faltas_counts_per_course() {
    let body = "Caro/a estudante, Segue-se o resumo das aulas: - T - AMATA (Análise Matemática) no dia 19/10/2026, das 18:10 às 19:00. \
                - TP - AMATA (Análise Matemática) no dia 19/10/2026, das 20:40 às 22:00. \
                - PL - LAPR3 (Laboratório / Projeto III) no dia 19/10/2026, das 20:40 às 22:00.";
    let out = s::build_summary(&[mail("Faltas - Semana de 18/10/2026 a 24/10/2026", "seraa@isep.ipp.pt", body)], &[], now());
    assert!(out.contains("Semana 18/10–24/10: 3 aula(s) com falta — AMATA ×2, LAPR3 ×1"), "{out}");
}

#[test]
fn helpdesk_collapses_one_ticket_into_one_item() {
    let mails = vec![
        mail("[HelpDesk #77] Auto-resposta/AutoReply: Cartão", "hd@isep.ipp.pt", "Recebemos o seu pedido!"),
        mail("[Helpdesk #77] Cartão definitivo", "pessoa@isep.ipp.pt", "O seu cartão já se encontra ativo. Use-o nas aulas."),
        mail("[Helpdesk #77] Respondido e Fechado/Answered and Closed: Cartão", "hd@isep.ipp.pt", "Pedido fechado."),
    ];
    let out = s::build_summary(&mails, &[], now());
    assert_eq!(out.matches("#77").count(), 1);
    assert!(out.contains("(pedido fechado)") && out.contains("O seu cartão já se encontra ativo."));
}

#[test]
fn security_notices_are_aggregated_and_geral_is_listed() {
    let mails = vec![
        mail("Novo início de sessão com a sua conta 'Moodle'", MOODLE, "x"),
        mail("Novo início de sessão com a sua conta 'Moodle'", MOODLE, "y"),
        mail("Ementa Semanal - Bar", "secretariado@isep.ipp.pt", "Segue a ementa da semana."),
    ];
    let out = s::build_summary(&mails, &[], now());
    assert!(out.contains("2 aviso(s) de novo início de sessão"));
    assert!(out.contains("• Ementa Semanal - Bar — Segue a ementa da semana."));
}

#[test]
fn moodle_lines_are_inserted_after_forum_and_header_counts_emails() {
    let out = s::build_summary(&[forum_mail("LAPR3", "T", "D", "x")], &["📥 NOVO NO MOODLE (1 ficheiro(s))".into()], now());
    assert!(out.find("📢").unwrap() < out.find("📥").unwrap());
    assert!(out.starts_with("📬 RESUMO ISEP — seg 26/10 19:40\n1 emails novos"));
}

#[test]
fn summarize_emails_rules_by_default_and_empty_case() {
    let cfg = json_map(json!({}));
    assert!(s::summarize_emails(&[], &cfg, &[]).contains("sem emails novos"));
    assert!(s::summarize_emails(&[mail("Aviso", "x@isep.ipp.pt", "Texto do aviso.")], &cfg, &[]).starts_with("📬 RESUMO ISEP"));
    assert!(s::summarize_emails(&[], &cfg, &["📥 NOVO NO MOODLE (1 ficheiro(s))".to_string()]).contains("NOVO NO MOODLE"));
}

#[test]
fn ai_mode_falls_back_with_notice() {
    let _home = Home::new();
    let cfg = json_map(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"}));
    let out = s::summarize_emails(&[mail("Aviso", "x@isep.ipp.pt", "Texto do aviso.")], &cfg, &[]);
    assert!(out.starts_with("📬 RESUMO ISEP") && out.contains("IA indisponível (Sem token de API"), "{out}");
}

#[test]
fn ai_mode_uses_ai_text_when_it_answers() {
    let _home = Home::new();
    let srv = fake_server(|_| Reply::json(200, json!({"choices": [{"message": {"content": "TEXTO DA IA ".repeat(5)}}]})));
    let cfg = json_map(json!({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K",
                              "ai_model": "m", "ai_base_url": format!("{}/v1/chat/completions", srv.base)}));
    assert_eq!(s::summarize_emails(&[mail("Aviso", "x", "y")], &cfg, &[]), "TEXTO DA IA ".repeat(5).trim().to_string());
}

// ---------------------------------------------------------------- análise dos ficheiros do Moodle

fn entry(name: &str, kind: &str, course: &str, section: &str, path: &str, ts: &str) -> NewFile {
    NewFile { ts: ts.into(), kind: kind.into(), course: course.into(), section: section.into(), name: name.into(), path: path.into(), size: 10 }
}

const LAB: &str = "DEI - Laboratório / Projeto III - 1º Semestre 2026/2027";

#[test]
fn classify_file_by_name_and_section() {
    assert_eq!(mr::classify_file("Enunciado Sprint 2.pdf", ""), "enunciado");
    assert_eq!(mr::classify_file("LAPR3 Evaluation Guide", ""), "avaliação");
    assert_eq!(mr::classify_file("Ficha de trabalho 3.pdf", ""), "ficha");
    assert_eq!(mr::classify_file("T3.pdf", "Slides teóricos"), "slides");
    assert_eq!(mr::classify_file("coisa.zip", "Outros"), "material");
}

#[test]
fn load_new_files_filters_by_time_and_keeps_latest_per_path() {
    let home = Home::new();
    let lines = [
        json!(entry("a.pdf", "novo", LAB, "S", "a.pdf", "2026-10-20T10:00:00")),
        json!(entry("b.pdf", "novo", LAB, "S", "b.pdf", "2026-10-26T10:00:00")),
        json!(entry("b.pdf", "atualizado", LAB, "S", "b.pdf", "2026-10-26T12:00:00")),
    ];
    let mut text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    text.push_str("isto não é json\n");
    let path = home.path("MoodleSync/new_files.jsonl");
    std::fs::write(&path, text).unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap().and_hms_opt(0, 0, 0).unwrap();
    let found = mr::load_new_files(&path, since);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].name.as_str(), found[0].kind.as_str()), ("b.pdf", "atualizado"));
    assert!(mr::load_new_files(&home.path("naoexiste.jsonl"), chrono::NaiveDateTime::MIN).is_empty());
}

#[test]
fn section_groups_by_course_and_reads_text_hint() {
    let home = Home::new();
    let dl = home.path("dl");
    std::fs::create_dir_all(dl.join("Lab")).unwrap();
    std::fs::write(dl.join("Lab/Enunciado Sprint 2.txt"), "Enunciado do Sprint 2\nA entrega do relatório tem prazo até 8 de novembro de 2026 no Moodle.\n").unwrap();
    let entries = vec![
        entry("Enunciado Sprint 2.txt", "novo", LAB, "Secção 01", "Lab/Enunciado Sprint 2.txt", "2026-10-26T19:00:00"),
        entry("Slides T3.pdf", "atualizado", "DEI - Estruturas de Informação - 1º Semestre 2026/2027", "Secção 01", "x/Slides T3.pdf", "2026-10-26T19:00:00"),
    ];
    let lines = mr::build_moodle_section(&entries, &dl, 6);
    let text = lines.join("\n");
    assert_eq!(lines[0], "📥 NOVO NO MOODLE (2 ficheiro(s))");
    assert!(text.contains("• Laboratório / Projeto III") && text.contains("• Estruturas de Informação"));
    assert!(text.contains("Enunciado Sprint 2.txt [enunciado, novo]"));
    assert!(text.contains("menciona prazo") && text.contains("8 de novembro"), "{text}");
    assert!(text.contains("Slides T3.pdf [slides, atualizado]"));
}

#[test]
fn section_caps_files_per_course_and_handles_missing_files() {
    let home = Home::new();
    let entries: Vec<NewFile> = (0..9).map(|i| entry(&format!("f{i}.pdf"), "novo", LAB, "S", "nao/existe.pdf", "2026-10-26T19:00:00")).collect();
    let text = mr::build_moodle_section(&entries, &home.path("dl"), 4).join("\n");
    assert!(text.contains("f3.pdf") && !text.contains("f4.pdf") && text.contains("e mais 5"));
    assert!(mr::build_moodle_section(&[], &home.path("dl"), 4).is_empty());
}
