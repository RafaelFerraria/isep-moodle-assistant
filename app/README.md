# Interface (React + TypeScript + Tailwind)

Sobre a API local (`../docs/API.md`). Na primeira vez abre-se o **assistente de configuração**; depois, seis páginas:

- **Resumo**: o resumo de hoje em destaque, o seletor de 7/15/30 dias, totais e as ações *Pré-visualizar* e *Enviar agora*.
- **Emails**: emails lidos da caixa, com o aviso dos que já entraram num resumo.
- **Moodle**: ficheiros pendentes, em curso (com percentagem) e já descarregados, e *Sincronizar agora*.
- **Session**: criar o bot, ligar o teu Session (deteção automática ou ID colado), destinatários e mensagem de teste.
- **Histórico**: execuções e registos dos últimos 7 dias.
- **Definições**: curso, email (entrar com a conta da escola ou palavra-passe), tipo de resumo (Regras ou IA) com teste, regras, hora, Moodle, pastas, notificações e verificações. Os segredos só se escrevem, nunca se mostram.

```bat
npm install
npm run dev       :: http://127.0.0.1:5173 — modo demonstração (dados simulados)
npm test          :: Vitest + Testing Library (105 testes)
npm run build     :: dist/ (usado pelo Electron e, mais tarde, pelo Capacitor)
```

## Como fala com o servidor

`createApi()` escolhe o transporte (`src/api/index.ts`):

1. dentro do Electron: `BridgeTransport` (`window.isepBridge`, o token fica no processo principal);
2. no browser, com `?api=http://127.0.0.1:PORTA&token=…`: `HttpTransport` (o servidor tem de permitir a origem; por omissão não há CORS);
3. caso contrário: `MockTransport`, um servidor simulado em memória que respeita as mesmas regras (validação com erros por campo, segredos só de escrita, uma tarefa de cada vez) — usado na demonstração e nos testes.

## Estrutura

```
src/api/        tipos do contrato, transportes, cliente (AssistantApi), mock
src/lib/        formatação pt-PT, contexto, hooks (useAsync, useJobRunner)
src/components/ primitivas de interface (Tailwind), painel de tarefas, email, Session, Moodle
src/pages/      Resumo, Emails, Moodle, Session, Histórico, Definições e o assistente (+ testes ao lado)
```

## Telemóvel (mais tarde)

`capacitor.config.ts` já existe. `npm run build && npx cap add android && npx cap sync` quando chegar a altura; no telemóvel a `HttpTransport` fala com uma API na nuvem.
