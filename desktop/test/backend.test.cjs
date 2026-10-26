'use strict'
const test = require('node:test')
const assert = require('node:assert/strict')
const fs = require('node:fs')
const http = require('node:http')
const os = require('node:os')
const path = require('node:path')
const backend = require('../src/backend.cjs')

const REPO = path.resolve(__dirname, '..', '..')
const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'isep-desktop-'))

function fakeServer(handler) {
  return new Promise((resolve) => {
    const server = http.createServer(handler)
    server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port }))
  })
}

test('stateFile usa ISEP_ASSISTANT_STATE ou %APPDATA%/ISEPAssistant', () => {
  assert.equal(backend.stateFile({ ISEP_ASSISTANT_STATE: 'X:/estado' }), path.join('X:/estado', 'api.json'))
  assert.equal(backend.stateFile({ APPDATA: 'C:/Users/a/AppData/Roaming' }), path.join('C:/Users/a/AppData/Roaming', 'ISEPAssistant', 'api.json'))
})

test('resolveBackend respeita a ordem: ISEP_API_BIN > instalado > Rust de desenvolvimento > Python', () => {
  const present = new Set()
  const exists = (p) => present.has(p)
  const exe = backend.EXE
  const rel = path.join('R', 'bin', exe)
  const rust = path.join('REPO', 'server', 'target', 'release', exe)
  const dbg = path.join('REPO', 'server', 'target', 'debug', exe)
  const py = path.join('REPO', 'isep_assistant', '__init__.py')

  assert.equal(backend.resolveBackend({ env: {}, repoRoot: 'REPO', exists }), null)

  present.add(py)
  let plan = backend.resolveBackend({ env: {}, repoRoot: 'REPO', exists })
  assert.equal(plan.kind, 'python')
  assert.deepEqual(plan.args, ['-m', 'isep_assistant.api.server'])
  assert.ok(plan.env.PYTHONPATH.startsWith('REPO'))

  present.add(dbg)
  assert.equal(backend.resolveBackend({ env: {}, repoRoot: 'REPO', exists }).command, dbg)
  present.add(rust)
  assert.equal(backend.resolveBackend({ env: {}, repoRoot: 'REPO', exists }).command, rust)
  present.add(rel)
  assert.equal(backend.resolveBackend({ env: {}, resourcesPath: 'R', repoRoot: 'REPO', exists }).command, rel)
  present.add('C:/meu/servidor.exe')
  const plan2 = backend.resolveBackend({ env: { ISEP_API_BIN: 'C:/meu/servidor.exe' }, resourcesPath: 'R', repoRoot: 'REPO', exists })
  assert.equal(plan2.command, 'C:/meu/servidor.exe')
  assert.equal(plan2.kind, 'rust')
  assert.deepEqual(plan2.args, ['serve'], 'o executável Rust precisa do subcomando serve')
})

test('resolveBackend inclui .deps no PYTHONPATH quando existe', () => {
  const deps = path.join('REPO', '.deps')
  const exists = (p) => p === deps || p === path.join('REPO', 'isep_assistant', '__init__.py')
  const plan = backend.resolveBackend({ env: { PYTHONPATH: 'EXTRA' }, repoRoot: 'REPO', exists })
  assert.deepEqual(plan.env.PYTHONPATH.split(path.delimiter), ['REPO', deps, 'EXTRA'])
})

test('readState só aceita porta inteira e token', () => {
  const dir = tmp()
  const file = path.join(dir, 'api.json')
  assert.equal(backend.readState(file), null)
  fs.writeFileSync(file, '{"port": 123, "token": ""}')
  assert.equal(backend.readState(file), null)
  fs.writeFileSync(file, '{nao json')
  assert.equal(backend.readState(file), null)
  fs.writeFileSync(file, '\uFEFF{"port": 123, "token": "t", "pid": 7}')
  assert.deepEqual(backend.readState(file), { port: 123, token: 't', pid: 7 })
})

test('httpRequest envia token e corpo JSON e devolve estado + dados', async () => {
  const seen = {}
  const { server, port } = await fakeServer((req, res) => {
    let body = ''
    req.on('data', (c) => (body += c))
    req.on('end', () => {
      Object.assign(seen, { url: req.url, method: req.method, auth: req.headers.authorization, host: req.headers.host, body })
      res.writeHead(422, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ detail: 'inválido', errors: { campo: 'erro' } }))
    })
  })
  try {
    const res = await backend.httpRequest({ port, token: 'segredo', method: 'PUT', path: '/settings', body: { a: 'ç' } })
    assert.equal(res.status, 422)
    assert.deepEqual(res.data.errors, { campo: 'erro' })
    assert.equal(seen.url, '/api/v1/settings')
    assert.equal(seen.auth, 'Bearer segredo')
    assert.equal(seen.body, '{"a":"ç"}')
    assert.match(seen.host, /^127\.0\.0\.1:/)
  } finally {
    server.close()
  }
})

test('httpRequest tolera respostas que não são JSON e rejeita se não houver servidor', async () => {
  const { server, port } = await fakeServer((req, res) => res.end('Bad gateway'))
  try {
    assert.deepEqual((await backend.httpRequest({ port, method: 'GET', path: '/x' })).data, { detail: 'Bad gateway' })
  } finally {
    server.close()
  }
  await assert.rejects(backend.httpRequest({ port, method: 'GET', path: '/x', timeoutMs: 500 }))
})

test('waitForBackend ignora estado de outro processo e espera pelo certo', async () => {
  const dir = tmp()
  const file = path.join(dir, 'api.json')
  const { server, port } = await fakeServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end('{"ok":true}')
  })
  try {
    fs.writeFileSync(file, JSON.stringify({ port: 1, token: 'velho', pid: 111 })) // execução anterior
    const child = { pid: 222, exitCode: null }
    setTimeout(() => fs.writeFileSync(file, JSON.stringify({ port, token: 'novo', pid: 222 })), 300)
    const state = await backend.waitForBackend({ file, child, timeoutMs: 5000, intervalMs: 50 })
    assert.equal(state.token, 'novo')
  } finally {
    server.close()
  }
})

test('waitForBackend falha depressa se o processo terminar e dá erro claro no timeout', async () => {
  const file = path.join(tmp(), 'api.json')
  await assert.rejects(backend.waitForBackend({ file, child: { pid: 1, exitCode: 3 }, timeoutMs: 2000 }), /terminou \(código 3\)/)
  await assert.rejects(backend.waitForBackend({ file, child: { pid: 1, exitCode: null }, timeoutMs: 300, intervalMs: 50 }), /tempo esgotado/)
})

test('startBackend sem servidor disponível explica o que falta', async () => {
  const dir = tmp()
  await assert.rejects(
    backend.startBackend({ env: { ISEP_ASSISTANT_STATE: dir }, repoRoot: path.join(dir, 'vazio'), exists: () => false }),
    /não encontrei o servidor local/,
  )
})

// --- integração real: arranca o servidor Python do repositório e fala com ele como a app faz ---
test('startBackend arranca o servidor real, autentica e fecha', async () => {
  const dir = tmp()
  const home = path.join(dir, 'home')
  fs.mkdirSync(path.join(home, 'EmailNotifier'), { recursive: true })
  fs.mkdirSync(path.join(home, 'MoodleSync'), { recursive: true })
  const env = { ...process.env, ISEP_ASSISTANT_STATE: path.join(dir, 'state'), ISEP_ASSISTANT_ROOT: home }
  Object.assign(process.env, { ISEP_ASSISTANT_STATE: env.ISEP_ASSISTANT_STATE, ISEP_ASSISTANT_ROOT: home })
  let result
  try {
    result = await backend.startBackend({ env, repoRoot: REPO, timeoutMs: 40000, exists: (p) => !p.includes('target') && fs.existsSync(p) })
    assert.equal(result.reused, false)
    assert.equal(result.plan.kind, 'python')
    const health = await backend.httpRequest({ port: result.state.port, method: 'GET', path: '/health' })
    assert.equal(health.status, 200)
    const noToken = await backend.httpRequest({ port: result.state.port, method: 'GET', path: '/status' })
    assert.equal(noToken.status, 401)
    const status = await backend.httpRequest({ port: result.state.port, token: result.state.token, method: 'GET', path: '/status' })
    assert.equal(status.status, 200)
    assert.equal(status.data.summary_mode.mode, 'rules')

    // uma segunda instância da app reutiliza o servidor em vez de arrancar outro
    const again = await backend.startBackend({ env, repoRoot: REPO, timeoutMs: 40000 })
    assert.equal(again.reused, true)
    assert.equal(again.state.port, result.state.port)
  } finally {
    if (result && result.child) result.child.kill()
  }
})

// --- integração real com o servidor Rust (se estiver compilado) ---
const RUST_EXE = path.join(REPO, 'server', 'target', 'debug', backend.EXE)
test('startBackend prefere o servidor Rust quando está compilado', { skip: !fs.existsSync(RUST_EXE) }, async () => {
  const dir = tmp()
  const home = path.join(dir, 'home')
  fs.mkdirSync(path.join(home, 'EmailNotifier'), { recursive: true })
  fs.mkdirSync(path.join(home, 'MoodleSync'), { recursive: true })
  const env = { ...process.env, ISEP_ASSISTANT_STATE: path.join(dir, 'state'), ISEP_ASSISTANT_ROOT: home, ISEP_FAKE_SCHEDULER: '1' }
  Object.assign(process.env, { ISEP_ASSISTANT_STATE: env.ISEP_ASSISTANT_STATE, ISEP_ASSISTANT_ROOT: home, ISEP_FAKE_SCHEDULER: '1' })
  let result
  try {
    result = await backend.startBackend({ env, repoRoot: REPO, timeoutMs: 20000 })
    assert.equal(result.plan.kind, 'rust')
    assert.equal(result.reused, false)
    const health = await backend.httpRequest({ port: result.state.port, method: 'GET', path: '/health' })
    assert.equal(health.status, 200)
    assert.equal(health.data.version.split('.').length, 3)
    assert.equal((await backend.httpRequest({ port: result.state.port, method: 'GET', path: '/status' })).status, 401)
    const settings = await backend.httpRequest({ port: result.state.port, token: result.state.token, method: 'GET', path: '/settings' })
    assert.equal(settings.status, 200)
    assert.equal(settings.data.summary.mode, 'rules')
  } finally {
    if (result && result.child) result.child.kill()
  }
})
