'use strict'
// Lê a caixa de entrada por IMAP com OAuth2 (XOAUTH2). Corre dentro da app (Electron como Node): não há nada a instalar.
// Uso: node mail_fetcher.cjs --host H --port P --user EMAIL --since YYYY-MM-DD [--max N]   (o token de acesso vem por stdin)
// Saída: uma linha JSON {"messages":[{"uid":N,"raw":"<hex>"}]} ou {"error":"..."}.
// ISEP_IMAP_PLAIN=1 usa TCP sem TLS (só para os testes com um servidor IMAP falso).
const net = require('node:net')
const tls = require('node:tls')

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const args = process.argv.slice(2)
const arg = (name, fallback) => {
  const i = args.indexOf(`--${name}`)
  return i >= 0 && args[i + 1] !== undefined ? args[i + 1] : fallback
}

function fail(message) {
  process.stdout.write(`${JSON.stringify({ error: String(message) })}\n`)
  process.exit(1)
}

function readStdin() {
  return new Promise((resolve) => {
    let data = ''
    process.stdin.setEncoding('utf8')
    process.stdin.on('data', (c) => (data += c))
    process.stdin.on('end', () => resolve(data.trim()))
  })
}

function imapDate(iso) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso || '')
  if (!m) throw new Error('--since deve ser YYYY-MM-DD')
  return `${Number(m[3])}-${MONTHS[Number(m[2]) - 1]}-${m[1]}`
}

/** Lê respostas IMAP completas (uma linha, mais os literais {n} que ela anuncia). */
class Reader {
  constructor(sock) {
    this.buf = Buffer.alloc(0)
    this.waiting = null
    this.error = null
    sock.on('data', (d) => {
      this.buf = Buffer.concat([this.buf, d])
      this.pump()
    })
    const die = (e) => {
      this.error = e
      this.pump()
    }
    sock.on('error', die)
    sock.on('close', () => die(new Error('ligação fechada pelo servidor')))
  }

  parse() {
    let pos = 0
    const parts = []
    const literals = []
    for (;;) {
      const end = this.buf.indexOf('\r\n', pos)
      if (end < 0) return null
      const line = this.buf.subarray(pos, end).toString('latin1')
      const lit = /\{(\d+)\}$/.exec(line)
      if (!lit) {
        parts.push(line)
        this.buf = this.buf.subarray(end + 2)
        return { text: parts.join(' '), literals }
      }
      const n = Number(lit[1])
      if (this.buf.length < end + 2 + n) return null
      parts.push(line.slice(0, line.length - lit[0].length))
      literals.push(this.buf.subarray(end + 2, end + 2 + n))
      pos = end + 2 + n
    }
  }

  pump() {
    if (!this.waiting) return
    const item = this.parse()
    if (item) {
      const { resolve } = this.waiting
      this.waiting = null
      resolve(item)
    } else if (this.error) {
      const { reject } = this.waiting
      this.waiting = null
      reject(this.error)
    }
  }

  next() {
    return new Promise((resolve, reject) => {
      this.waiting = { resolve, reject }
      this.pump()
    })
  }
}

async function main() {
  const host = arg('host')
  const port = Number(arg('port', '993'))
  const user = arg('user')
  const max = Math.min(Math.max(Number(arg('max', '300')) || 300, 1), 1000)
  if (!host || !user) return fail('faltam --host e --user')
  const since = imapDate(arg('since'))
  const token = await readStdin()
  if (!token) return fail('falta o token de acesso (stdin)')

  const plain = process.env.ISEP_IMAP_PLAIN === '1'
  const sock = await new Promise((resolve, reject) => {
    const s = plain ? net.connect(port, host) : tls.connect({ host, port, servername: host })
    s.once(plain ? 'connect' : 'secureConnect', () => resolve(s))
    s.once('error', reject)
    s.setTimeout(90000, () => reject(new Error('tempo limite na ligação ao servidor de email')))
  })
  const reader = new Reader(sock)
  let tagN = 0

  const run = async (command, continuation) => {
    const tag = `A${++tagN}`
    sock.write(`${tag} ${command}\r\n`)
    const untagged = []
    for (;;) {
      const item = await reader.next()
      if (item.text.startsWith('+')) {
        sock.write(`${continuation ?? ''}\r\n`) // falha de autenticação: o servidor pede uma linha vazia
        continue
      }
      if (item.text.startsWith(`${tag} `)) {
        const status = item.text.slice(tag.length + 1).split(' ')[0]
        return { status, text: item.text, untagged }
      }
      untagged.push(item)
    }
  }

  const greeting = await reader.next()
  if (!/^\* (OK|PREAUTH)/i.test(greeting.text)) return fail(`resposta inesperada do servidor: ${greeting.text.slice(0, 80)}`)

  const xoauth = Buffer.from(`user=${user}\x01auth=Bearer ${token}\x01\x01`).toString('base64')
  const auth = await run(`AUTHENTICATE XOAUTH2 ${xoauth}`)
  if (auth.status !== 'OK') return fail('o servidor recusou o início de sessão (a autorização pode ter expirado)')

  const sel = await run('SELECT INBOX')
  if (sel.status !== 'OK') return fail('não foi possível abrir a caixa de entrada')

  const found = await run(`UID SEARCH SINCE ${since}`)
  if (found.status !== 'OK') return fail('a pesquisa de emails falhou')
  const uids = found.untagged
    .filter((u) => u.text.startsWith('* SEARCH'))
    .flatMap((u) => u.text.slice('* SEARCH'.length).trim().split(/\s+/).filter(Boolean).map(Number))
    .sort((a, b) => a - b)
    .slice(-max)

  const messages = []
  for (let i = 0; i < uids.length; i += 25) {
    const batch = uids.slice(i, i + 25)
    const res = await run(`UID FETCH ${batch.join(',')} (UID BODY.PEEK[])`)
    if (res.status !== 'OK') return fail('a leitura dos emails falhou')
    for (const u of res.untagged) {
      const uid = /UID (\d+)/.exec(u.text)
      if (uid && u.literals.length) messages.push({ uid: Number(uid[1]), raw: u.literals[0].toString('hex') })
    }
  }
  try {
    await run('LOGOUT')
  } catch {
    /* o servidor fecha a ligação a seguir ao LOGOUT */
  }
  sock.destroy()
  process.stdout.write(`${JSON.stringify({ messages })}\n`)
  process.exit(0)
}

main().catch((e) => fail(e && e.message ? e.message : e))
