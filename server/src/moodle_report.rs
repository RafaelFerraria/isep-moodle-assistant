//! Análise dos ficheiros que o sincronizador descarregou, para entrar no resumo diário.
use crate::model::NewFile;
use crate::summarizer::{first_sentences, norm, short_course};
use chrono::NaiveDateTime;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

/// (tipo, palavras-chave no nome do ficheiro/secção) por ordem de prioridade.
const KINDS: &[(&str, &[&str])] = &[
    ("enunciado", &["enunciado", "requisitos", "requirements", "trabalho prático", "projeto", "project", "sprint", "assignment"]),
    ("avaliação", &["avalia", "evaluation", "teste", "exame", "frequência", "grading", "rubric", "critérios"]),
    ("ficha", &["ficha", "exercício", "exercicio", "worksheet", "guião", "guiao"]),
    ("slides", &["slides", "apresenta", "presentation", "teórica", "teorica", "lecture"]),
];

static DEADLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)([^.\n]{0,60}\b(?:prazo|entrega|submiss[ãa]o|deadline|due|data limite)\b[^.\n]{0,40}\d{1,2}\s*(?:[/-]\s*\d{1,2}|de\s+[a-zç]+|\s+[A-Z][a-z]+)[^.\n]{0,25})",
    )
    .unwrap()
});

pub fn classify_file(name: &str, section: &str) -> &'static str {
    let probe = format!("{name} {section}").to_lowercase();
    KINDS.iter().find(|(_, words)| words.iter().any(|w| probe.contains(w))).map(|(k, _)| *k).unwrap_or("material")
}

/// Texto do início do ficheiro (.txt/.md). PDFs ficam sem pista: a versão Rust ainda não lê PDFs.
fn read_text_preview(path: &Path, max_chars: usize) -> String {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "txt" || ext == "md" {
        if let Ok(bytes) = std::fs::read(path) {
            return String::from_utf8_lossy(&bytes).chars().take(max_chars).collect();
        }
    }
    String::new()
}

pub struct Analysis {
    pub kind: &'static str,
    pub hint: String,
}

pub fn analyse(entry: &NewFile, download_dir: &Path) -> Analysis {
    let kind = classify_file(&entry.name, &entry.section);
    let text = read_text_preview(&download_dir.join(&entry.path), 4000);
    let mut hint = String::new();
    if !text.is_empty() {
        if let Some(m) = DEADLINE_RE.captures(&norm(&text)) {
            hint = format!("menciona prazo: “{}”", first_sentences(m[1].trim(), 110));
        } else if kind != "material" || entry.name.to_lowercase().ends_with(".txt") {
            if let Some(line) = text.lines().map(str::trim).find(|l| l.chars().count() > 25) {
                hint = format!("começa por: “{}”", first_sentences(line, 110));
            }
        }
    }
    Analysis { kind, hint }
}

/// Entradas de new_files.jsonl posteriores a `since` (um registo por ficheiro: o mais recente).
pub fn load_new_files(jsonl_path: &Path, since: NaiveDateTime) -> Vec<NewFile> {
    let Ok(text) = std::fs::read_to_string(jsonl_path) else { return vec![] };
    let mut seen: Vec<NewFile> = Vec::new();
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<NewFile>(line) else { continue };
        let Ok(ts) = NaiveDateTime::parse_from_str(&e.ts, "%Y-%m-%dT%H:%M:%S") else { continue };
        if ts > since {
            match seen.iter_mut().find(|x| x.path == e.path) {
                Some(slot) => *slot = e,
                None => seen.push(e),
            }
        }
    }
    seen
}

/// Secção "📥 NOVO NO MOODLE" agrupada por disciplina, com o tipo de cada ficheiro e uma pista do conteúdo.
pub fn build_moodle_section(entries: &[NewFile], download_dir: &Path, max_per_course: usize) -> Vec<String> {
    if entries.is_empty() {
        return vec![];
    }
    let mut by_course: Vec<(String, Vec<&NewFile>)> = Vec::new();
    for e in entries {
        let course = short_course(&e.course);
        match by_course.iter_mut().find(|(c, _)| *c == course) {
            Some((_, v)) => v.push(e),
            None => by_course.push((course, vec![e])),
        }
    }
    let mut lines = vec![format!("📥 NOVO NO MOODLE ({} ficheiro(s))", entries.len())];
    for (course, items) in by_course {
        lines.push(format!("• {course}"));
        for e in items.iter().take(max_per_course) {
            let a = analyse(e, download_dir);
            let tag = if e.kind == "atualizado" { "atualizado" } else { "novo" };
            lines.push(format!("  – {} [{}, {}]", e.name, a.kind, tag));
            if !a.hint.is_empty() {
                lines.push(format!("    {}", a.hint));
            }
        }
        if items.len() > max_per_course {
            lines.push(format!("  … e mais {}", items.len() - max_per_course));
        }
    }
    lines
}
