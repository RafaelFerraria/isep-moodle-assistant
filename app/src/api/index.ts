import { AssistantApi } from './client'
import { BridgeTransport, HttpTransport } from './transport'
import { MockTransport } from './mock'

export * from './client'
export * from './types'
export { HttpTransport, BridgeTransport, MockTransport }
export type { Transport } from './transport'
export type { MockOptions } from './mock'

/**
 * Escolhe como falar com o servidor:
 *  1. dentro do Electron: pela ponte (o processo principal guarda o token);
 *  2. no browser (desenvolvimento): `?api=http://127.0.0.1:PORTA&token=...`;
 *  3. caso contrário: modo demonstração com dados simulados.
 */
export function createApi(): AssistantApi {
  if (typeof window !== 'undefined' && window.isepBridge) {
    return new AssistantApi(new BridgeTransport(window.isepBridge))
  }
  const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '')
  const base = params.get('api')
  if (base) return new AssistantApi(new HttpTransport(base, params.get('token')))
  return new AssistantApi(new MockTransport({ jobDurationMs: 1500 }))
}

export function isDemoMode(): boolean {
  if (typeof window === 'undefined') return false
  return !window.isepBridge && !new URLSearchParams(location.search).get('api')
}
