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

O assistente cria `EmailNotifier\config.json` (ver `config.example.json`). Para o resumo incluir o Moodle, define lá `moodle_sync_dir` e `moodle_download_dir` — o assistente preenche `moodle_sync_dir` automaticamente quando as duas pastas estão lado a lado.

Teste manual: `MoodleSync\sincronizar_agora.bat` e `EmailNotifier\executar_notificador.bat`.

## Segurança e privacidade

- Nada sensível vai para o repositório: `config.json`, palavras-passe, *mnemonic* do Session, logs e o estado local estão no `.gitignore`.
- A palavra-passe do Moodle fica cifrada com a conta Windows (DPAPI) — só o teu utilizador a consegue decifrar.
- Todo o processamento é local; o único envio externo é a mensagem para o teu Session.
- Este projeto **não é oficial** nem afiliado ao ISEP. Usa-o apenas com a tua própria conta e respeitando as regras de utilização do Moodle.

## Estrutura

```
MoodleSync/      moodle_sync.py, configurar.bat, sincronizar_agora.bat
EmailNotifier/   main.py, mail_fetcher.py, summarizer.py, moodle_report.py,
                 session_sender.cjs (envio via Session), assistentes de configuração
CONFIGURAR_TUDO.bat
```

## Licença

[MIT](LICENSE)
