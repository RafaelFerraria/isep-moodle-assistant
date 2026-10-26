import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, AssistantApi, HttpTransport, MockTransport } from '.'
import type { Transport } from '.'

function fake(status: number, data: unknown): Transport {
  return { request: async () => ({ status, data }) }
}

describe('AssistantApi', () => {
  it('devolve os dados quando o pedido corre bem', async () => {
    const api = new AssistantApi(fake(200, { ok: true, version: '1' }))
    await expect(api.health()).resolves.toEqual({ ok: true, version: '1' })
  })

  it('converte 422 em erros por campo', async () => {
    const api = new AssistantApi(fake(422, { detail: 'Definições inválidas.', errors: { 'schedule.time': 'formato HH:MM' } }))
    const err = await api.updateSettings({ schedule: { time: 'x' } }).catch((e) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err.status).toBe(422)
    expect(err.fields).toEqual({ 'schedule.time': 'formato HH:MM' })
  })

  it('guarda o id da tarefa ativa no 409', async () => {
    const api = new AssistantApi(fake(409, { detail: 'Já existe uma tarefa em execução.', active_job_id: 'abc' }))
    const err = await api.startDigestRun().catch((e) => e)
    expect(err.status).toBe(409)
    expect(err.activeJobId).toBe('abc')
  })

  it('distingue token inválido de falha de rede', async () => {
    const unauthorized = await new AssistantApi(fake(401, { detail: 'Token em falta ou inválido.' })).status().catch((e) => e)
    expect(unauthorized.isUnauthorized).toBe(true)
    const broken: Transport = { request: async () => { throw new TypeError('fetch failed') } }
    const network = await new AssistantApi(broken).status().catch((e) => e)
    expect(network.isNetwork).toBe(true)
    expect(network.message).toMatch(/Sem ligação/)
  })

  it('usa uma mensagem genérica quando o erro não traz detalhe', async () => {
    const err = await new AssistantApi(fake(500, null)).status().catch((e) => e)
    expect(err.message).toBe('Erro 500')
  })

  it('monta os parâmetros do histórico e codifica ids', async () => {
    const mock = new MockTransport()
    const api = new AssistantApi(mock)
    await api.history({ kind: 'digest', limit: 10, offset: 5 })
    await api.historyItem('a/b')
      .catch(() => undefined)
    await api.job('x y', 3).catch(() => undefined)
    expect(mock.requests.map((r) => r.path)).toEqual(['/history?kind=digest&limit=10&offset=5', '/history/a%2Fb', '/jobs/x%20y?since_line=3'])
  })
})

describe('HttpTransport', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('envia o token, o corpo JSON e o prefixo /api/v1', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    const t = new HttpTransport('http://127.0.0.1:8765/', 'segredo')
    const res = await t.request('PUT', '/settings', { a: 1 })
    expect(res).toEqual({ status: 200, data: { ok: true } })
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('http://127.0.0.1:8765/api/v1/settings')
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer segredo')
    expect(init.body).toBe('{"a":1}')
  })

  it('tolera respostas sem JSON', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('Bad gateway', { status: 502 })))
    const res = await new HttpTransport('http://x').request('GET', '/status')
    expect(res.status).toBe(502)
    expect(res.data).toEqual({ detail: 'Bad gateway' })
  })

  it('não envia Authorization sem token', async () => {
    const fetchMock = vi.fn(async () => new Response('{}', { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    await new HttpTransport('http://x').request('GET', '/health')
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect((init.headers as Record<string, string>).Authorization).toBeUndefined()
  })
})
