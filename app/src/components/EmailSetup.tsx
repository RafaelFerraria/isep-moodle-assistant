import { CheckCircle2, KeyRound, LogIn } from 'lucide-react'
import { useEffect, useState } from 'react'
import { ApiError } from '../api'
import type { Settings } from '../api'
import { useApp } from '../lib/context'
import { Button, Field, Notice, TextInput } from './ui'

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

/** Servidor IMAP habitual do fornecedor do email (a escola usa o Microsoft 365). Vazio = pedir ao utilizador. */
export function defaultImapHost(address: string): string {
  const domain = address.split('@')[1]?.trim().toLowerCase() ?? ''
  const microsoft = ['isep.ipp.pt', 'outlook.com', 'hotmail.com', 'live.com', 'office365.com']
  if (microsoft.some((d) => domain === d || domain.endsWith(`.${d}`))) return 'outlook.office365.com'
  if (domain === 'gmail.com' || domain === 'googlemail.com') return 'imap.gmail.com'
  return ''
}

/**
 * Email da escola, tudo dentro da app (como o Session): email, palavra-passe e servidor. A app lê a caixa sozinha por IMAP,
 * sem Thunderbird nem janelas de terceiros. A palavra-passe fica cifrada neste PC e nunca volta ao ecrã.
 */
export function EmailSetup({ email, onChanged }: { email: Settings['email']; onChanged: () => void }) {
  const { api } = useApp()
  const [address, setAddress] = useState(email.address)
  const [password, setPassword] = useState('')
  const [host, setHost] = useState(email.imap_host)
  const [port, setPort] = useState(String(email.imap_port || 993))
  const [hostEdited, setHostEdited] = useState(!!email.imap_host)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null)
  const [manualUrl, setManualUrl] = useState<string | null>(null)
  const [pasted, setPasted] = useState('')
  useEffect(() => setAddress(email.address), [email.address])

  const validAddress = EMAIL_RE.test(address.trim())
  const shownHost = hostEdited ? host : defaultImapHost(address) || host
  const portNumber = Number(port)
  const validPort = Number.isInteger(portNumber) && portNumber >= 1 && portNumber <= 65535
  const haveSecret = !!password || email.password_set
  const ready = validAddress && haveSecret && !!shownHost.trim() && validPort

  const fail = (e: unknown) => {
    const fields = e instanceof ApiError ? e.fields : {}
    setError(Object.values(fields)[0] ?? (e instanceof ApiError ? e.message : String(e)))
  }

  const saveAndTest = async () => {
    setBusy(true)
    setError(null)
    setResult(null)
    try {
      await api.updateSettings({
        email: { method: 'imap', address: address.trim(), imap_host: shownHost.trim(), imap_port: portNumber, ...(password ? { password } : {}) },
      })
      setPassword('')
      const t = await api.testEmail()
      setResult(
        t.ok
          ? { ok: true, text: `Ligado. Encontrei ${t.count ?? 0} email(s) nos últimos 3 dias.` }
          : { ok: false, text: t.error ?? 'Não foi possível ligar ao email.' },
      )
      onChanged()
    } catch (e) {
      fail(e)
    } finally {
      setBusy(false)
    }
  }

  const clearPassword = () => {
    setBusy(true)
    setError(null)
    api
      .updateSettings({ email: { clear_password: true } })
      .then(() => onChanged())
      .catch(fail)
      .finally(() => setBusy(false))
  }

  const connected = email.method === 'imap' && email.connected
  const isMicrosoft = validAddress && defaultImapHost(address) === 'outlook.office365.com'
  const bridge = typeof window !== 'undefined' ? window.isepBridge : undefined

  // termina o início de sessão com o endereço para onde a Microsoft devolveu o utilizador e testa a caixa
  const finishSignIn = async (redirectUrl: string) => {
    const r = await api.finishEmailOAuth(address.trim(), redirectUrl)
    if (!r.ok) throw new ApiError(0, r.error ?? 'O início de sessão falhou.')
    setManualUrl(null)
    setPasted('')
    const t = await api.testEmail()
    setResult(t.ok ? { ok: true, text: `Ligado. Encontrei ${t.count ?? 0} email(s) nos últimos 3 dias.` } : { ok: false, text: t.error ?? 'Não foi possível ler os emails.' })
    onChanged()
  }

  const signIn = async () => {
    setBusy(true)
    setError(null)
    setResult(null)
    try {
      const start = await api.startEmailOAuth(address.trim())
      if (bridge?.oauthLogin) {
        const url = await bridge.oauthLogin(start.auth_url, start.redirect_uri)
        if (!url) throw new ApiError(0, 'Cancelaste o início de sessão.')
        await finishSignIn(url)
      } else {
        // sem a app de secretária (por exemplo no navegador): a página abre à parte e o utilizador cola o endereço de regresso
        setManualUrl(start.auth_url)
      }
    } catch (e) {
      fail(e)
    } finally {
      setBusy(false)
    }
  }

  const submitPasted = async () => {
    setBusy(true)
    setError(null)
    try {
      await finishSignIn(pasted.trim())
    } catch (e) {
      fail(e)
    } finally {
      setBusy(false)
    }
  }

  const signOut = () => {
    setBusy(true)
    setError(null)
    api
      .updateSettings({ email: { sign_out: true } })
      .then(() => onChanged())
      .catch(fail)
      .finally(() => setBusy(false))
  }

  return (
    <div className="space-y-4">
      {email.method === 'thunderbird' && (
        <Notice tone="info">Estás a ler os emails pelo Thunderbird (configuração antiga). Preenche o email e a palavra-passe aqui para a app os ler sozinha, sem Thunderbird.</Notice>
      )}
      {connected && !result && (
        <Notice tone="good">
          <span className="flex items-center gap-2">
            <CheckCircle2 className="size-4" aria-hidden /> Email ligado. A app lê a tua caixa diretamente e nunca apaga nem envia nada.
          </span>
        </Notice>
      )}

      <div className="max-w-md">
        <Field label="O teu email da escola" hint="Ex.: numero@isep.ipp.pt" error={address && !validAddress ? 'Endereço de email inválido.' : undefined}>
          {(p) => <TextInput {...p} type="email" value={address} onChange={(e) => setAddress(e.target.value)} autoComplete="email" />}
        </Field>
      </div>

      {isMicrosoft && (
        <div className="space-y-3 rounded-xl border border-brand-200 bg-brand-50 p-4 dark:border-indigo-900 dark:bg-indigo-950/40">
          <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">Entrar com a conta da escola (recomendado)</p>
          <p className="text-xs text-slate-600 dark:text-slate-400">
            Abre a página de entrada da Microsoft numa janela desta app, como no Thunderbird. A app não vê a tua palavra-passe: só recebe uma autorização, que fica guardada cifrada.
          </p>
          {email.oauth_connected && (
            <Notice tone="good">
              <span className="flex items-center gap-2">
                <CheckCircle2 className="size-4" aria-hidden /> Conta da escola ligada.
              </span>
            </Notice>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <Button variant="primary" onClick={() => void signIn()} loading={busy} icon={<LogIn className="size-4" />}>
              {email.oauth_connected ? 'Entrar de novo' : 'Entrar com a conta da escola'}
            </Button>
            {email.oauth_connected && (
              <Button variant="ghost" disabled={busy} onClick={signOut}>
                Terminar sessão
              </Button>
            )}
          </div>
          {manualUrl && (
            <div className="space-y-2 text-sm">
              <p>
                Abre{' '}
                <a href={manualUrl} target="_blank" rel="noreferrer" className="font-medium text-brand-700 underline dark:text-indigo-300">
                  a página de entrada da Microsoft
                </a>
                , entra e, no fim, copia o endereço que aparece na barra (começa por https://localhost) e cola-o aqui:
              </p>
              <Field label="Endereço de regresso">{(p) => <TextInput {...p} value={pasted} onChange={(e) => setPasted(e.target.value)} spellCheck={false} placeholder="https://localhost/?code=…" />}</Field>
              <Button onClick={() => void submitPasted()} loading={busy} disabled={!pasted.trim().startsWith('https://localhost')}>
                Concluir
              </Button>
            </div>
          )}
        </div>
      )}

      <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">{isMicrosoft ? 'Ou com palavra-passe' : 'Entrar com palavra-passe'}</p>
      {isMicrosoft && <p className="-mt-2 text-xs text-slate-500">Muitas escolas com Microsoft 365 já não aceitam este método («Basic authentication is disabled»). Se acontecer, usa o botão acima.</p>}
      <div className="grid gap-4 sm:grid-cols-2">
        <Field
          label="Palavra-passe do email"
          hint={email.password_set ? 'Já está guardada (cifrada). Deixa em branco para a manter.' : 'Fica guardada cifrada neste PC. Se a escola exigir uma palavra-passe de aplicação, usa essa.'}
        >
          {(p) => <TextInput {...p} type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="current-password" />}
        </Field>
        <Field label="Servidor IMAP" hint={defaultImapHost(address) ? 'Preenchido pelo fornecedor do teu email.' : 'Ex.: outlook.office365.com'}>
          {(p) => (
            <TextInput
              {...p}
              value={shownHost}
              onChange={(e) => {
                setHost(e.target.value)
                setHostEdited(true)
              }}
              spellCheck={false}
            />
          )}
        </Field>
        <Field label="Porta" error={port && !validPort ? 'Entre 1 e 65535.' : undefined}>
          {(p) => <TextInput {...p} inputMode="numeric" value={port} onChange={(e) => setPort(e.target.value)} />}
        </Field>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="primary" onClick={() => void saveAndTest()} loading={busy} disabled={!ready} icon={<KeyRound className="size-4" />}>
          Guardar e testar a ligação
        </Button>
        {email.password_set && (
          <Button variant="ghost" disabled={busy} onClick={clearPassword}>
            Apagar a palavra-passe guardada
          </Button>
        )}
      </div>

      {result && <Notice tone={result.ok ? 'good' : 'bad'}>{result.text}</Notice>}
      {error && <Notice tone="bad">{error}</Notice>}
    </div>
  )
}
