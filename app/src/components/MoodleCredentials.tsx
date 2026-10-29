import { useEffect, useState } from 'react'
import { ApiError } from '../api'
import { useApp } from '../lib/context'
import { Button, Card, Field, Notice, TextInput } from './ui'

/** Valida o login no Moodle e guarda a palavra-passe cifrada pelo servidor. Usado nas Definições e no assistente. */
export function MoodleCredentials({ username, onSaved }: { username: string; onSaved: () => void }) {
  const { api } = useApp()
  const [user, setUser] = useState(username)
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{ tone: 'good' | 'bad'; text: string } | null>(null)
  useEffect(() => setUser(username), [username])
  const save = async () => {
    setBusy(true)
    setMessage(null)
    try {
      const r = await api.setMoodleCredentials(user.trim(), password)
      setPassword('')
      setMessage({ tone: 'good', text: `Credenciais validadas e guardadas (${r.username}).` })
      onSaved()
    } catch (e) {
      setMessage({ tone: 'bad', text: e instanceof ApiError ? e.message : String(e) })
    } finally {
      setBusy(false)
    }
  }
  return (
    <Card title="Conta do Moodle" subtitle="A palavra-passe é validada no Moodle e guardada cifrada com o Windows (DPAPI). Nunca é mostrada.">
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Número mecanográfico">{(p) => <TextInput {...p} value={user} onChange={(e) => setUser(e.target.value)} autoComplete="username" />}</Field>
        <Field label="Palavra-passe do Moodle">{(p) => <TextInput {...p} type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="current-password" />}</Field>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <Button onClick={() => void save()} loading={busy} disabled={!user.trim() || !password}>
          Validar e guardar
        </Button>
        {message && <Notice tone={message.tone}>{message.text}</Notice>}
      </div>
    </Card>
  )
}
