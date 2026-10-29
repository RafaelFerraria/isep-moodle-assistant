# Casca desktop (Electron)

Mostra a app React (`../app`) numa janela do Windows e trata do servidor local:

1. procura o servidor (`ISEP_API_BIN` > `resources/bin` > `server/target/{release,debug}` > servidor Python);
2. arranca-o (`serve`) e espera pelo ficheiro `%APPDATA%\ISEPAssistant\api.json` (porta + token) e por `/health`;
3. responde aos pedidos da janela por IPC (`window.isepBridge.request`): **o token nunca chega à página**;
4. fecha o servidor ao sair (e reutiliza um servidor já a correr, de outra instância);
5. abre a **entrada da Microsoft** numa janela própria (`isep:oauth`, `src/oauth.cjs`): sessão só em memória, só `https`, apanha o regresso a `https://localhost` e devolve esse endereço (com o código) à página sem o registar.

Segurança da janela: `contextIsolation`, `sandbox`, sem `nodeIntegration`, CSP restritiva (`connect-src 'none'`),
navegação e janelas novas bloqueadas (links externos abrem no browser).

```bat
npm install
npm run build:ui      :: compila a app (../app) para app/dist
npm start             :: janela com a app compilada
npm run dev           :: janela a apontar para http://127.0.0.1:5173 (npm run dev em ../app)
npm test              :: 15 testes (resolução do servidor, IPC/HTTP, arranque real dos servidores, regras da entrada da Microsoft)
npm run pack          :: empacota (sem instalador) em dist\win-unpacked
npm run dist          :: instalador NSIS
```

O pacote leva a interface, o servidor Rust, `session_sender.cjs` e `mail_fetcher.cjs` (corridos pelo próprio Electron, sem Node.js instalado). O Thunderbird não vai no pacote; `scripts/fetch-thunderbird.cjs` só serve para o modo antigo.

`ISEP_SMOKE_OUT=<prefixo>` grava `<prefixo>.png` e `<prefixo>.txt` com o que a janela mostra e sai (teste de fumo / capturas).
