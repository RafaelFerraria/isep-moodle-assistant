# Interface (React + TypeScript + Tailwind)

Três ecrãs sobre a API local (`../docs/API.md`):

- **Estado**: saúde geral, próximo/último resumo, Moodle, modo de resumo, verificações e as ações *Pré-visualizar*, *Enviar agora* e *Sincronizar Moodle* (com progresso).
- **Histórico**: resumos enviados, sincronizações, ficheiros novos do Moodle e registos.
- **Definições**: modo de resumo (Regras ou IA: CLI com login ou token de API) com teste, hora do resumo, destino Session, conta do Moodle e pastas. Os segredos só se escrevem, nunca se mostram.

```bat
npm install
npm run dev       :: http://127.0.0.1:5173 — modo demonstração (dados simulados)
npm test          :: Vitest + Testing Library (54 testes)
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
src/components/ primitivas de interface (Tailwind) e o painel de tarefas
src/pages/      Estado, Histórico, Definições (+ testes ao lado)
```

## Telemóvel (mais tarde)

`capacitor.config.ts` já existe. `npm run build && npx cap add android && npx cap sync` quando chegar a altura; no telemóvel a `HttpTransport` fala com uma API na nuvem.
