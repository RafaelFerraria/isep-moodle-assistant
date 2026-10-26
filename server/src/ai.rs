//! Resumo com IA: uma CLI com a tua conta (Claude, Gemini, Codex…) ou um token de API.
//!
//! Configuração (`config.json`):
//! * `ai_provider: "cli"` + `ai_cli: "claude" | "gemini" | "codex" | "custom"` (+ `ai_cli_command`)
//! * `ai_provider: "api"` + `ai_api_type: "openai" | "anthropic" | "gemini" | "openai_compatible"`
//!   + `ai_api_key` (ou variável de ambiente), `ai_model`, `ai_base_url`
use crate::http::HttpClient;
use crate::model::Email;
use crate::util::{run_with_timeout, str_of, str_or, which, Json};
use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;

pub const CLI_NAMES: &[&str] = &["claude", "gemini", "codex"];
pub const API_TYPES: &[&str] = &["openai", "anthropic", "gemini", "openai_compatible"];
pub const TIMEOUT: Duration = Duration::from_secs(180);
pub const MAX_EMAILS: usize = 30;
pub const MAX_BODY_CHARS: usize = 700;

pub struct ApiDefaults {
    pub model: &'static str,
    pub env: &'static str,
    pub url: &'static str,
}

pub fn api_defaults(api_type: &str) -> Option<ApiDefaults> {
    Some(match api_type {
        "openai" => ApiDefaults { model: "gpt-4o-mini", env: "OPENAI_API_KEY", url: "https://api.openai.com/v1/chat/completions" },
        "anthropic" => ApiDefaults { model: "claude-haiku-5-5", env: "ANTHROPIC_API_KEY", url: "https://api.anthropic.com/v1/messages" },
        "gemini" => ApiDefaults {
            model: "gemini-2.0-flash",
            env: "GEMINI_API_KEY",
            url: "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent",
        },
        "openai_compatible" => ApiDefaults { model: "", env: "AI_API_KEY", url: "" },
        _ => return None,
    })
}

/// O prompt completo segue por stdin (evita limites de tamanho e problemas de aspas no Windows).
fn cli_preset(name: &str) -> Option<Vec<String>> {
    let v: &[&str] = match name {
        "claude" => &["claude", "-p"],
        "gemini" => &["gemini", "-p", "Segue as instruções e os emails recebidos por stdin."],
        "codex" => &["codex", "exec", "-"],
        _ => return None,
    };
    Some(v.iter().map(|s| s.to_string()).collect())
}

pub fn build_prompt(emails: &[Email], moodle_lines: &[String], today: &str) -> String {
    let blocks: String = emails
        .iter()
        .take(MAX_EMAILS)
        .enumerate()
        .map(|(i, e)| {
            format!(
                "--- EMAIL #{} ---\nDe: {}\nData: {}\nAssunto: {}\nConteúdo:\n{}\n\n",
                i + 1,
                e.sender,
                e.received_time,
                e.subject,
                e.body.chars().take(MAX_BODY_CHARS).collect::<String>()
            )
        })
        .collect();
    let moodle = if moodle_lines.is_empty() { "(nenhum ficheiro novo)".to_string() } else { moodle_lines.join("\n") };
    format!(
        "És o assistente de um estudante de Engenharia Informática do ISEP. Hoje é {today}.\n\
Escreve UMA mensagem de resumo para ler no telemóvel (Session), em português de Portugal, texto simples\n\
(sem Markdown, sem tabelas), clara, curta e sem frases cortadas a meio.\n\n\
Regras:\n\
- Agrupa por assunto e ordena por urgência. Secções (omite as vazias), cada uma com um emoji:\n\
  ⏰ PRAZOS (disciplina, o que entregar, data/hora e \"hoje/amanhã/em N dias\"),\n\
  📢 FÓRUNS & AVISOS (disciplina, quem escreveu, o essencial numa frase),\n\
  📥 NOVO NO MOODLE (usa a informação abaixo e diz o que cada ficheiro novo parece ser/exigir),\n\
  🗓 FALTAS, 🛠 HELPDESK, ℹ️ OUTROS.\n\
- Junta emails repetidos (o mesmo aviso em vários fóruns, auto-respostas, vários emails do mesmo pedido).\n\
- Indica sempre a ação concreta que o estudante tem de fazer, se existir.\n\
- Não inventes datas, salas ou factos que não estejam nos emails. Ignora avisos de segurança de login.\n\
- Responde SÓ com a mensagem final, sem introdução nem comentários.\n\n\
NOVIDADES DO MOODLE (ficheiros descarregados):\n{moodle}\n\n\
EMAILS ({n}):\n{blocks}",
        n = emails.len()
    )
}

/// Divide um comando em argumentos respeitando aspas duplas.
pub fn split_command(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in raw.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------- CLI com conta com login

pub fn resolve_cli_command(config: &Json) -> Result<Vec<String>, String> {
    let which_cli = str_or(config, "ai_cli", "claude").to_lowercase();
    if which_cli == "custom" {
        let cmd = match config.get("ai_cli_command") {
            Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
            Some(Value::String(s)) => split_command(s),
            _ => vec![],
        };
        if cmd.is_empty() {
            return Err("ai_cli_command vazio (modo custom).".into());
        }
        return Ok(cmd);
    }
    cli_preset(&which_cli).ok_or_else(|| format!("CLI desconhecida: {which_cli}"))
}

fn run_cli(config: &Json, prompt: &str) -> Result<String, String> {
    let cmd = resolve_cli_command(config)?;
    let exe = which(&cmd[0]).ok_or_else(|| {
        format!("Comando '{}' não encontrado no PATH. Instala-o e faz login uma vez no terminal.", cmd[0])
    })?;
    let mut command = Command::new(&exe);
    command.args(&cmd[1..]);
    let out = run_with_timeout(command, Some(prompt), TIMEOUT).map_err(|e| format!("{}: {e}", cmd[0]))?;
    let text = out.stdout.trim().to_string();
    if out.timed_out {
        return Err(format!("{} excedeu o tempo limite.", cmd[0]));
    }
    if out.status != Some(0) || text.is_empty() {
        let detail = if out.stderr.trim().is_empty() { &text } else { out.stderr.trim() };
        return Err(format!(
            "{} terminou com erro ({}): {}",
            exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            out.status.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
            detail.chars().take(300).collect::<String>().trim()
        ));
    }
    Ok(text)
}

// ---------------------------------------------------------------- API com token

fn http_hint(code: u16) -> &'static str {
    match code {
        401 => "token inválido",
        403 => "sem permissão",
        404 => "modelo/URL inexistente",
        429 => "limite de uso excedido",
        _ => "erro",
    }
}

fn post_json(url: &str, headers: &[(&str, String)], payload: &Value) -> Result<Value, String> {
    let client = HttpClient::new(TIMEOUT).map_err(|e| format!("Erro de rede ao contactar a API ({e})"))?;
    let hs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let resp = client
        .post_json(url, &hs, payload)
        .map_err(|e| format!("Erro de rede ao contactar a API ({})", if e.contains("tempo limite") { "tempo limite" } else { "ligação" }))?;
    if !resp.is_success() {
        // não inclui o corpo da resposta: costuma repetir (parte do) token e iria parar à mensagem do Session
        return Err(format!("HTTP {} da API ({})", resp.status, http_hint(resp.status)));
    }
    serde_json::from_slice::<Value>(&resp.body).map_err(|_| "Resposta inválida da API.".to_string())
}

fn run_api(config: &Json, prompt: &str) -> Result<String, String> {
    let api_type = str_or(config, "ai_api_type", "openai").to_lowercase();
    let d = api_defaults(&api_type).ok_or_else(|| format!("Tipo de API desconhecido: {api_type}"))?;
    let key = {
        let k = str_of(config, "ai_api_key");
        if !k.is_empty() { k.to_string() } else { std::env::var(d.env).unwrap_or_default() }
    };
    if key.is_empty() {
        return Err(format!("Sem token de API (define ai_api_key ou a variável {}).", d.env));
    }
    let model = str_or(config, "ai_model", d.model).to_string();
    let url = str_or(config, "ai_base_url", d.url).to_string();
    if model.is_empty() || url.is_empty() {
        return Err("Falta ai_model/ai_base_url para a API compatível com OpenAI.".into());
    }

    match api_type.as_str() {
        "anthropic" => {
            let data = post_json(
                &url,
                &[("x-api-key", key), ("anthropic-version", "2023-06-01".into())],
                &json!({"model": model, "max_tokens": 1800, "messages": [{"role": "user", "content": prompt}]}),
            )?;
            let text: String = data["content"].as_array().map(|a| a.iter().filter_map(|b| b["text"].as_str()).collect()).unwrap_or_default();
            Ok(text.trim().to_string())
        }
        "gemini" => {
            let data = post_json(
                &url.replace("{model}", &model),
                &[("x-goog-api-key", key)],
                &json!({"contents": [{"parts": [{"text": prompt}]}], "generationConfig": {"temperature": 0.2}}),
            )?;
            let text: String = data["candidates"][0]["content"]["parts"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect())
                .unwrap_or_default();
            Ok(text.trim().to_string())
        }
        _ => {
            let data = post_json(
                &url,
                &[("Authorization", format!("Bearer {key}"))],
                &json!({"model": model, "temperature": 0.2, "messages": [{"role": "user", "content": prompt}]}),
            )?;
            Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string())
        }
    }
}

// ---------------------------------------------------------------- API pública do módulo

pub fn summarize_with_ai(emails: &[Email], config: &Json, moodle_lines: &[String], today: &str) -> Result<String, String> {
    let prompt = build_prompt(emails, moodle_lines, today);
    let provider = str_or(config, "ai_provider", "cli").to_lowercase();
    let text = if provider == "cli" { run_cli(config, &prompt)? } else { run_api(config, &prompt)? };
    if text.chars().count() < 30 {
        return Err("Resposta da IA demasiado curta.".into());
    }
    Ok(text)
}

/// Texto curto com o modo de resumo configurado (para os ecrãs de estado).
pub fn describe(config: &Json) -> String {
    if str_or(config, "summary_mode", "rules") != "ai" {
        return "Regras (local, sem enviar emails a terceiros)".into();
    }
    if str_or(config, "ai_provider", "cli") == "cli" {
        return format!("IA via CLI com login ({})", str_or(config, "ai_cli", "claude"));
    }
    let model = str_or(config, "ai_model", "predefinido");
    format!("IA via API ({}, modelo {})", str_or(config, "ai_api_type", "openai"), model)
}
