"""Assistente do modo de resumo: regras (local) ou IA (CLI com login / token de API).

Usado na instalação (setup_master.py) e, sozinho, para reconfigurar mais tarde:
    configurar_resumo.bat
"""
import os
import sys
import json
import shutil
import datetime

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ROOT_DIR_BOOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR_BOOT not in sys.path:
    sys.path.insert(0, ROOT_DIR_BOOT)
from isep_assistant.ai_summarizer import API_DEFAULTS, CLI_PRESETS, describe
from isep_assistant.summarizer import summarize_emails, SAMPLE_EMAILS

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE = os.path.join(BASE_DIR, "config.json")

CLI_LABELS = {
    "claude": "Claude Code (conta Claude)        — comando 'claude'",
    "gemini": "Gemini CLI / Antigravity (conta Google) — comando 'gemini'",
    "codex": "Codex CLI (conta ChatGPT/OpenAI)   — comando 'codex'",
    "custom": "Outro comando (indicas tu)",
}
API_LABELS = {
    "openai": "OpenAI (ChatGPT API)",
    "anthropic": "Anthropic (Claude API)",
    "gemini": "Google (Gemini API)",
    "openai_compatible": "Outra API compatível com OpenAI (URL à escolha)",
}

def ask_choice(title: str, options: dict, default: str) -> str:
    keys = list(options)
    print(f"\n{title}")
    for i, k in enumerate(keys, 1):
        print(f"  {i}) {options[k]}" + ("   [atual]" if k == default else ""))
    while True:
        raw = input(f"Escolha [1-{len(keys)}] (ENTER = manter): ").strip()
        if not raw:
            return default if default in keys else keys[0]
        if raw.isdigit() and 1 <= int(raw) <= len(keys):
            return keys[int(raw) - 1]
        print("⚠️ Opção inválida.")


def test_summary(config: dict) -> bool:
    """Gera um resumo de teste com a configuração atual; devolve True se a IA respondeu mesmo."""
    print("\n⏳ A testar (pode demorar até ~1 minuto)...")
    out = summarize_emails(SAMPLE_EMAILS, config)
    failed = "IA indisponível" in out
    print("\n" + "-" * 50 + f"\n{out}\n" + "-" * 50)
    return not failed


def configure_summary(config: dict) -> dict:
    print("\n" + "=" * 60)
    print("   MODO DE RESUMO")
    print("=" * 60)
    print("  1) Regras — local e privado: os emails nunca saem do teu PC.")
    print("     Agrupa por assunto, ordena prazos e limpa o texto.")
    print("  2) IA — um modelo reescreve o resumo (mais natural, junta e interpreta melhor).")
    print("     ⚠️ Os emails (assunto e início do texto) são enviados ao fornecedor da IA.")

    mode = ask_choice("Que modo queres usar?", {"rules": "Regras (local)", "ai": "IA (CLI com login ou token de API)"},
                      (config.get("summary_mode") or "rules"))
    config["summary_mode"] = mode
    if mode == "rules":
        print("✅ Modo 'Regras' selecionado.")
        return config

    provider = ask_choice("Como queres ligar à IA?", {
        "cli": "Conta com login numa CLI instalada (Claude / Gemini-Antigravity / Codex) — sem token",
        "api": "Token de API (OpenAI, Anthropic, Google ou compatível)",
    }, config.get("ai_provider") or "cli")
    config["ai_provider"] = provider

    if provider == "cli":
        which = ask_choice("Que CLI queres usar?", CLI_LABELS, config.get("ai_cli") or "claude")
        config["ai_cli"] = which
        if which == "custom":
            print("\nIndica o comando completo. O prompt é enviado por stdin (ex.: claude -p  |  codex exec -).")
            cur = config.get("ai_cli_command") or ""
            cmd = input(f"Comando [{cur}]: ").strip() or cur
            config["ai_cli_command"] = cmd
            exe = cmd.split()[0] if cmd else ""
        else:
            exe = CLI_PRESETS[which][0]
        if exe and not shutil.which(exe):
            print(f"\n⚠️ O comando '{exe}' não foi encontrado. Instala-o e faz login uma vez no terminal")
            print("   (corre o comando e segue o login da conta). Podes continuar e testar mais tarde.")
    else:
        api = ask_choice("Que API?", API_LABELS, config.get("ai_api_type") or "openai")
        config["ai_api_type"] = api
        d = API_DEFAULTS[api]
        if api == "openai_compatible":
            config["ai_base_url"] = input(f"URL do endpoint /chat/completions [{config.get('ai_base_url', '')}]: ").strip() \
                or config.get("ai_base_url", "")
        model_default = config.get("ai_model") or d["model"]
        config["ai_model"] = input(f"Modelo [{model_default}]: ").strip() or model_default
        print(f"\nO token pode vir da variável de ambiente {d['env']} (recomendado) ou ficar guardado em config.json")
        print("(ficheiro local, ignorado pelo git).")
        have_env = bool(os.environ.get(d["env"]))
        if have_env:
            print(f"✅ Variável {d['env']} detetada — não é preciso guardar o token.")
        import getpass
        key = getpass.getpass("Token de API (ENTER para manter/usar a variável de ambiente): ").strip()
        if key:
            config["ai_api_key"] = key

    if input("\nQueres testar agora com um resumo de exemplo? (S/n): ").strip().lower() in ("", "s", "sim", "y"):
        if not test_summary(config):
            print("\n⚠️ A IA não respondeu. Se continuares, o resumo diário cai para o modo 'Regras' quando falhar.")
            if input("Voltar ao modo 'Regras' agora? (s/N): ").strip().lower() in ("s", "sim", "y"):
                config["summary_mode"] = "rules"
    print(f"\n✅ Modo de resumo: {describe(config)}")
    return config


def main():
    config = {}
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, "r", encoding="utf-8-sig") as f:
            config = json.load(f)
    else:
        print("config.json não encontrado — corre primeiro o CONFIGURAR_TUDO.bat.")
        return
    configure_summary(config)
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
    print("Configuração guardada. Aplica-se ao próximo resumo diário.")


if __name__ == "__main__":
    main()
