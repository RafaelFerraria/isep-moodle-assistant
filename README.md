# ISEP Moodle Assistant

> Descarrega automaticamente o que aparece no Moodle e envia-te todos os dias um resumo claro dos emails da faculdade para o telemóvel, via [Session](https://getsession.org). Feito para estudantes do ISEP (Windows).

Uma **aplicação de secretária** que se configura sozinha, sem terminal, e corre todos os dias mesmo com a janela fechada:

| Página | O que mostra |
|---|---|
| **Resumo** | O resumo de hoje em destaque e os dos últimos 7, 15 ou 30 dias, com totais. |
| **Emails** | Os emails lidos da tua caixa, com o aviso de quais já entraram num resumo. Só leitura. |
| **Moodle** | Ficheiros pendentes, em curso (com percentagem) e já descarregados. |
| **Session** | Criar o bot, ligar o teu Session, destinatários e mensagem de teste. |
| **Histórico** | Execuções e registos dos últimos 7 dias, guardados no teu PC. |
| **Definições** | Curso, email, tipo de resumo (regras ou IA), agenda, Moodle, pastas e verificações. |

Na primeira vez abre-se um **assistente** (curso → email → Moodle → Session → resumo → agendamento). Nenhum passo exige terminal.

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
- **Novo no Moodle**: tipo de cada ficheiro (enunciado, ficha, slides, avaliação…) e uma pista do conteúdo (prazo mencionado ou primeira linha).
- **Faltas**, **Helpdesk** (um item por pedido, não por email) e **outros**.
- **Regras editáveis** (Definições): palavras importantes (sobem para o topo), remetentes a ignorar e assuntos a ignorar, com validação.

## Dois modos de resumo

Escolhes no assistente e podes mudar nas Definições.

| Modo | Como funciona | Privacidade |
|---|---|---|
| **Regras** (predefinido) | Classifica e agrupa os emails localmente (prazos, avisos, faltas, helpdesk…). Sem dependências nem custos. | Os emails não saem do teu PC. |
| **IA** | Um modelo reescreve o resumo de forma mais natural. | O assunto e o início do texto dos emails são enviados ao fornecedor escolhido. |

No modo **IA** ligas o modelo de uma de duas formas:

- **CLI com a tua conta (sem token):** uma ferramenta onde já fizeste login — `claude` (Claude Code), `gemini` (Gemini CLI, conta Google), `codex` (conta ChatGPT/OpenAI) — ou qualquer comando que leia o prompt por *stdin* (`custom`).
- **Token de API:** OpenAI, Anthropic, Google Gemini ou compatível com OpenAI. O token pode ficar numa variável de ambiente (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, recomendado) ou em `config.json` (local, fora do git).

Se a IA falhar (sem rede, login expirado, token inválido, limite de uso), o resumo diário **não deixa de ser enviado**: cai para o modo Regras e avisa no fim da mensagem. As mensagens de erro nunca incluem o token.

## Tudo dentro da app

A app traz o que precisa. **Não tens de instalar Python, Node.js nem Thunderbird.**

- **Email:** a app lê a tua caixa por IMAP, dentro dela.
  - **«Entrar com a conta da escola»** (recomendado para o ISEP/Microsoft 365): abre a página de entrada da Microsoft numa janela da app, como o Thunderbird faz. A app nunca vê a tua palavra-passe, só recebe uma autorização, que fica guardada cifrada.
  - **Email + palavra-passe + servidor**: para escolas que ainda aceitem este método. A Microsoft desligou o login por palavra-passe na maioria dos inquilinos («Basic authentication is disabled»); nesse caso a app explica-o e sugere o botão acima.
  - A app só lê: nunca apaga nem envia emails. Quem vem do Thunderbird (instalações antigas) continua a funcionar até mudar nas Definições.
- **Session:** o programa que fala com o Session vai dentro da app (corre com o Electron). A app cria o bot, deteta o teu Session ID quando lhe escreves «Olá» (ou aceita o ID colado) e envia o resumo. Falta-te apenas a aplicação do Session no telemóvel para o receberes, e escreveres uma vez ao bot, porque o Session não deixa o bot iniciar a conversa.
- **Moodle:** o endereço vem da tua instituição (ISEP, UBI, UMa, ULisboa, ULusófona e UCP têm o endereço automático; nas outras a app pede-o e confirma que é um Moodle). Podes escolher a pasta dos downloads e desligar o descarregamento automático.
- **Agendamento:** o assistente regista no Agendador de Tarefas do Windows um resumo diário à hora que escolheres, ao retomar da suspensão e ao iniciar sessão. As tarefas correm `isep-assistant-api.exe digest` com a pasta de dados da app, mesmo com a app fechada.

## Como funciona

```
        ┌─────────────┐   ficheiros novos   ┌──────────────┐
Moodle ─►  sincroniza  ├────────────────────►│   resumo     │──► Session (telemóvel)
        └─────────────┘                     │  (diário)    │
Email (IMAP / conta da escola) ────────────►└──────────────┘
```

1. O Moodle é sincronizado uma vez por dia, de forma incremental (espera pela rede, uma disciplina com erro não bloqueia as outras, com *lock* contra execuções em paralelo). O progresso é por ficheiro e por disciplina.
2. O resumo corre à hora definida, garante que o Moodle foi sincronizado, lê os emails novos (deduplicados por `Message-ID`) e envia o resultado para o Session.

## Requisitos

- Windows 10/11 (usa DPAPI para guardar segredos e o Agendador de Tarefas).
- A aplicação [Session](https://getsession.org) no telemóvel ou no PC, para receberes o resumo.
- Uma conta de email da escola e as credenciais do Moodle.
- *(Opcional, modo IA)* uma CLI com login (`claude`, `gemini`, `codex`) ou um token de API.

## Instalação

Gera a app (ver [Empacotar](#empacotar-windows)) e abre `ISEP Assistant.exe`: o assistente leva-te pelos passos. Os dados ficam em `%APPDATA%\ISEP Assistant\data`.

> A instalação em Python (`CONFIGURAR_TUDO.bat`, `MoodleSync\configurar.bat`) mantém-se como **versão anterior** para quem já a usa; faz o mesmo mas pela linha de comandos e só lê o email pelo Thunderbird.

## Segurança e privacidade

- Nada sensível vai para o repositório: `config.json`, palavras-passe, *mnemonic* do Session, logs e o estado local estão no `.gitignore`.
- Palavras-passe (Moodle, email) e tokens ficam **cifrados com a tua conta Windows (DPAPI)**: só o teu utilizador os decifra. A API local nunca os devolve.
- A janela da app **nunca vê o token da API**: o processo principal do Electron fala com o servidor e devolve só os dados. O servidor escuta **só em `127.0.0.1`**, exige token, valida o cabeçalho `Host` (contra *DNS rebinding*) e não tem CORS.
- A janela de entrada da Microsoft usa uma sessão só em memória, só navega por `https`, e o endereço de regresso (que leva o código de autorização) nunca é escrito em registos. O pedido usa PKCE e valida um estado anti-falsificação.
- No modo **Regras** todo o processamento é local e o único envio externo é a mensagem para o teu Session. No modo **IA**, o conteúdo dos emails vai também para o fornecedor que escolheres.
- Os registos guardam-se **7 dias** e o histórico de resumos **30 dias**; o que é mais antigo é apagado automaticamente.
- Este projeto **não é oficial** nem afiliado ao ISEP. Usa-o apenas com a tua própria conta e respeitando as regras de utilização do Moodle.

### Limitações conhecidas

- A entrada «com a conta da escola» usa por omissão o identificador público do Thunderbird (Mozilla), que muitas escolas já aprovaram. É uma solução prática para uso pessoal, que a Mozilla ou a escola podem deixar de aceitar. Dá para trocar por um registo próprio no Azure (`ms_client_id` / `ms_tenant_id` em `config.json`), que é também o caminho para uma versão em servidor.
- Se a escola exigir a aprovação de um administrador para a aplicação, a app mostra uma mensagem a dizê-lo.

## Estrutura

```
server/             NÚCLEO + API em Rust (isep-assistant-api.exe): resumos, Moodle, emails, Session, histórico, agendamento
app/                INTERFACE (React + TypeScript + Tailwind, Vite). Pronta para Capacitor (telemóvel, mais tarde)
desktop/            CASCA DESKTOP (Electron): arranca o servidor local, liga a app React a ele e abre a entrada da Microsoft
isep_assistant/     NÚCLEO em Python (versão anterior) — mesmo contrato, mesmos ficheiros de dados
EmailNotifier/      session_sender.cjs (Session), mail_fetcher.cjs (IMAP), assistentes de configuração em Python
MoodleSync/         atalhos da versão em Python
tests/              testes Python (pytest) + testes de contrato que correm contra Python E Rust
docs/API.md         contrato da API local
```

```
┌────────────── Electron (desktop/) ──────────────┐
│  janela: app React (app/)  ──IPC──►  processo   │
│  (sem token, sem rede)              principal   │──HTTP 127.0.0.1 + token──► isep-assistant-api (Rust)
└─────────────────────────────────────────────────┘                            ├─ resumo diário, Moodle, histórico
                                                                                ├─ email (IMAP) e Session, via o Electron como Node
                                                                                └─ Agendador de Tarefas ► `digest`
```

A interface é a mesma para qualquer servidor: mais tarde basta apontá-la a uma API na nuvem (`HttpTransport`).

## Desenvolvimento

Pré-requisitos: Node.js 20+, Rust (o toolchain `stable-x86_64-pc-windows-gnu` basta — não precisa de MinGW nem do Visual Studio C++, ver [server/README.md](server/README.md)) e Python 3.9+ (só para os testes de contrato).

```bat
:: 1) servidor Rust
cd server
cargo build           :: debug; "cargo build --release" para o executável final
cargo test            :: 179 testes (HTTP, configuração, resumos, IA, Moodle, email, OAuth, Session, API…)

:: 2) interface
cd ..\app
npm install
npm test              :: 105 testes (Vitest + Testing Library)
npm run dev           :: http://127.0.0.1:5173 em modo demonstração (dados simulados)

:: 3) desktop (arranca o servidor Rust e mostra a app)
cd ..\desktop
npm install
npm run build:ui      :: compila a app para app/dist
npm start             :: ou "npm run dev" com a app em http://127.0.0.1:5173
npm test              :: 15 testes (inclui arrancar os servidores a sério e a entrada da Microsoft)

:: 4) contrato: o mesmo conjunto de pedidos contra Python E Rust
cd ..
pip install -r requirements-dev.txt
python -m pytest      :: 137 testes (núcleo Python + contrato)
```

O Electron escolhe o servidor por esta ordem: `ISEP_API_BIN`, executável instalado, `server/target/{release,debug}`, e por fim o servidor Python.

### Empacotar (Windows)

```bat
cd desktop
npm run pack          :: pasta sem instalador em desktop\dist\win-unpacked
npm run dist          :: instalador NSIS
```

O pacote leva a interface, o servidor Rust, o envio para o Session e o leitor de email. O Thunderbird **não** vai no pacote. O `desktop\scripts\fetch-thunderbird.cjs` só serve para quem queira o modo antigo.

### Telemóvel (mais tarde)

A app já tem `capacitor.config.ts`. Quando chegar a altura: `npm run build && npx cap add android && npx cap sync`. No telemóvel a `HttpTransport` falará com uma API na nuvem em vez do servidor local.

## Licença

[MIT](LICENSE)
