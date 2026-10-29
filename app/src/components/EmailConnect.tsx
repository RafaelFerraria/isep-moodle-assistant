import { ExternalLink, Mail } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { ApiError } from '../api'
import type { DeviceLogin } from '../api'
import { useApp } from '../lib/context'
import { Button, Notice } from './ui'

/**
 * Liga a conta Microsoft do ISEP à app (serviço próprio de email). Mostra o código, espera que o utilizador o
 * escreva em microsoft.com/devicelogin e consulta o servidor até a autorização ser concluída.
 */
export function EmailConnect({ connected, onConnected }: { connected: boolean; onConnected: () => void }) {
  const { api, pollMs } = useApp()
  const [login, setLogin] = useState<DeviceLogin | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const alive = useRef(true)

  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
      if (timer.current) clearTimeout(timer.current)
    }
  }, [])

  const poll = useCallback(
    (l: DeviceLogin) => {
      // nos testes (pollMs pequeno) não esperamos o intervalo real da Microsoft
      const wait = pollMs >= 1000 ? Math.max(l.interval, 1) * 1000 : pollMs
      timer.current = setTimeout(async () => {
        try {
          const r = await api.pollEmailConnect(l.device_code)
          if (!alive.current) return
          if (r.status === 'pending') poll(l)
          else if (r.status === 'done') {
            setLogin(null)
            onConnected()
          } else {
            setLogin(null)
            setError(r.message)
          }
        } catch (e) {
          if (!alive.current) return
          setLogin(null)
          setError(e instanceof ApiError ? e.message : String(e))
        }
      }, wait)
    },
    [api, pollMs, onConnected],
  )

  const start = async () => {
    setBusy(true)
    setError(null)
    try {
      const l = await api.startEmailConnect()
      if (!alive.current) return
      setLogin(l)
      poll(l)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      if (alive.current) setBusy(false)
    }
  }

  return (
    <div className="space-y-3">
      {connected && !login && <Notice tone="good">Conta Microsoft ligada. Os emails são lidos diretamente, sem o Thunderbird.</Notice>}
      {login ? (
        <div className="space-y-3 rounded-xl border border-brand-200 bg-brand-50 p-4 dark:border-indigo-900 dark:bg-indigo-950/40">
          <p className="text-sm text-slate-800 dark:text-slate-200">
            1. Abre{' '}
            <a href={login.verification_uri} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 font-medium text-brand-700 underline dark:text-indigo-300">
              {login.verification_uri.replace('https://', '')} <ExternalLink className="size-3.5" aria-hidden />
            </a>
          </p>
          <p className="text-sm text-slate-800 dark:text-slate-200">2. Escreve este código e entra com a tua conta do ISEP:</p>
          <p aria-label="Código de autorização" className="select-all font-mono text-2xl font-semibold tracking-widest text-slate-900 dark:text-slate-100">
            {login.user_code}
          </p>
          <p className="text-xs text-slate-500">À espera da tua autorização…</p>
        </div>
      ) : (
        <Button onClick={() => void start()} loading={busy} icon={<Mail className="size-4" />}>
          {connected ? 'Ligar outra vez' : 'Ligar a conta Microsoft do ISEP'}
        </Button>
      )}
      {error && <Notice tone="bad">{error}</Notice>}
    </div>
  )
}
