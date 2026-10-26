'use strict'
// Lógica do processo principal sem dependência do Electron (testável com `node --test`).
const { spawn } = require('node:child_process')
const fs = require('node:fs')
const http = require('node:http')
const os = require('node:os')
const path = require('node:path')

const EXE = process.platform === 'win32' ? 'isep-assistant-api.exe' : 'isep-assistant-api'

/** Pasta onde o servidor escreve a porta e o token (igual a paths::state_dir no núcleo). */
function stateFile(env = process.env) {
  const dir = env.ISEP_ASSISTANT_STATE || path.join(env.APPDATA || path.join(os.homedir(), '.config'), 'ISEPAssistant')
  return path.join(dir, 'api.json')
}

/**
 * Como arrancar o servidor, por ordem:
 *  1. ISEP_API_BIN (caminho explícito);
 *  2. executável Rust ao lado da app instalada (resources/bin);
 *  3. executável Rust de desenvolvimento (server/target/release|debug);
 *  4. servidor Python (mesmo contrato), a partir da raiz do repositório.
 */
function resolveBackend({ env = process.env, resourcesPath = null, repoRoot, exists = fs.existsSync } = {}) {
  const candidates = []
  if (env.ISEP_API_BIN) candidates.push(env.ISEP_API_BIN)
  if (resourcesPath) candidates.push(path.join(resourcesPath, 'bin', EXE))
  if (repoRoot) {
    candidates.push(path.join(repoRoot, 'server', 'target', 'release', EXE))
    candidates.push(path.join(repoRoot, 'server', 'target', 'debug', EXE))
  }
  const bin = candidates.find((c) => exists(c))
  if (bin) return { kind: 'rust', command: bin, args: ['serve'], cwd: repoRoot || path.dirname(bin) }
  if (repoRoot && exists(path.join(repoRoot, 'isep_assistant', '__init__.py'))) {
    const deps = path.join(repoRoot, '.deps')
    const pythonPath = [repoRoot, exists(deps) ? deps : null, env.PYTHONPATH].filter(Boolean).join(path.delimiter)
    return {
      kind: 'python',
      command: env.ISEP_PYTHON || 'python',
      args: ['-m', 'isep_assistant.api.server'],
      cwd: repoRoot,
      env: { PYTHONPATH: pythonPath, PYTHONIOENCODING: 'utf-8' },
    }
  }
  return null
}

function readState(file) {
  try {
    const state = JSON.parse(fs.readFileSync(file, 'utf8').replace(/^﻿/, ''))
    if (Number.isInteger(state.port) && typeof state.token === 'string' && state.token) return state
  } catch {
    /* ainda não existe ou está a meio de ser escrito */
  }
  return null
}

function httpRequest({ port, token, method, path: urlPath, body, host = '127.0.0.1', timeoutMs = 120000 }) {
  return new Promise((resolve, reject) => {
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body))
    const headers = { Accept: 'application/json' }
    if (token) headers.Authorization = `Bearer ${token}`
    if (payload) {
      headers['Content-Type'] = 'application/json'
      headers['Content-Length'] = payload.length
    }
    const req = http.request({ host, port, method, path: `/api/v1${urlPath}`, headers, timeout: timeoutMs }, (res) => {
      const chunks = []
      res.on('data', (c) => chunks.push(c))
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8')
        let data = null
        if (text) {
          try {
            data = JSON.parse(text)
          } catch {
            data = { detail: text.slice(0, 200) }
          }
        }
        resolve({ status: res.statusCode || 0, data })
      })
    })
    req.on('timeout', () => req.destroy(new Error('tempo limite')))
    req.on('error', reject)
    if (payload) req.write(payload)
    req.end()
  })
}

/** Espera que o servidor escreva o estado (do processo que arrancámos) e responda em /health. */
async function waitForBackend({ file, child, timeoutMs = 30000, intervalMs = 150, request = httpRequest }) {
  const start = Date.now()
  let lastError = 'o servidor não arrancou'
  while (Date.now() - start < timeoutMs) {
    if (child && child.exitCode !== null) throw new Error(`o servidor terminou (código ${child.exitCode})`)
    const state = readState(file)
    // um ficheiro de uma execução anterior tem outro pid: ignora-o
    if (state && (!child || !state.pid || state.pid === child.pid)) {
      try {
        const res = await request({ port: state.port, token: null, method: 'GET', path: '/health', timeoutMs: 2000 })
        if (res.status === 200) return state
        lastError = `/health devolveu ${res.status}`
      } catch (e) {
        lastError = e.message
      }
    }
    await new Promise((r) => setTimeout(r, intervalMs))
  }
  throw new Error(`tempo esgotado à espera do servidor (${lastError})`)
}

function spawnBackend(plan, { spawnFn = spawn } = {}) {
  const child = spawnFn(plan.command, plan.args, {
    cwd: plan.cwd,
    env: { ...process.env, ...(plan.env || {}) },
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let output = ''
  const keep = (b) => {
    output = (output + b.toString()).slice(-4000)
  }
  child.stdout && child.stdout.on('data', keep)
  child.stderr && child.stderr.on('data', keep)
  child.tail = () => output
  return child
}

/** Liga tudo: encontra, arranca e espera. Devolve { child, state, plan } ou lança com uma mensagem clara. */
async function startBackend(options = {}) {
  const { env = process.env } = options
  const file = stateFile(env)
  // se já há um servidor válido (outra instância da app), reutiliza-o
  const existing = readState(file)
  if (existing) {
    try {
      const res = await httpRequest({ port: existing.port, token: null, method: 'GET', path: '/health', timeoutMs: 1500 })
      if (res.status === 200) return { child: null, state: existing, plan: null, reused: true }
    } catch {
      /* ficheiro velho: continua e arranca um novo */
    }
  }
  const plan = resolveBackend(options)
  if (!plan) throw new Error('não encontrei o servidor local (isep-assistant-api nem o núcleo Python).')
  try {
    fs.unlinkSync(file)
  } catch {
    /* não existia */
  }
  const child = spawnBackend(plan, options)
  try {
    const state = await waitForBackend({ file, child, timeoutMs: options.timeoutMs })
    return { child, state, plan, reused: false }
  } catch (e) {
    try {
      child.kill()
    } catch {
      /* já terminou */
    }
    throw new Error(`${e.message}${child.tail && child.tail() ? `\n${child.tail().trim()}` : ''}`)
  }
}

module.exports = { stateFile, resolveBackend, readState, httpRequest, waitForBackend, spawnBackend, startBackend, EXE }
