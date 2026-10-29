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

### Só no servidor Rust (páginas da app e configuração)

| Método | Caminho | O que faz |
|---|---|---|
| GET | `/overview?days=7\|15\|30` | Resumo de hoje (`today`), o mais recente (`latest`), os resumos do período (`items`) e totais. |
| GET | `/emails?days=&limit=` | Emails da origem configurada (`source`); se não estiver pronta, `error` explica porquê. |
| GET | `/moodle/files?limit=` | Progresso da sincronização (`running`, `percent`, `counts`, `items` por estado) e ficheiros já descarregados. |
| GET | `/activity?days=` | Histórico e logs dos últimos dias (máx. 7). |
| GET | `/setup` | Passos do assistente de configuração e se está tudo pronto. |
| GET | `/session` | Estado do Session: bot, destinatários, Node.js. A frase secreta nunca é devolvida. |
| POST | `/session/bot` | Cria o bot do Session (`{force}` para trocar o existente). |
| POST | `/session/pair` | **Tarefa**: espera a primeira mensagem ao bot e guarda quem a enviou como destinatário. |
| POST | `/session/test` | **Tarefa**: envia uma mensagem de teste. |
| POST, DELETE | `/session/recipients`, `/session/recipients/{índice}` | Adiciona (`{session_id}`) ou remove um destinatário. |
| POST | `/email/oauth/start` | `{email}`: devolve `auth_url` (página de entrada da Microsoft, com PKCE) e `redirect_uri` (`https://localhost`). |
| POST | `/email/oauth/finish` | `{email, redirect_url}`: valida o estado, troca o código e guarda o token de renovação cifrado. Devolve `{ok}` ou `{ok:false, error}`. |
| POST | `/email/test` | Lê os últimos 3 dias com o que está guardado: `{ok, count}` ou `{ok:false, error}`. |
| POST | `/email/connect/start`, `/email/connect/poll` | Ligação por código de dispositivo (mantida, mas muitas escolas bloqueiam-na). |
| GET | `/thunderbird` | (modo antigo) estado do Thunderbird da app. `POST /thunderbird/install` (tarefa), `/thunderbird/open` e `/thunderbird/account`. |

As definições ganham `setup.done` e, em `email`: `connected`, `password_set`, `oauth_connected`, `imap_host` e `imap_port`. Editáveis: `email.method` (`imap` ou `thunderbird`), `email.address`, `email.imap_host`, `email.imap_port`, `email.password` (só de escrita, guardada cifrada), `email.clear_password`, `email.sign_out` e `setup.done`.

### Tarefas

`POST` em `/digest/*`, `/moodle/sync`, `/session/pair` e `/session/test` respondem `202` com `{id, status: "running", …}`. Fazer *polling* de `GET /jobs/{id}` até `status` ser `done` ou `failed`; `progress` traz as linhas de log (use `since_line` para receber só as novas) e `result` o resultado. Só corre **uma tarefa de cada vez**: outro pedido enquanto há uma ativa devolve `409` com `active_job_id`.

### Exemplo de definições

```json
{
  "summary":     { "mode": "ai", "ai_provider": "api", "ai_cli": "claude", "ai_cli_command": "",
                   "ai_api_type": "openai", "ai_model": "gpt-4o-mini", "ai_base_url": "",
                   "ai_api_key_set": true, "ai_api_key_env": "OPENAI_API_KEY", "ai_api_key_env_set": false },
  "schedule":    { "time": "18:00" },
  "destination": { "session_recipients": ["05ab12…cd34"], "session_configured": true },
  "email":       { "method": "imap", "address": "numero@isep.ipp.pt", "connected": true, "password_set": false,
                   "oauth_connected": true, "imap_host": "outlook.office365.com", "imap_port": 993 },
  "moodle":      { "sync_dir": "…", "download_dir": "…" }
}
```

Para alterar, envia só o que muda. Campos só de escrita: `summary.ai_api_key` (define), `summary.clear_ai_api_key: true` (apaga), `destination.session_id` (define o destinatário).
