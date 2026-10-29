# Servidor local (Rust)

Núcleo e API local do ISEP Assistant em Rust: `isep-assistant-api.exe`.

```bat
isep-assistant-api serve [--port N]     API local (a interface liga-se aqui)
isep-assistant-api digest [--preview]   resumo diário (usado pelo Agendador de Tarefas)
isep-assistant-api sync                 sincroniza o Moodle agora
isep-assistant-api fetch-thunderbird --to <pasta>   (modo antigo) descarrega e verifica o Thunderbird oficial
isep-assistant-api version

--root <pasta>   em qualquer comando: fixa a pasta de dados (é o que o Agendador de Tarefas passa)
```

Mesmo contrato HTTP, mesmos ficheiros de dados (`EmailNotifier/config.json`, `last_run.json`, `history.jsonl`, `MoodleSync/…`) e mesmas credenciais DPAPI da versão Python: as duas implementações são intermutáveis. `tests/test_contract.py` (na raiz) corre o mesmo conjunto de pedidos contra as duas.

## Módulos (`src/`)

| Módulo | Função |
|---|---|
| `api/` | servidor HTTP (`tiny_http`), rotas, segurança (token, `Host`), tarefas em segundo plano |
| `service.rs` | estado, definições, teste de resumo, ficheiros novos, logs |
| `digest.rs` | resumo diário: emails + Moodle → resumo → Session (histórico, `last_run.json`) |
| `summarizer.rs` / `ai.rs` | resumo por regras / por IA (CLI com login ou token de API) |
| `moodle_sync.rs` / `moodle_report.rs` | sincronizador do Moodle e análise dos ficheiros novos |
| `mail.rs` | origens de email (`MailSource`): IMAP da app (predefinido) e Thunderbird (mbox, instalações antigas) |
| `imap.rs` | email da app: IMAP com OAuth2 (XOAUTH2) ou palavra-passe; a ligação corre em `EmailNotifier/mail_fetcher.cjs` com o Electron como Node |
| `oauth.rs` | entrada com a conta da escola (Microsoft 365): código de autorização + PKCE, token de renovação cifrado (DPAPI) |
| `thunderbird.rs` | Thunderbird oficial (modo antigo): descarrega, confere o hash SHA-512 e a assinatura da Mozilla, instala numa pasta da app |
| `institutions.rs` / `rules.rs` | catálogo de instituições e Moodles; regras editáveis do resumo |
| `progress.rs` | progresso da sincronização do Moodle (`sync_progress.json`) |
| `views.rs` | dados das páginas: resumo, emails, ficheiros do Moodle, logs de 7 dias |
| `http.rs` | cliente HTTP sobre a **WinHTTP** do Windows (TLS e proxy do sistema, cookies) |
| `config.rs`, `history.rs`, `session.rs`, `system.rs`, `dpapi.rs`, `paths.rs`, `util.rs` | o resto |

## Limitações face à versão Python

- Não lê o texto dos **PDFs** novos para o resumo (a análise usa o nome e `.txt`/`.md`).
- Os assistentes de instalação em linha de comandos (Thunderbird, bot do Session) continuam em Python; na app, o Session e o email configuram-se no assistente.

## Porque estas dependências (e versões fixas)

O toolchain `stable-x86_64-pc-windows-gnu` do `rustup` **não traz o `dlltool`** do MinGW. As bibliotecas que usam
`raw-dylib` (`windows-link`, `windows-sys` ≥ 0.60, `getrandom` 0.3…) precisam dele para compilar. Para o projeto compilar
só com o `rustup`, sem instalar mais nada:

- sem `tokio`/`reqwest`/`rustls`: servidor síncrono (`tiny_http`) e cliente sobre a WinHTTP (`windows-sys` **0.59**);
- versões fixas no `Cargo.lock`: `chrono 0.4.39`, `parking_lot 0.12.3` / `parking_lot_core 0.9.10`, `getrandom 0.2.15`, `ahash 0.8.11`.

**Não faças `cargo update` sem testar**: uma dependência nova que use `raw-dylib` volta a falhar com
`error calling dlltool 'dlltool.exe': program not found`. Com um MinGW completo ou o Visual Studio com C++ instalados
(e `rustup default stable-x86_64-pc-windows-msvc`) estes limites deixam de ser necessários.

## Testes

```bat
cargo test
```

Testes de integração em `tests/` com servidores HTTP falsos locais (IA, Moodle) e raiz de dados temporária. Definem
`ISEP_FAKE_SCHEDULER=1` para nunca tocarem no Agendador de Tarefas real e usam pastas temporárias para os downloads.
Variáveis só para desenvolvimento/testes: `ISEP_ASSISTANT_ROOT`, `ISEP_ASSISTANT_STATE`, `ISEP_MOODLE_BASE_URL`,
`ISEP_THUNDERBIRD_PROFILES`, `ISEP_FAKE_SCHEDULER`, `ISEP_MS_LOGIN_URL`, `ISEP_IMAP_HOST` / `ISEP_IMAP_PORT` / `ISEP_IMAP_PLAIN`,
`ISEP_NODE`, `ISEP_MAIL_SCRIPT`, `ISEP_TB_DIR` e `ISEP_TB_*`.
