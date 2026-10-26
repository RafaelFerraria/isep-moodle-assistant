"""Resumo com IA: usa uma CLI com a tua conta (Claude, Gemini, Codex…) ou um token de API.

Modos (config.json):
  "ai_provider": "cli"  -> "ai_cli": "claude" | "gemini" | "codex" | "custom" (+ "ai_cli_command")
  "ai_provider": "api"  -> "ai_api_type": "openai" | "anthropic" | "gemini" | "openai_compatible"
                           + "ai_api_key" (ou variável de ambiente), "ai_model", "ai_base_url"
"""
import os
import json
import shlex
import shutil
import subprocess
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional

CLI_PRESETS = {
    # O prompt completo segue por stdin (evita limites de tamanho e problemas de aspas no Windows)
    "claude": ["claude", "-p"],
    "gemini": ["gemini", "-p", "Segue as instruções e os emails recebidos por stdin."],
    "codex": ["codex", "exec", "-"],
}

API_DEFAULTS = {
    "openai": {"model": "gpt-4o-mini", "env": "OPENAI_API_KEY", "url": "https://api.openai.com/v1/chat/completions"},
    "anthropic": {"model": "claude-haiku-5-5", "env": "ANTHROPIC_API_KEY", "url": "https://api.anthropic.com/v1/messages"},
    "gemini": {"model": "gemini-2.0-flash", "env": "GEMINI_API_KEY",
               "url": "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"},
    "openai_compatible": {"model": "", "env": "AI_API_KEY", "url": ""},
}

TIMEOUT_SECONDS = 180
MAX_EMAILS = 30
MAX_BODY_CHARS = 700


def build_prompt(emails: List[Dict[str, Any]], moodle_lines: Optional[List[str]], today: str) -> str:
    blocks = []
    for idx, e in enumerate(emails[:MAX_EMAILS], 1):
        blocks.append(
            f"--- EMAIL #{idx} ---\nDe: {e.get('sender', '')}\nData: {e.get('received_time', '')}\n"
            f"Assunto: {e.get('subject', '')}\nConteúdo:\n{(e.get('body') or '')[:MAX_BODY_CHARS]}\n")
    moodle = "\n".join(moodle_lines) if moodle_lines else "(nenhum ficheiro novo)"
    return f"""És o assistente de um estudante de Engenharia Informática do ISEP. Hoje é {today}.
Escreve UMA mensagem de resumo para ler no telemóvel (Session), em português de Portugal, texto simples
(sem Markdown, sem tabelas), clara, curta e sem frases cortadas a meio.

Regras:
- Agrupa por assunto e ordena por urgência. Secções (omite as vazias), cada uma com um emoji:
  ⏰ PRAZOS (disciplina, o que entregar, data/hora e "hoje/amanhã/em N dias"),
  📢 FÓRUNS & AVISOS (disciplina, quem escreveu, o essencial numa frase),
  📥 NOVO NO MOODLE (usa a informação abaixo e diz o que cada ficheiro novo parece ser/exigir),
  🗓 FALTAS, 🛠 HELPDESK, ℹ️ OUTROS.
- Junta emails repetidos (o mesmo aviso em vários fóruns, auto-respostas, vários emails do mesmo pedido).
- Indica sempre a ação concreta que o estudante tem de fazer, se existir.
- Não inventes datas, salas ou factos que não estejam nos emails. Ignora avisos de segurança de login.
- Responde SÓ com a mensagem final, sem introdução nem comentários.

NOVIDADES DO MOODLE (ficheiros descarregados):
{moodle}

EMAILS ({len(emails)}):
{chr(10).join(blocks)}"""


# ---------------------------------------------------------------- CLI com conta com login

def resolve_cli_command(config: Dict[str, Any]) -> List[str]:
    which = (config.get("ai_cli") or "claude").lower()
    if which == "custom":
        raw = config.get("ai_cli_command") or ""
        cmd = raw if isinstance(raw, list) else shlex.split(raw, posix=False)
        if not cmd:
            raise RuntimeError("ai_cli_command vazio (modo custom).")
        return cmd
    if which not in CLI_PRESETS:
        raise RuntimeError(f"CLI desconhecida: {which}")
    return list(CLI_PRESETS[which])


def run_cli(config: Dict[str, Any], prompt: str) -> str:
    cmd = resolve_cli_command(config)
    exe = shutil.which(cmd[0])
    if not exe:
        raise RuntimeError(f"Comando '{cmd[0]}' não encontrado no PATH. Instala-o e faz login uma vez no terminal.")
    cmd[0] = exe
    res = subprocess.run(cmd, input=prompt, capture_output=True, text=True, encoding="utf-8",
                         errors="replace", timeout=TIMEOUT_SECONDS)
    out = (res.stdout or "").strip()
    if res.returncode != 0 or not out:
        raise RuntimeError(f"{os.path.basename(cmd[0])} terminou com erro ({res.returncode}): "
                           f"{(res.stderr or out)[:300].strip()}")
    return out


# ---------------------------------------------------------------- API com token

def _post_json(url: str, headers: Dict[str, str], payload: Dict[str, Any]) -> Dict[str, Any]:
    req = urllib.request.Request(url, data=json.dumps(payload).encode("utf-8"),
                                 headers={"Content-Type": "application/json", **headers}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_SECONDS) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as err:
        # não inclui o corpo da resposta: costuma repetir (parte do) token e iria parar à mensagem do Session
        hint = {401: "token inválido", 403: "sem permissão", 404: "modelo/URL inexistente", 429: "limite de uso excedido"}
        raise RuntimeError(f"HTTP {err.code} da API ({hint.get(err.code, 'erro')})") from None


def run_api(config: Dict[str, Any], prompt: str) -> str:
    api_type = (config.get("ai_api_type") or "openai").lower()
    if api_type not in API_DEFAULTS:
        raise RuntimeError(f"Tipo de API desconhecido: {api_type}")
    d = API_DEFAULTS[api_type]
    key = config.get("ai_api_key") or os.environ.get(d["env"], "")
    if not key:
        raise RuntimeError(f"Sem token de API (define ai_api_key ou a variável {d['env']}).")
    model = config.get("ai_model") or d["model"]
    url = config.get("ai_base_url") or d["url"]
    if not model or not url:
        raise RuntimeError("Falta ai_model/ai_base_url para a API compatível com OpenAI.")

    if api_type == "anthropic":
        data = _post_json(url, {"x-api-key": key, "anthropic-version": "2023-06-01"},
                          {"model": model, "max_tokens": 1800, "messages": [{"role": "user", "content": prompt}]})
        return "".join(b.get("text", "") for b in data.get("content", [])).strip()
    if api_type == "gemini":
        data = _post_json(url.format(model=model), {"x-goog-api-key": key},
                          {"contents": [{"parts": [{"text": prompt}]}], "generationConfig": {"temperature": 0.2}})
        parts = data["candidates"][0]["content"]["parts"]
        return "".join(p.get("text", "") for p in parts).strip()
    data = _post_json(url, {"Authorization": f"Bearer {key}"},
                      {"model": model, "temperature": 0.2, "messages": [{"role": "user", "content": prompt}]})
    return data["choices"][0]["message"]["content"].strip()


# ---------------------------------------------------------------- API pública do módulo

def summarize_with_ai(emails: List[Dict[str, Any]], config: Dict[str, Any],
                      moodle_lines: Optional[List[str]], today: str) -> str:
    prompt = build_prompt(emails, moodle_lines, today)
    provider = (config.get("ai_provider") or "cli").lower()
    text = run_cli(config, prompt) if provider == "cli" else run_api(config, prompt)
    if len(text) < 30:
        raise RuntimeError("Resposta da IA demasiado curta.")
    return text


def describe(config: Dict[str, Any]) -> str:
    """Texto curto com o modo de resumo configurado (para o ecrã final dos assistentes)."""
    if (config.get("summary_mode") or "rules") != "ai":
        return "Regras (local, sem enviar emails a terceiros)"
    if (config.get("ai_provider") or "cli") == "cli":
        return f"IA via CLI com login ({config.get('ai_cli', 'claude')})"
    return f"IA via API ({config.get('ai_api_type', 'openai')}, modelo {config.get('ai_model') or 'predefinido'})"
