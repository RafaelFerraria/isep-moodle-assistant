'use strict'
const { app, BrowserWindow, ipcMain, shell, session } = require('electron')
const fs = require('node:fs')
const path = require('node:path')
const { httpRequest, startBackend } = require('./backend.cjs')

const REPO_ROOT = path.resolve(__dirname, '..', '..')
const DEV_URL = process.env.ELECTRON_DEV_URL || null // ex.: http://127.0.0.1:5173 (npm run dev na pasta app)

/**
 * Instalada, a app guarda os dados em %APPDATA%/<app>/data (config, histórico, last_run…) e copia o
 * session_sender.cjs para lá. Em desenvolvimento usa a raiz do repositório, como as instalações Python.
 */
function prepareDataRoot() {
  if (!app.isPackaged || process.env.ISEP_ASSISTANT_ROOT) return
  const root = path.join(app.getPath('userData'), 'data')
  for (const d of ['EmailNotifier', 'MoodleSync']) fs.mkdirSync(path.join(root, d), { recursive: true })
  const src = path.join(process.resourcesPath, 'session', 'session_sender.cjs')
  const dst = path.join(root, 'EmailNotifier', 'session_sender.cjs')
  if (fs.existsSync(src) && (!fs.existsSync(dst) || fs.statSync(src).mtimeMs > fs.statSync(dst).mtimeMs)) fs.copyFileSync(src, dst)
  process.env.ISEP_ASSISTANT_ROOT = root
}

/** @type {{ child: import('node:child_process').ChildProcess | null, state: { port: number, token: string } | null, error: string | null }} */
const backend = { child: null, state: null, error: null }
let ready = null

function ensureBackend() {
  if (!ready) {
    prepareDataRoot()
    ready = startBackend({
      repoRoot: app.isPackaged ? null : REPO_ROOT,
      resourcesPath: app.isPackaged ? process.resourcesPath : null,
    })
      .then((r) => {
        backend.child = r.child
        backend.state = r.state
        return r
      })
      .catch((e) => {
        backend.error = e.message
        ready = null // permite tentar de novo
        throw e
      })
  }
  return ready
}

// O token nunca chega à página: a ponte pede e o processo principal acrescenta-o.
ipcMain.handle('isep:request', async (_event, method, urlPath, body) => {
  if (typeof method !== 'string' || typeof urlPath !== 'string' || !urlPath.startsWith('/')) {
    return { status: 400, data: { detail: 'Pedido inválido.' } }
  }
  try {
    await ensureBackend()
  } catch (e) {
    return { status: 503, data: { detail: `Servidor local indisponível: ${e.message}` } }
  }
  const { port, token } = backend.state
  try {
    return await httpRequest({ port, token, method: method.toUpperCase(), path: urlPath, body })
  } catch (e) {
    // o servidor pode ter caído: tenta arrancá-lo uma vez e repete
    backend.state = null
    ready = null
    try {
      await ensureBackend()
      return await httpRequest({ port: backend.state.port, token: backend.state.token, method: method.toUpperCase(), path: urlPath, body })
    } catch (e2) {
      throw new Error(e2.message || e.message)
    }
  }
})

ipcMain.handle('isep:backend', async () => {
  try {
    await ensureBackend()
    return { ready: true, error: null }
  } catch (e) {
    return { ready: false, error: e.message }
  }
})

function createWindow() {
  const win = new BrowserWindow({
    width: 1180,
    height: 800,
    minWidth: 900,
    minHeight: 600,
    title: 'ISEP Assistant',
    backgroundColor: '#0f172a',
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })
  win.setMenuBarVisibility(false)

  // A app não abre outras janelas nem sai do seu próprio endereço; links externos vão para o browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) void shell.openExternal(url)
    return { action: 'deny' }
  })
  win.webContents.on('will-navigate', (event, url) => {
    const allowed = DEV_URL ? url.startsWith(DEV_URL) : url.startsWith('file://')
    if (!allowed) event.preventDefault()
  })

  // Teste de fumo: ISEP_SMOKE_OUT=<prefixo> -> grava <prefixo>.png e <prefixo>.txt com o que a janela mostra e sai
  if (process.env.ISEP_SMOKE_OUT) {
    const out = process.env.ISEP_SMOKE_OUT
    const fs = require('node:fs')
    win.webContents.once('did-finish-load', () => {
      setTimeout(async () => {
        try {
          const text = await win.webContents.executeJavaScript('document.body.innerText')
          fs.writeFileSync(`${out}.txt`, text)
          fs.writeFileSync(`${out}.png`, (await win.webContents.capturePage()).toPNG())
          app.exit(0)
        } catch (e) {
          fs.writeFileSync(`${out}.txt`, `ERRO: ${e.message}`)
          app.exit(1)
        }
      }, Number(process.env.ISEP_SMOKE_DELAY_MS || 4000))
    })
  }

  if (DEV_URL) void win.loadURL(DEV_URL)
  else void win.loadFile(app.isPackaged ? path.join(process.resourcesPath, 'ui', 'index.html') : path.join(REPO_ROOT, 'app', 'dist', 'index.html'))
}

app.whenReady().then(() => {
  // CSP restritiva: só código da própria app; a rede passa toda pelo processo principal (IPC).
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    const dev = !!DEV_URL
    const csp = dev
      ? "default-src 'self' http://127.0.0.1:5173 ws://127.0.0.1:5173; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline' 'unsafe-eval'"
      : "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'none'; object-src 'none'; base-uri 'none'"
    callback({ responseHeaders: { ...details.responseHeaders, 'Content-Security-Policy': [csp] } })
  })
  createWindow()
  void ensureBackend().catch(() => undefined) // arranca já; o erro, se houver, aparece na app
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => app.quit())
app.on('before-quit', () => {
  if (backend.child && backend.child.exitCode === null) backend.child.kill()
})
