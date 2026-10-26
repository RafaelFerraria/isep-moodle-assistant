# ISEP Moodle Assistant

> Automatic Moodle downloads + a daily, easy-to-read email digest delivered to your phone via [Session](https://getsession.org). Built for ISEP students (Windows).

Dois pequenos programas que trabalham em conjunto e correm sozinhos todos os dias:

| Módulo | O que faz |
|---|---|
| **`MoodleSync/`** | Entra no Moodle do ISEP, descobre as tuas disciplinas e descarrega, de forma incremental, ficheiros, pastas, enunciados de tarefas e páginas. Guarda um registo (`new_files.jsonl`) do que é novo ou foi atualizado. |
| **`EmailNotifier/`** | Lê os emails da faculdade (Thunderbird), **organiza-os por assunto** e envia um resumo claro para o teu Session. Inclui uma secção com o que **foi colocado de novo no Moodle**, já analisado. |

## Como é o resumo

Em vez de despejar frases cortadas dos emails, o resumo agrupa e interpreta:

```
📬 RESUMO ISEP — seg 26/10 18:00
18 emails novos

────────────────────────
⏰ PRAZOS
🔴 Estruturas de Informação: First Sprint Submission (due by 25 October)
   O prazo terminou dom 25/10 20:00. Ainda podes entregar até dom 25/10 23:59 (fica como atrasado).
🟠 Laboratório / Projeto III: 3 entregas: 2DO-2DP-C - SPRINT 1 - Status Report; …
   Entrega: HOJE 23:59

────────────────────────
📢 AVISOS DOS DOCENTES
• LAPR3 / SEM_3_PI — Requisitos para o Sprint 2
  Nome do Docente: “Os requisitos para o Sprint 2 já estão publicados…”

────────────────────────
📥 NOVO NO MOODLE (2 ficheiro(s))
• Laboratório / Projeto III
  – Enunciado Sprint 2.pdf [enunciado, novo]
    menciona prazo: “entrega até 8 de novembro”

────────────────────────
🗓 FALTAS
• Semana 18/10–24/10: 4 aula(s) com falta — AMATA ×2, LAPR3 ×1, ARQCP ×1

────────────────────────
🛠 HELPDESK
• #123456 Cartão de estudante – cartão definitivo atribuído (pedido fechado)
```

- **Prazos** ordenados por urgência, com "HOJE / AMANHÃ / em N dias" e entregas iguais agrupadas.
- **Avisos dos docentes** deduplicados (o mesmo aviso chega ao fórum geral e ao da turma).
- **Novo no Moodle**: tipo de cada ficheiro (enunciado, ficha, slides, avaliação…) e, se tiver `pypdf`, uma pista do conteúdo (prazo mencionado ou primeira linha).
- **Faltas**, **Helpdesk** (um item por pedido, não por email) e **outros**.

## Dois modos de resumo

Escolhes na instalação e podes mudar quando quiseres com `EmailNotifier\configurar_resumo.bat`.

| Modo | Como funciona | Privacidade |
|---|---|---|
| **Regras** (predefinido) | Classifica e agrupa os emails localmente (prazos, avisos, faltas, helpdesk…). Sem dependências nem custos. | Os emails não saem do teu PC. |
| **IA** | Um modelo reescreve o resumo de forma mais natural e junta/interpreta melhor os emails. | O assunto e o início do texto dos emails são enviados ao fornecedor escolhido. |

No modo **IA** ligas o modelo de uma de duas formas:

- **CLI com a tua conta (sem token):** usa uma ferramenta de linha de comandos onde já fizeste login — `claude` (Claude Code), `gemini` (Gemini CLI / Antigravity, conta Google), `codex` (conta ChatGPT/OpenAI) — ou qualquer outro comando que leia o prompt por *stdin* (modo `custom`).
- **Token de API:** OpenAI, Anthropic, Google Gemini ou qualquer API compatível com OpenAI. O token pode ficar numa variável de ambiente (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, recomendado) ou em `config.json` (ficheiro local, fora do git).

Se a IA falhar (sem rede, login expirado, token inválido, limite de uso), o resumo diário **não deixa de ser enviado**: cai para o modo Regras e avisa no fim da mensagem. As mensagens de erro nunca incluem o token.

## Como funciona

```
        ┌──────────────┐  new_files.jsonl   ┌────────────────┐
Moodle ─►  MoodleSync  ├───────────────────►│ EmailNotifier  │──► Session (telemóvel)
        └──────────────┘                    │  (resumo 18h)  │
Thunderbird (emails ISEP) ─────────────────►└────────────────┘
```

1. `MoodleSync` corre ao iniciar sessão no Windows, espera pelas 18h e sincroniza uma vez por dia (com espera pela rede, uma disciplina com erro a não bloquear as outras e *lock* contra execuções em paralelo).
2. O `EmailNotifier` corre às 18h (Agendador de Tarefas), garante que o Moodle foi sincronizado, lê os emails novos (com deduplicação por `Message-ID`) e envia o resumo.

## Requisitos

- Windows 10/11 (usa DPAPI para guardar a palavra-passe e o Agendador de Tarefas)
- Python 3.9+ e Node.js 18+ (o `CONFIGURAR_TUDO.bat` instala-os se faltarem)
- [Mozilla Thunderbird](https://www.thunderbird.net) com a conta `@isep.ipp.pt` configurada
- Uma conta [Session](https://getsession.org) para receber o resumo
- *(Opcional, modo IA)* uma CLI com login (`claude`, `gemini`, `codex`) ou um token de API

## Instalação

```bat
git clone https://github.com/<o-teu-utilizador>/isep-moodle-assistant.git
cd isep-moodle-assistant
pip install -r requirements.txt

:: 1) Moodle: pede o nº mecanográfico e a palavra-passe (guardada cifrada com DPAPI)
MoodleSync\configurar.bat

:: 2) Resumo por Session: assistente de configuração + agendamento das 18h
CONFIGURAR_TUDO.bat
```

O assistente pergunta também o **modo de resumo** (Regras ou IA) e cria `EmailNotifier\config.json` (ver `config.example.json`). Para mudar o modo mais tarde, corre `EmailNotifier\configurar_resumo.bat`. Para o resumo incluir o Moodle, define lá `moodle_sync_dir` e `moodle_download_dir` — o assistente preenche `moodle_sync_dir` automaticamente quando as duas pastas estão lado a lado.

Teste manual: `MoodleSync\sincronizar_agora.bat` e `EmailNotifier\executar_notificador.bat`.

## Segurança e privacidade

- Nada sensível vai para o repositório: `config.json`, palavras-passe, *mnemonic* do Session, logs e o estado local estão no `.gitignore`.
- A palavra-passe do Moodle fica cifrada com a conta Windows (DPAPI) — só o teu utilizador a consegue decifrar.
- No modo **Regras** todo o processamento é local e o único envio externo é a mensagem para o teu Session. No modo **IA**, o conteúdo dos emails vai também para o fornecedor que escolheres.
- Este projeto **não é oficial** nem afiliado ao ISEP. Usa-o apenas com a tua própria conta e respeitando as regras de utilização do Moodle.

## Estrutura

```
MoodleSync/      moodle_sync.py, configurar.bat, sincronizar_agora.bat
EmailNotifier/   main.py, mail_fetcher.py, summarizer.py (regras), ai_summarizer.py (IA),
                 configurar_resumo.py/.bat (escolher/alterar o modo), moodle_report.py,
                 session_sender.cjs (envio via Session), assistentes de configuração
CONFIGURAR_TUDO.bat
```

## Licença

[MIT](LICENSE)
