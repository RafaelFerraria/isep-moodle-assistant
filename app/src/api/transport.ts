// Camada de transporte: a app não sabe se fala por HTTP, pela ponte do Electron ou com o mock.

export interface TransportResponse {
  status: number
  data: unknown
}

export interface Transport {
  request(method: string, path: string, body?: unknown): Promise<TransportResponse>
}

/** HTTP direto (desenvolvimento no browser, futura nuvem). Em `baseUrl` vai o endereço sem `/api/v1`. */
export class HttpTransport implements Transport {
  private readonly baseUrl: string
  private readonly token: string | null

  constructor(baseUrl: string, token: string | null = null) {
    this.baseUrl = baseUrl.replace(/\/+$/, '')
    this.token = token
  }

  async request(method: string, path: string, body?: unknown): Promise<TransportResponse> {
    const headers: Record<string, string> = { Accept: 'application/json' }
    if (body !== undefined) headers['Content-Type'] = 'application/json'
    if (this.token) headers.Authorization = `Bearer ${this.token}`
    const res = await fetch(`${this.baseUrl}/api/v1${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    })
    const text = await res.text()
    let data: unknown = null
    if (text) {
      try {
        data = JSON.parse(text)
      } catch {
        data = { detail: text.slice(0, 200) }
      }
    }
    return { status: res.status, data }
  }
}

/** Ponte exposta pelo Electron (preload): o processo principal fala com o servidor local e guarda o token. */
export interface IsepBridge {
  request(method: string, path: string, body?: unknown): Promise<TransportResponse>
  backend?(): Promise<{ ready: boolean; error?: string | null; version?: string }>
  /** Abre o seletor de pastas do sistema. Devolve `null` se o utilizador cancelar. */
  pickFolder?(): Promise<string | null>
}

declare global {
  interface Window {
    isepBridge?: IsepBridge
  }
}

export class BridgeTransport implements Transport {
  private readonly bridge: IsepBridge

  constructor(bridge: IsepBridge) {
    this.bridge = bridge
  }

  request(method: string, path: string, body?: unknown): Promise<TransportResponse> {
    return this.bridge.request(method, path, body)
  }
}
