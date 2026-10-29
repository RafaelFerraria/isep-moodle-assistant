//! Resumo por regras: classifica os emails, agrupa por assunto e escreve um texto claro para o telemóvel.
use crate::ai;
use crate::model::Email;
use crate::util::Json;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use regex::Regex;
use std::sync::LazyLock;
use crate::rules::Rules;

pub const LINE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━";
const DIAS_SEMANA: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}

re!(PT_DATETIME, r"(?i)(\d{1,2})\s+de\s+([a-zçã]+)\s+de\s+(\d{4})\s+[àa]s\s+(\d{1,2}):(\d{2})");
re!(RE_PREFIX, r"(?i)^((re|fwd|fw|enc)\s*:\s*)+");
re!(RE_VIA, r"\(via [^)]*\)");
re!(RE_ANGLE, r"<[^>]*>");
re!(RE_DEI, r"^DEI\s*-\s*");
re!(RE_SEMESTRE, r"(?i)\s*-\s*\d+º\s*Semestre.*$");
re!(RE_PAREN_NUM, r"\s*\(\d+\)");
re!(RE_YEARS, r"\s*20\d\d\s*/\s*20\d\d");
re!(BOILERPLATE_STOP, r"(?i)^(responder|ver o t[óo]pico|ver a mensagem|cumprimentos|atentamente|-{4,}|_{4,}|links:|para (cancelar|alterar)|you can reply|this message)");
re!(GREETING, r"(?i)^(ol[áa]|caro|cara|bom dia|boa tarde|boa noite|ex\.mos)\b.{0,30}[,.]?$");
re!(RE_URL, r"<?https?://\S+>?");
re!(RE_REF, r"\[\d+\]");
re!(RE_CSS, r"[{};]|/\*|\*/|font-family|border-spacing|@media");
re!(RE_FALTAS_SUBJ, r"(?i)^faltas\s*-\s*semana");
re!(RE_HELPDESK, r"(?i)\[help\s?desk\s*#\d+\]");
re!(RE_FORUM_PART, r"»\s*([^»<]+?)\s*<");
re!(RE_POR, r"(?i)^\s*por\s+.+");
re!(RE_COURSE, r"(?i)na disciplina\s+'?(.+?)'?\s+(?:atingirá|terminou|vence|está)");
re!(RE_LATE, r"(?i)Ainda poderá submeter.*?(\d{1,2}\s+de\s+\w+\s+de\s+\d{4}\s+[àa]s\s+\d{1,2}:\d{2})");
re!(RE_REMINDER_TITLE, r"(?i)^data limite em .*?\d{1,2}:\d{2}\s*:\s*(.+)$");
re!(RE_ATRASADO, r"(?i)^atrasado\s*:\s*");
re!(RE_FALTA_ITEM, r"\b([A-Z]{1,3})\s*-\s*(\w+)\s*\([^)]*\)\s*no dia\s*\d{2}/\d{2}/\d{4}");
re!(RE_WEEK, r"(?i)semana de\s*(\d{2}/\d{2})/\d{4}\s*a\s*(\d{2}/\d{2})/\d{4}");
re!(RE_TICKET, r"#(\d+)");
re!(RE_AUTO, r"(?i)auto-?resposta|autoreply|respondido e fechado");
re!(RE_CLOSED, r"(?i)fechado|closed");
re!(RE_BRACKET_PREFIX, r"^\[[^\]]*\]\s*");
re!(RE_STUDENT_NUM, r"\d{7}");

// ---------------------------------------------------------------- utilitários de texto

/// Junta quebras de linha e espaços repetidos numa só linha.
pub fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn month_number(name: &str) -> Option<u32> {
    Some(match name.to_lowercase().as_str() {
        "janeiro" => 1,
        "fevereiro" => 2,
        "março" | "marco" => 3,
        "abril" => 4,
        "maio" => 5,
        "junho" => 6,
        "julho" => 7,
        "agosto" => 8,
        "setembro" => 9,
        "outubro" => 10,
        "novembro" => 11,
        "dezembro" => 12,
        _ => return None,
    })
}

/// Lê "26 de outubro de 2026 às 23:59" (qualquer capitalização) de dentro de um texto.
pub fn parse_pt_datetime(text: &str) -> Option<NaiveDateTime> {
    let m = PT_DATETIME.captures(text)?;
    let month = month_number(&m[2])?;
    NaiveDate::from_ymd_opt(m[3].parse().ok()?, month, m[1].parse().ok()?)?
        .and_hms_opt(m[4].parse().ok()?, m[5].parse().ok()?, 0)
}

pub fn fmt_dt(dt: NaiveDateTime) -> String {
    format!("{} {:02}/{:02} {:02}:{:02}", DIAS_SEMANA[dt.weekday().num_days_from_monday() as usize], dt.day(), dt.month(), dt.hour(), dt.minute())
}

/// "HOJE 23:59", "AMANHÃ 23:59", "ter 27/10 23:59 (em 2 dias)".
pub fn relative_label(dt: NaiveDateTime, now: NaiveDateTime) -> String {
    let days = (dt.date() - now.date()).num_days();
    if dt < now {
        return format!("terminou {}", fmt_dt(dt));
    }
    match days {
        0 => format!("HOJE {:02}:{:02}", dt.hour(), dt.minute()),
        1 => format!("AMANHÃ {:02}:{:02}", dt.hour(), dt.minute()),
        _ => format!("{} (em {days} dias)", fmt_dt(dt)),
    }
}

/// "DEI - Laboratório / Projeto III - 1º Semestre 2026/2027" -> "Laboratório / Projeto III".
pub fn short_course(name: &str) -> String {
    let name = norm(name);
    let name = name.trim_matches(|c| c == '\'' || c == '"');
    let name = RE_DEI.replace(name, "");
    let name = RE_SEMESTRE.replace(&name, "");
    let name = RE_PAREN_NUM.replace_all(&name, ""); // 'LAPR3(85519)' -> 'LAPR3'
    let name = RE_YEARS.replace_all(&name, "");
    let name = name.trim_matches(|c| c == ' ' || c == '-' || c == ':');
    if name.is_empty() { "Disciplina".into() } else { name.to_string() }
}

pub fn clean_sender(sender: &str) -> String {
    let s = RE_ANGLE.replace_all(sender, "");
    let s = RE_VIA.replace_all(&s, "");
    let s = s.replace(['"', '\\'], "");
    let s = norm(&s);
    if s.is_empty() { "Desconhecido".into() } else { s }
}

pub fn clean_subject(subj: &str) -> String {
    norm(&RE_PREFIX.replace(subj, ""))
}

/// Corta em fim de frase (nunca a meio de uma palavra) até `max_chars`.
pub fn first_sentences(text: &str, max_chars: usize) -> String {
    let text = norm(text);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text;
    }
    let cut: Vec<char> = chars[..max_chars].to_vec();
    let rfind = |pat: [char; 2]| (0..cut.len().saturating_sub(1)).rev().find(|&i| cut[i] == pat[0] && cut[i + 1] == pat[1]);
    let end = [rfind(['.', ' ']), rfind(['!', ' ']), rfind(['?', ' '])].into_iter().flatten().max();
    if let Some(e) = end {
        if e as f64 > max_chars as f64 * 0.4 {
            return cut[..=e].iter().collect();
        }
    }
    let s: String = cut.iter().collect();
    let head = s.rsplit_once(' ').map(|(h, _)| h).unwrap_or(&s);
    format!("{}…", head.trim_end_matches([',', ';', ':']))
}

/// Primeiro texto útil do corpo, sem URLs, referências [1], CSS, saudações nem rodapés.
pub fn body_excerpt(body: &str, skip_header_lines: usize, max_chars: usize) -> String {
    let mut useful: Vec<String> = Vec::new();
    for raw in body.lines().skip(skip_header_lines) {
        let line = raw.trim_matches(|c: char| c == ' ' || c == '>' || c == '\t');
        if line.is_empty() {
            continue;
        }
        if BOILERPLATE_STOP.is_match(line) {
            break;
        }
        let line = RE_URL.replace_all(line, "");
        let line = RE_REF.replace_all(&line, "");
        let line = line.trim();
        if line.chars().count() < 3 || GREETING.is_match(line) || RE_CSS.is_match(line) {
            continue;
        }
        useful.push(line.to_string());
    }
    first_sentences(&useful.join(" "), max_chars)
}

// ---------------------------------------------------------------- classificação

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Forum,
    Prazo,
    Seguranca,
    Moodle,
    Faltas,
    Helpdesk,
    Geral,
}

pub fn classify(e: &Email) -> Kind {
    let sender = e.sender.to_lowercase().replace('\\', "");
    let low_subj = e.subject.to_lowercase();
    let first_line = e.body.split('\n').next().unwrap_or("").to_lowercase();
    if sender.contains("moodle") {
        if first_line.contains("» fóruns") {
            return Kind::Forum;
        }
        if low_subj.starts_with("data limite") || low_subj.starts_with("atrasado") || e.body.to_lowercase().contains("data limite:") {
            return Kind::Prazo;
        }
        if low_subj.starts_with("novo início de sessão") || low_subj.starts_with("novo inicio de sessao") {
            return Kind::Seguranca;
        }
        return Kind::Moodle;
    }
    if RE_FALTAS_SUBJ.is_match(&e.subject) {
        return Kind::Faltas;
    }
    if RE_HELPDESK.is_match(&e.subject) {
        return Kind::Helpdesk;
    }
    Kind::Geral
}

// ---------------------------------------------------------------- extração por tipo

pub struct ForumPost {
    pub course: String,
    pub topic: String,
    pub author: String,
    pub text: String,
}

pub fn parse_forum(e: &Email) -> ForumPost {
    let head = e.body.split('\n').next().unwrap_or("");
    let course = match head.split_once('<') {
        Some((before, _)) => short_course(before),
        None => "Moodle".into(),
    };
    let parts: Vec<String> = RE_FORUM_PART.captures_iter(head).map(|c| c[1].trim().to_string()).collect();
    let topic = parts.last().cloned().unwrap_or_else(|| clean_subject(&e.subject));
    // o texto da mensagem vem depois do cabeçalho (breadcrumb, autor, título, 'por X - data')
    let start = e.body.lines().take(12).position(|l| RE_POR.is_match(l)).map(|i| i + 1).unwrap_or(0);
    ForumPost { course, topic, author: clean_sender(&e.sender), text: body_excerpt(&e.body, start, 260) }
}

#[derive(Debug, Clone)]
pub struct Prazo {
    pub overdue: bool,
    pub course: String,
    pub title: String,
    pub due: Option<NaiveDateTime>,
    pub late_until: Option<NaiveDateTime>,
}

pub fn parse_prazo(e: &Email) -> Prazo {
    let subj = clean_subject(&e.subject);
    let body = norm(&e.body);
    let overdue = subj.to_lowercase().starts_with("atrasado");
    let course = RE_COURSE.captures(&body).map(|c| short_course(&c[1])).unwrap_or_else(|| "Moodle".into());

    if overdue {
        let title = RE_ATRASADO.replace(&subj, "").to_string();
        let ended = body.split_once("terminou o prazo").and_then(|(_, rest)| parse_pt_datetime(rest));
        let late_until = RE_LATE.captures(&body).and_then(|c| parse_pt_datetime(&c[1]));
        return Prazo { overdue: true, course, title, due: ended, late_until };
    }
    let title = RE_REMINDER_TITLE.captures(&subj).map(|c| c[1].trim().to_string()).unwrap_or_else(|| subj.clone());
    let due = match body.split_once("DATA LIMITE:") {
        Some((_, rest)) => parse_pt_datetime(rest),
        None => parse_pt_datetime(&subj),
    };
    Prazo { overdue: false, course, title, due, late_until: None }
}

pub struct Faltas {
    pub week: String,
    pub per_course: Vec<(String, usize)>,
}

pub fn parse_faltas(e: &Email) -> Faltas {
    let body = norm(&e.body);
    let mut per_course: Vec<(String, usize)> = Vec::new();
    for m in RE_FALTA_ITEM.captures_iter(&body) {
        let course = m[2].to_string();
        match per_course.iter_mut().find(|(c, _)| *c == course) {
            Some((_, n)) => *n += 1,
            None => per_course.push((course, 1)),
        }
    }
    let week = RE_WEEK.captures(&e.subject).map(|w| format!("{}–{}", &w[1], &w[2])).unwrap_or_default();
    Faltas { week, per_course }
}

// ---------------------------------------------------------------- construção das secções

pub fn section_prazos(items: &[Prazo], now: NaiveDateTime) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    // junta entregas do mesmo curso e prazo (ex.: várias turmas com o mesmo sprint)
    let mut groups: Vec<((String, bool, Option<NaiveDateTime>), Vec<&Prazo>)> = Vec::new();
    for it in items {
        let key = (it.course.clone(), it.overdue, it.due);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(it),
            None => groups.push((key, vec![it])),
        }
    }
    groups.sort_by_key(|((_, overdue, due), _)| (if *overdue { 0 } else { 1 }, due.unwrap_or(NaiveDateTime::MAX)));

    let mut lines = vec!["⏰ PRAZOS".to_string()];
    for ((course, overdue, due), group) in groups {
        let titles: Vec<&str> = group.iter().map(|g| g.title.as_str()).collect();
        let what = if titles.len() > 3 {
            format!("{} entregas ({}; +{})", titles.len(), titles[..2].join("; "), titles.len() - 2)
        } else if titles.len() > 1 {
            format!("{} entregas: {}", titles.len(), titles.join("; "))
        } else {
            titles[0].to_string()
        };
        if overdue {
            let tail = match group[0].late_until {
                Some(late) if late > now => format!(" Ainda podes entregar até {} (fica como atrasado).", fmt_dt(late)),
                _ => String::new(),
            };
            lines.push(format!("🔴 {course}: {what}\n   O prazo terminou {}.{tail}", due.map(fmt_dt).unwrap_or_default()));
        } else {
            let when = due.map(|d| relative_label(d, now)).unwrap_or_else(|| "data não indicada".into());
            let urgent = due.map(|d| {
                let secs = (d - now).num_seconds();
                (0..48 * 3600).contains(&secs)
            }) == Some(true);
            lines.push(format!("{} {course}: {what}\n   Entrega: {when}", if urgent { "🟠" } else { "🟡" }));
        }
    }
    lines
}

pub fn section_avisos(items: &[ForumPost]) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    // o mesmo aviso chega várias vezes (fórum geral + fórum da turma): junta pelo título
    struct Merged {
        topic: String,
        author: String,
        text: String,
        courses: Vec<String>,
    }
    let mut merged: Vec<(String, Merged)> = Vec::new();
    for it in items {
        let key = it.topic.to_lowercase();
        match merged.iter_mut().find(|(k, _)| *k == key) {
            Some((_, m)) => {
                if !m.courses.contains(&it.course) {
                    m.courses.push(it.course.clone());
                }
                if it.text.chars().count() > m.text.chars().count() {
                    m.text = it.text.clone();
                }
            }
            None => merged.push((key, Merged { topic: it.topic.clone(), author: it.author.clone(), text: it.text.clone(), courses: vec![it.course.clone()] })),
        }
    }
    let mut lines = vec!["📢 FÓRUNS & AVISOS".to_string()];
    for (_, m) in merged {
        lines.push(format!("• {} — {}", m.courses.join(" / "), m.topic));
        let who = format!("{}{}", m.author, if RE_STUDENT_NUM.is_match(&m.author) { " (colega)" } else { "" });
        lines.push(if m.text.is_empty() { format!("  {who} (sem texto; vê no fórum)") } else { format!("  {who}: “{}”", m.text) });
    }
    lines
}

pub fn section_faltas(items: &[Faltas]) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    let mut lines = vec!["🗓 FALTAS".to_string()];
    for f in items {
        let total: usize = f.per_course.iter().map(|(_, n)| n).sum();
        let detail = f.per_course.iter().map(|(c, n)| format!("{c} ×{n}")).collect::<Vec<_>>().join(", ");
        lines.push(if total > 0 {
            format!("• Semana {}: {total} aula(s) com falta — {detail}", f.week)
        } else {
            format!("• Semana {}: ver o detalhe no email", f.week)
        });
    }
    lines
}

fn strip_bracket(s: &str) -> String {
    RE_BRACKET_PREFIX.replace(s, "").to_string()
}

pub fn section_helpdesk(emails: &[&Email]) -> Vec<String> {
    if emails.is_empty() {
        return vec![];
    }
    let mut tickets: Vec<(String, Vec<&Email>)> = Vec::new();
    for e in emails {
        let id = RE_TICKET.captures(&e.subject).map(|c| c[1].to_string()).unwrap_or_else(|| "?".into());
        match tickets.iter_mut().find(|(t, _)| *t == id) {
            Some((_, v)) => v.push(e),
            None => tickets.push((id, vec![e])),
        }
    }
    let mut lines = vec!["🛠 HELPDESK".to_string()];
    for (tid, group) in tickets {
        // a resposta humana (não a auto-resposta) é a que interessa
        let human: Vec<&&Email> = group.iter().filter(|g| !RE_AUTO.is_match(&g.subject)).collect();
        let pick: &Email = human.first().map(|h| **h).unwrap_or(group[0]);
        let subj = strip_bracket(&clean_subject(&pick.subject));
        let closed = group.iter().any(|g| RE_CLOSED.is_match(&g.subject));
        lines.push(format!("• #{tid} {subj}{}", if closed { " (pedido fechado)" } else { "" }));
        let txt = if human.is_empty() { String::new() } else { body_excerpt(&pick.body, 0, 200) };
        if !txt.is_empty() {
            lines.push(format!("  {txt}"));
        }
    }
    lines
}

pub fn section_geral(emails: &[&Email]) -> Vec<String> {
    if emails.is_empty() {
        return vec![];
    }
    let mut lines = vec!["ℹ️ OUTROS".to_string()];
    for e in emails {
        let subj = strip_bracket(&clean_subject(if e.subject.is_empty() { "Sem assunto" } else { &e.subject }));
        let txt = body_excerpt(&e.body, 0, 140);
        if !txt.is_empty() && !subj.to_lowercase().contains(&txt.to_lowercase()) {
            lines.push(format!("• {subj} — {txt}"));
        } else {
            lines.push(format!("• {subj}"));
        }
    }
    lines
}

pub fn build_summary(emails: &[Email], moodle_lines: &[String], now: NaiveDateTime) -> String {
    build_summary_with(emails, moodle_lines, now, &Rules::default())
}

/// «Importante»: emails que contêm uma das palavras escolhidas pelo utilizador. Aparecem primeiro e saem das outras secções.
pub fn section_importante(emails: &[&Email]) -> Vec<String> {
    if emails.is_empty() {
        return vec![];
    }
    let mut lines = vec!["⭐ IMPORTANTE".to_string()];
    for e in emails {
        let subj = strip_bracket(&clean_subject(if e.subject.is_empty() { "Sem assunto" } else { &e.subject }));
        let txt = body_excerpt(&e.body, 0, 140);
        lines.push(if txt.is_empty() { format!("• {subj}") } else { format!("• {subj} — {txt}") });
    }
    lines
}

pub fn build_summary_with(emails: &[Email], moodle_lines: &[String], now: NaiveDateTime, rules: &Rules) -> String {
    let important: Vec<&Email> = emails.iter().filter(|e| rules.is_important(e)).collect();
    let emails: Vec<Email> = emails.iter().filter(|e| !rules.is_important(e)).cloned().collect();
    let emails = &emails[..];
    let of = |k: Kind| -> Vec<&Email> { emails.iter().filter(|e| classify(e) == k).collect() };
    let prazos: Vec<Prazo> = of(Kind::Prazo)
        .into_iter()
        .map(parse_prazo)
        .filter(|p| p.overdue || p.due.is_none() || p.due.map(|d| d >= now) == Some(true))
        .collect();
    let forum: Vec<ForumPost> = of(Kind::Forum).into_iter().map(parse_forum).collect();
    let faltas: Vec<Faltas> = of(Kind::Faltas).into_iter().map(parse_faltas).collect();
    let mut geral = of(Kind::Geral);
    geral.extend(of(Kind::Moodle));
    let seguranca = of(Kind::Seguranca).len();

    let sections: Vec<Vec<String>> = vec![
        section_importante(&important),
        section_prazos(&prazos, now),
        section_avisos(&forum),
        moodle_lines.to_vec(),
        section_faltas(&faltas),
        section_helpdesk(&of(Kind::Helpdesk)),
        section_geral(&geral),
    ];
    let mut out: Vec<String> = vec![
        format!("📬 RESUMO ISEP — {} {:02}/{:02} {:02}:{:02}", DIAS_SEMANA[now.weekday().num_days_from_monday() as usize], now.day(), now.month(), now.hour(), now.minute()),
        format!("{} emails novos", emails.len() + important.len()),
    ];
    for sec in sections.into_iter().filter(|s| !s.is_empty()) {
        out.push(String::new());
        out.push(LINE.into());
        out.extend(sec);
    }
    if seguranca > 0 {
        out.push(String::new());
        out.push(LINE.into());
        out.push(format!("🔐 Moodle: {seguranca} aviso(s) de novo início de sessão (se foste tu, ignora)."));
    }
    out.join("\n")
}

/// Emails fictícios para testar a ligação à IA / pré-visualizar o formato sem ler a caixa de correio.
pub fn sample_emails() -> Vec<Email> {
    vec![
        Email::new(
            "Data limite em amanhã às 23:59: Sprint 2 - Entrega",
            "Moodle ISEP (via MoodleISEP)",
            "Olá, o trabalho Sprint 2 na disciplina LAPR3 atingirá a data limite brevemente.",
        ),
        Email::new("Aula de substituição de ESINF", "Docente ESINF", "A aula de quinta-feira passa para a sala F105, às 10h."),
    ]
}

/// Resumo por regras (local) ou, se `summary_mode == "ai"`, reescrito por uma IA.
/// Se a IA falhar (CLI sem login, token inválido, sem rede), cai para as regras e avisa no fim.
pub fn summarize_emails(emails: &[Email], config: &Json, moodle_lines: &[String]) -> String {
    let rules = Rules::from_config(config);
    // as regras de «ignorar» valem nos dois modos: o que o utilizador não quer ver nunca chega à IA
    let kept: Vec<Email> = emails.iter().filter(|e| !rules.is_ignored(e)).cloned().collect();
    let emails = &kept[..];
    if emails.is_empty() && moodle_lines.is_empty() {
        return "📬 ISEP: sem emails novos nem ficheiros novos no Moodle desde a última verificação.".into();
    }
    let now = Local::now().naive_local();
    if crate::util::str_or(config, "summary_mode", "rules").to_lowercase() == "ai" {
        let today = format!("{} {:02}/{:02}/{} {:02}:{:02}", DIAS_SEMANA[now.weekday().num_days_from_monday() as usize], now.day(), now.month(), now.year(), now.hour(), now.minute());
        return match ai::summarize_with_ai(emails, config, moodle_lines, &today) {
            Ok(text) => text,
            Err(err) => {
                let short: String = norm(&err).chars().take(140).collect();
                format!("{}\n\n⚠️ IA indisponível ({short}); resumo gerado por regras.", build_summary(emails, moodle_lines, now))
            }
        };
    }
    build_summary_with(emails, moodle_lines, now, &rules)
}
