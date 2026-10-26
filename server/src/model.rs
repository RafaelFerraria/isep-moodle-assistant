//! Tipos partilhados.
use serde::{Deserialize, Serialize};

/// Um email já descodificado (assunto, remetente e texto simples).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Email {
    pub subject: String,
    pub sender: String,
    #[serde(default)]
    pub sender_email: String,
    #[serde(default)]
    pub received_time: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub message_id: String,
}

impl Email {
    pub fn new(subject: &str, sender: &str, body: &str) -> Self {
        Email {
            subject: subject.into(),
            sender: sender.into(),
            sender_email: sender.into(),
            received_time: String::new(),
            body: body.into(),
            message_id: String::new(),
        }
    }
}

/// Ficheiro novo/atualizado no Moodle (uma linha de new_files.jsonl).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NewFile {
    pub ts: String,
    pub kind: String, // "novo" | "atualizado"
    #[serde(default)]
    pub course: String,
    #[serde(default)]
    pub section: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub size: u64,
}
