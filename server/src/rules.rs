//! Regras editáveis do resumo (modo «Regras»): palavras importantes e remetentes/assuntos a ignorar.
//!
//! O utilizador escreve-as nas Definições; antes de guardar são validadas aqui (tamanho, caracteres, duplicados).
use crate::model::Email;
use crate::util::Json;
use serde_json::{json, Value};

pub const MAX_ENTRIES: usize = 30;
pub const MIN_LEN: usize = 2;
pub const MAX_LEN: usize = 60;
pub const FIELDS: [&str; 3] = ["important_keywords", "ignore_senders", "ignore_subjects"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rules {
    /// Emails com estas palavras (assunto, remetente ou texto) aparecem primeiro, em «Importante».
    pub important_keywords: Vec<String>,
    /// Emails cujo remetente contém isto são ignorados.
    pub ignore_senders: Vec<String>,
    /// Emails cujo assunto contém isto são ignorados.
    pub ignore_subjects: Vec<String>,
}

fn list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn contains_any(haystack: &str, needles: &[String]) -> bool {
    let h = haystack.to_lowercase();
    needles.iter().any(|n| h.contains(&n.to_lowercase()))
}

impl Rules {
    pub fn from_config(config: &Json) -> Rules {
        let v = config.get("summary_rules");
        Rules {
            important_keywords: list(v.and_then(|r| r.get("important_keywords"))),
            ignore_senders: list(v.and_then(|r| r.get("ignore_senders"))),
            ignore_subjects: list(v.and_then(|r| r.get("ignore_subjects"))),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({"important_keywords": self.important_keywords, "ignore_senders": self.ignore_senders, "ignore_subjects": self.ignore_subjects})
    }

    pub fn is_empty(&self) -> bool {
        self.important_keywords.is_empty() && self.ignore_senders.is_empty() && self.ignore_subjects.is_empty()
    }

    pub fn is_ignored(&self, e: &Email) -> bool {
        contains_any(&e.sender, &self.ignore_senders) || contains_any(&e.subject, &self.ignore_subjects)
    }

    pub fn is_important(&self, e: &Email) -> bool {
        !self.important_keywords.is_empty() && contains_any(&format!("{} {} {}", e.subject, e.sender, e.body), &self.important_keywords)
    }

    /// Aplica uma alteração parcial. Devolve as regras novas ou todos os erros (campo -> mensagem).
    pub fn patch(&self, patch: &Value) -> Result<Rules, Vec<(String, String)>> {
        let Some(obj) = patch.as_object() else { return Err(vec![("rules".into(), "deve ser um objeto".into())]) };
        let mut errors: Vec<(String, String)> = Vec::new();
        let mut out = self.clone();
        for key in obj.keys().filter(|k| !FIELDS.contains(&k.as_str())) {
            errors.push((key.clone(), "regra desconhecida".into()));
        }
        for field in FIELDS {
            let Some(raw) = obj.get(field) else { continue };
            match clean_list(raw) {
                Ok(items) => match field {
                    "important_keywords" => out.important_keywords = items,
                    "ignore_senders" => out.ignore_senders = items,
                    _ => out.ignore_subjects = items,
                },
                Err(msg) => errors.push((field.into(), msg)),
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors)
        }
    }
}

/// Valida e limpa uma lista escrita pelo utilizador.
fn clean_list(raw: &Value) -> Result<Vec<String>, String> {
    let Some(items) = raw.as_array() else { return Err("deve ser uma lista de textos".into()) };
    let mut out: Vec<String> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else { return Err(format!("o item {} não é texto", i + 1)) };
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue; // linhas em branco são ignoradas
        }
        let n = text.chars().count();
        if n < MIN_LEN {
            return Err(format!("«{text}» é demasiado curto (mínimo {MIN_LEN} caracteres)"));
        }
        if n > MAX_LEN {
            return Err(format!("«{}…» é demasiado longo (máximo {MAX_LEN} caracteres)", text.chars().take(20).collect::<String>()));
        }
        if text.chars().any(char::is_control) {
            return Err(format!("«{text}» tem caracteres inválidos"));
        }
        if !out.iter().any(|x| x.to_lowercase() == text.to_lowercase()) {
            out.push(text);
        }
    }
    if out.len() > MAX_ENTRIES {
        return Err(format!("no máximo {MAX_ENTRIES} entradas (tens {})", out.len()));
    }
    Ok(out)
}
