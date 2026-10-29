import { useEffect, useState } from 'react'
import { ApiError } from '../api'
import type { Settings } from '../api'
import { useApp } from '../lib/context'
import { EmailConnect } from './EmailConnect'
import { Button, Field, Notice, TextInput } from './ui'

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

/**
 * Email da escola, tudo dentro da app: o utilizador escreve o endereço, autoriza uma vez (código de dispositivo) e a app
 * lê a caixa por IMAP. Guarda por si, sem passar pelo botão «Guardar» das Definições.
 */
export function EmailSetup({ email, onChanged }: { email: Settings['email']; onChanged: () => void }) {
  const { api } = useApp()
  const [address, setAddress] = useState(email.address)
  const [error, setError] = useState<string | null>(null)
  const [migrating, setMigrating] = useState(false)
  useEffect(() => setAddress(email.address), [email.address])

  const valid = EMAIL_RE.test(address.trim())
  const legacy = email.method === 'thunderbird' && !email.connected

  // grava o endereço e passa a usar o serviço da app antes de pedir o código
  const prepare = async () => {
    setError(null)
    if (!valid) throw new ApiError(0, 'Escreve o teu email da escola completo (ex.: numero@escola.pt).')
    try {
      await api.updateSettings({ email: { method: 'imap', address: address.trim() } })
    } catch (e) {
      const msg = e instanceof ApiError ? (e.fields['email.address'] ?? e.message) : String(e)
      setError(msg)
      throw e
    }
  }

  return (
    <div className="space-y-4">
      {legacy && (
        <Notice tone="info">
          <span className="block">Estás a ler os emails pelo Thunderbird (configuração anterior). Podes passar a ler diretamente a partir da app.</span>
          <Button className="mt-2" onClick={() => setMigrating(true)} variant="secondary">
            Passar a usar só a app
          </Button>
        </Notice>
      )}
      {(!legacy || migrating) && (
        <>
          <div className="max-w-md">
            <Field label="O teu email da escola" error={address && !valid ? 'Endereço de email inválido.' : undefined} hint="Ex.: numero@isep.ipp.pt. A app só lê; nunca apaga nem envia.">
              {(p) => <TextInput {...p} type="email" value={address} onChange={(e) => setAddress(e.target.value)} autoComplete="email" />}
            </Field>
          </div>
          <EmailConnect connected={email.connected && email.method === 'imap'} disabled={!valid} beforeStart={prepare} onConnected={onChanged} />
        </>
      )}
      {error && <Notice tone="bad">{error}</Notice>}
    </div>
  )
}

