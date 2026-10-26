import http.server
import json
import sys
import threading

import pytest

from conftest import mail
from isep_assistant import ai_summarizer as ai

EMAILS = [mail("Aula de ESINF", "docente@isep.ipp.pt", "Sala F105 às 10h.")]


@pytest.fixture
def fake_api():
    """Servidor HTTP local que imita OpenAI, Anthropic e Gemini e guarda os pedidos recebidos."""
    seen = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        fail_with = None

        def do_POST(self):
            payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            seen[self.path] = {"headers": {k.lower(): v for k, v in self.headers.items()}, "body": payload}
            if Handler.fail_with:
                data = json.dumps({"error": {"message": "Incorrect API key provided: sk-SEGREDO123456"}}).encode()
                self.send_response(Handler.fail_with)
            else:
                if "anthropic" in self.path:
                    out = {"content": [{"type": "text", "text": "RESPOSTA-ANTHROPIC " + "x" * 40}]}
                elif "generateContent" in self.path:
                    out = {"candidates": [{"content": {"parts": [{"text": "RESPOSTA-GEMINI " + "x" * 40}]}}]}
                else:
                    out = {"choices": [{"message": {"content": "RESPOSTA-OPENAI " + "x" * 40}}]}
                data = json.dumps(out).encode()
                self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *a):
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{server.server_port}", seen, Handler
    server.shutdown()


def run(cfg):
    return ai.summarize_with_ai(EMAILS, cfg, ["📥 NOVO NO MOODLE (1 ficheiro(s))"], "seg 26/10/2026 19:00")


def test_prompt_contains_emails_moodle_and_rules():
    prompt = ai.build_prompt(EMAILS, ["📥 NOVO NO MOODLE (1 ficheiro(s))"], "seg 26/10")
    assert "Sala F105" in prompt and "NOVO NO MOODLE" in prompt
    assert "Não inventes" in prompt and "seg 26/10" in prompt


def test_prompt_limits_amount_of_email_text():
    many = [mail(f"Assunto {i}", "x", "y" * 5000) for i in range(100)]
    prompt = ai.build_prompt(many, None, "hoje")
    assert prompt.count("--- EMAIL #") == ai.MAX_EMAILS
    assert len(prompt) < ai.MAX_EMAILS * (ai.MAX_BODY_CHARS + 200) + 3000


def test_openai_compatible_sends_bearer_and_model(fake_api):
    base, seen, _ = fake_api
    out = run({"ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K", "ai_model": "m1",
               "ai_base_url": base + "/v1/chat/completions"})
    assert out.startswith("RESPOSTA-OPENAI")
    req = seen["/v1/chat/completions"]
    assert req["headers"]["authorization"] == "Bearer K" and req["body"]["model"] == "m1"
    assert "Sala F105" in req["body"]["messages"][0]["content"]


def test_anthropic_uses_x_api_key_header(fake_api, monkeypatch):
    base, seen, _ = fake_api
    monkeypatch.setitem(ai.API_DEFAULTS["anthropic"], "url", base + "/anthropic/v1/messages")
    out = run({"ai_provider": "api", "ai_api_type": "anthropic", "ai_api_key": "AK"})
    assert out.startswith("RESPOSTA-ANTHROPIC")
    req = seen["/anthropic/v1/messages"]
    assert req["headers"]["x-api-key"] == "AK" and req["headers"]["anthropic-version"]
    assert req["body"]["model"] == ai.API_DEFAULTS["anthropic"]["model"]


def test_gemini_uses_header_not_query_string(fake_api, monkeypatch):
    base, seen, _ = fake_api
    monkeypatch.setitem(ai.API_DEFAULTS["gemini"], "url", base + "/models/{model}:generateContent")
    out = run({"ai_provider": "api", "ai_api_type": "gemini", "ai_api_key": "GK", "ai_model": "g-test"})
    assert out.startswith("RESPOSTA-GEMINI")
    path = "/models/g-test:generateContent"
    assert seen[path]["headers"]["x-goog-api-key"] == "GK"


def test_api_key_from_environment(fake_api, monkeypatch):
    base, seen, _ = fake_api
    monkeypatch.setenv("OPENAI_API_KEY", "ENVKEY")
    monkeypatch.setitem(ai.API_DEFAULTS["openai"], "url", base + "/v1/chat/completions")
    run({"ai_provider": "api", "ai_api_type": "openai"})
    assert seen["/v1/chat/completions"]["headers"]["authorization"] == "Bearer ENVKEY"


def test_missing_key_is_a_clear_error(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    with pytest.raises(RuntimeError, match="OPENAI_API_KEY"):
        run({"ai_provider": "api", "ai_api_type": "openai"})


def test_http_error_never_leaks_response_body(fake_api):
    base, _, handler = fake_api
    handler.fail_with = 401
    try:
        with pytest.raises(RuntimeError) as err:
            run({"ai_provider": "api", "ai_api_type": "openai_compatible", "ai_api_key": "K", "ai_model": "m",
                 "ai_base_url": base + "/v1/chat/completions"})
    finally:
        handler.fail_with = None
    assert "401" in str(err.value) and "token inválido" in str(err.value)
    assert "SEGREDO" not in str(err.value)


def test_cli_receives_prompt_on_stdin():
    script = "import sys; d = sys.stdin.read(); print('CLI recebeu', len(d), 'caracteres; sala:', 'F105' in d)"
    out = run({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": [sys.executable, "-c", script]})
    assert "sala: True" in out


def test_cli_custom_command_string_is_split():
    cfg = {"ai_cli": "custom", "ai_cli_command": 'meu-cli --flag "com espaço"'}
    assert ai.resolve_cli_command(cfg)[:2] == ["meu-cli", "--flag"]


def test_cli_presets_and_errors():
    assert ai.resolve_cli_command({"ai_cli": "claude"}) == ["claude", "-p"]
    assert ai.resolve_cli_command({"ai_cli": "codex"})[:2] == ["codex", "exec"]
    with pytest.raises(RuntimeError, match="desconhecida"):
        ai.resolve_cli_command({"ai_cli": "inexistente"})
    with pytest.raises(RuntimeError, match="vazio"):
        ai.resolve_cli_command({"ai_cli": "custom", "ai_cli_command": ""})
    with pytest.raises(RuntimeError, match="não encontrado"):
        run({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": ["comando-que-nao-existe-xyz"]})


def test_cli_failure_reports_exit_code():
    with pytest.raises(RuntimeError, match="erro"):
        run({"ai_provider": "cli", "ai_cli": "custom",
             "ai_cli_command": [sys.executable, "-c", "import sys; print('x', file=sys.stderr); sys.exit(3)"]})


def test_too_short_answer_is_rejected():
    with pytest.raises(RuntimeError, match="curta"):
        run({"ai_provider": "cli", "ai_cli": "custom", "ai_cli_command": [sys.executable, "-c", "print('ok')"]})


def test_describe():
    assert "Regras" in ai.describe({})
    assert "claude" in ai.describe({"summary_mode": "ai", "ai_provider": "cli", "ai_cli": "claude"})
    assert "openai" in ai.describe({"summary_mode": "ai", "ai_provider": "api", "ai_api_type": "openai"})
