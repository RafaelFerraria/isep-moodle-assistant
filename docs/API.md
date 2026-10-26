# API local

A interface gráfica fala com o núcleo através de uma API HTTP local. O mesmo contrato servirá, mais tarde, a versão na nuvem: só muda o endereço.

Há **duas implementações** do contrato — Rust (`server/`, a usada pela app) e Python (`isep_assistant/`, referência) — e `tests/test_contract.py` corre o mesmo conjunto de pedidos contra as duas.

## Arrancar

```bat
server\target\release\isep-assistant-api.exe serve            :: Rust; porta livre escolhida pelo sistema
server\target\release\isep-assistant-api.exe serve --port 8765

python -m isep_assistant.api.server                           :: Python (equivalente)
python -m isep_assistant.api.server --port 8765
```

O servidor escuta **só em `127.0.0.1`** e escreve a porta e o token em `%APPDATA%\ISEPAssistant\api.json`:

```json
{ "port": 51234, "token": "…", "pid": 4321 }
```

A UI deve ler este ficheiro, confirmar que o servidor responde em `/api/v1/health` (o ficheiro pode ser de um processo que já terminou) e enviar `Authorization: Bearer <token>` em todos os pedidos. O token muda em cada arranque.

## Segurança

- **Token obrigatório** (exceto `/health`), comparado em tempo constante.
- **Cabeçalho `Host` validado** (`localhost`, `127.0.0.1`, `[::1]`): bloqueia DNS rebinding.
- **Sem CORS**: páginas web noutras origens não conseguem ler as respostas.
- **Segredos nunca saem**: a mnemónica do Session, o token de API da IA e a palavra-passe do Moodle só podem ser definidos, nunca lidos. Os endpoints devolvem apenas `*_set: true/false`.
- Mensagens de erro da IA/rede não incluem o corpo das respostas (podem repetir parte do token).

## Endpoints (`/api/v1`)

| Método | Caminho | Descrição |
|---|---|---|
| GET | `/health` | `{ok, version}`. Público. |
| GET | `/status` | Verificações (`checks[]`), modo de resumo, última/próxima execução, Moodle, tarefas agendadas, `active_job`. |
| GET | `/settings` | Definições sem segredos (resumo, hora, destino, email, Moodle). |
| PUT | `/settings` | Alteração parcial, mesma forma que o GET. `422` com `errors{campo: mensagem}` se inválido. Se a hora mudar, volta a registar as tarefas do Windows (`effects`, `reschedule`). |
| POST | `/summary/test` | Gera um resumo de exemplo com o modo atual (testa a CLI/token de IA). |
| POST | `/schedule/register` | Volta a registar as tarefas do Agendador do Windows. |
| PUT | `/moodle/credentials` | `{username, password}`: valida o login e guarda a palavra-passe cifrada (DPAPI). |
| POST | `/digest/preview` | **Tarefa**: gera o resumo sem enviar nem avançar o estado. |
| POST | `/digest/run` | **Tarefa**: gera e envia o resumo. |
| POST | `/moodle/sync` | **Tarefa**: sincroniza o Moodle. |
| GET | `/jobs`, `/jobs/{id}?since_line=N` | Estado e progresso das tarefas. |
| GET | `/history?kind=&limit=&offset=` | Execuções (mais recentes primeiro, sem o texto do resumo). |
| GET | `/history/{id}` | Execução completa, com o resumo enviado. |
| GET | `/moodle/new-files?limit=` | Ficheiros novos/atualizados no Moodle. |
| GET | `/logs/{notifier\|sync}?lines=` | Fim dos ficheiros de log. |

### Tarefas

`POST` em `/digest/*` e `/moodle/sync` respondem `202` com `{id, status: "running", …}`. Fazer *polling* de `GET /jobs/{id}` até `status` ser `done` ou `failed`; `progress` traz as linhas de log (use `since_line` para receber só as novas) e `result` o resultado. Só corre **uma tarefa de cada vez**: outro pedido enquanto há uma ativa devolve `409` com `active_job_id`.

### Exemplo de definições

```json
{
  "summary":     { "mode": "ai", "ai_provider": "api", "ai_cli": "claude", "ai_cli_command": "",
                   "ai_api_type": "openai", "ai_model": "gpt-4o-mini", "ai_base_url": "",
                   "ai_api_key_set": true, "ai_api_key_env": "OPENAI_API_KEY", "ai_api_key_env_set": false },
  "schedule":    { "time": "18:00" },
  "destination": { "session_recipients": ["05ab12…cd34"], "session_configured": true },
  "email":       { "method": "thunderbird", "address": "numero@isep.ipp.pt" },
  "moodle":      { "sync_dir": "…", "download_dir": "…" }
}
```

Para alterar, envia só o que muda. Campos só de escrita: `summary.ai_api_key` (define), `summary.clear_ai_api_key: true` (apaga), `destination.session_id` (define o destinatário).
