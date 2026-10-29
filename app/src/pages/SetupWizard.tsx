import { Bot, CalendarClock, Check, Mail, Shield } from 'lucide-react'
import { useState } from 'react'
import { ApiError } from '../api'
import type { EmailMethod, SessionState, SetupState, SummaryMode } from '../api'
import { EmailConnect } from '../components/EmailConnect'
import { MoodleCredentials } from '../components/MoodleCredentials'
import { SessionPanel } from '../components/SessionPanel'
import { Button, Card, Choice, Field, Notice, Spinner, TextInput, cx } from '../components/ui'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'

const STEPS = ['Emails', 'Moodle', 'Session', 'Resumo', 'Agendamento'] as const

/** Primeiro arranque: leva o utilizador do zero a um resumo diário a funcionar, sem terminal. */
export function SetupWizard({ onDone }: { onDone: () => void }) {
  const { api } = useApp()
  const setup = useAsync<SetupState>(() => api.setup(), [api])
  const settings = useAsync(() => api.settings(), [api])
  const [step, setStep] = useState(0)
  const [method, setMethod] = useState<EmailMethod | null>(null)
  const [address, setAddress] = useState('')
  const [mode, setMode] = useState<SummaryMode | null>(null)
  const [time, setTime] = useState<string | null>(null)
  const [sessionOk, setSessionOk] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [scheduled, setScheduled] = useState<string | null>(null)

  if (!setup.data || !settings.data) {
    return (
      <div className="grid h-full place-items-center" role="status">
        <div className="flex items-center gap-2 text-sm text-slate-500">
          <Spinner /> A preparar a configuração…
        </div>
      </div>
    )
  }

  const s = settings.data
  const chosenMethod = method ?? s.email.method
  const chosenMode = mode ?? s.summary.mode
  const chosenTime = time ?? s.schedule.time
  const thunderbirdOk = s.email.method === 'thunderbird' && (setup.data.steps.find((c) => c.id === 'email')?.ok ?? false)
  const sessionReady = sessionOk ?? setup.data.steps.find((c) => c.id === 'session')?.ok ?? false

  const run = async (work: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await work()
    } catch (e) {
      setError(e instanceof ApiError ? (Object.values(e.fields)[0] ?? e.message) : String(e))
    } finally {
      setBusy(false)
    }
  }

  const next = () => setStep((n) => Math.min(n + 1, STEPS.length - 1))

  const saveEmail = () =>
    run(async () => {
      await api.updateSettings({ email: { method: chosenMethod, ...(address.trim() ? { address: address.trim() } : {}) } })
      await settings.reload()
      next()
    })

  const saveMode = () =>
    run(async () => {
      await api.updateSettings({ summary: { mode: chosenMode } })
      next()
    })

  const activate = () =>
    run(async () => {
      await api.updateSettings({ schedule: { time: chosenTime } })
      const r = await api.registerSchedule()
      if (!r.ok) throw new ApiError(0, r.message)
      setScheduled(r.message)
    })

  const finish = () =>
    run(async () => {
      await api.updateSettings({ setup: { done: true } })
      onDone()
    })

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6 lg:p-10">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Vamos configurar o ISEP Assistant</h1>
        <p className="text-sm text-slate-500 dark:text-slate-400">São 5 passos e demora poucos minutos. Podes mudar tudo depois nas Definições.</p>
      </header>

      <nav aria-label="Passos da configuração">
        <ol className="flex flex-wrap gap-2">
          {STEPS.map((label, i) => (
            <li
              key={label}
              aria-current={i === step ? 'step' : undefined}
              className={cx(
                'flex items-center gap-2 rounded-full px-3 py-1 text-xs font-medium',
                i === step ? 'bg-brand-600 text-white' : i < step ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-400',
              )}
            >
              {i < step ? <Check className="size-3.5" aria-hidden /> : <span aria-hidden>{i + 1}</span>}
              {label}
            </li>
          ))}
        </ol>
      </nav>

      {error && <Notice tone="bad">{error}</Notice>}

      {step === 0 && (
        <Card title="De onde vêm os teus emails?" subtitle="A app só lê; nunca apaga nem envia emails.">
          <div className="space-y-5">
            <Choice<EmailMethod>
              legend="Origem dos emails"
              value={chosenMethod}
              onChange={setMethod}
              options={[
                { value: 'thunderbird', title: 'Thunderbird (recomendado)', description: 'Usa o Thunderbird que já tens com a conta do ISEP. Funciona sem aprovações.', icon: <Mail className="size-5" /> },
                { value: 'graph', title: 'Conta do ISEP (experimental)', description: 'Liga-se diretamente à conta Microsoft, sem Thunderbird. Pode exigir aprovação de um administrador do ISEP.', icon: <Mail className="size-5" /> },
              ]}
            />
            {chosenMethod === 'graph' ? (
              <EmailConnect connected={s.email.connected} onConnected={() => void settings.reload()} />
            ) : (
              <Notice tone={thunderbirdOk ? 'good' : 'info'}>
                {thunderbirdOk ? 'Thunderbird encontrado.' : 'Confirma que o Thunderbird está instalado e com a conta do ISEP configurada.'}
              </Notice>
            )}
            <Field label="O teu email do ISEP (opcional)">
              {(p) => <TextInput {...p} type="email" value={address} onChange={(e) => setAddress(e.target.value)} placeholder={s.email.address || 'numero@isep.ipp.pt'} />}
            </Field>
            <div className="flex justify-end">
              <Button variant="primary" onClick={() => void saveEmail()} loading={busy} disabled={chosenMethod === 'graph' && !s.email.connected}>
                Continuar
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 1 && (
        <div className="space-y-4">
          <MoodleCredentials username="" onSaved={() => void setup.reload()} />
          <p className="text-xs text-slate-500">Sem o Moodle, o resumo só inclui emails. Podes ligá-lo mais tarde.</p>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={() => setStep(0)}>
              Voltar
            </Button>
            <Button variant="primary" onClick={next}>
              {setup.data.steps.find((c) => c.id === 'moodle')?.ok ? 'Continuar' : 'Saltar este passo'}
            </Button>
          </div>
        </div>
      )}

      {step === 2 && (
        <div className="space-y-4">
          <Card title="Session" subtitle="É aqui que recebes o resumo.">
            <SessionPanel onChange={(st: SessionState) => setSessionOk(st.configured && st.node_ok)} />
          </Card>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={() => setStep(1)}>
              Voltar
            </Button>
            <Button variant="primary" onClick={next} disabled={!sessionReady}>
              Continuar
            </Button>
          </div>
        </div>
      )}

      {step === 3 && (
        <Card title="Como queres o resumo?" subtitle="Podes afinar a IA mais tarde nas Definições.">
          <div className="space-y-5">
            <Choice<SummaryMode>
              legend="Tipo de resumo"
              value={chosenMode}
              onChange={setMode}
              options={[
                { value: 'rules', title: 'Regras', description: 'Local e privado: os emails nunca saem do teu PC.', icon: <Shield className="size-5" /> },
                { value: 'ai', title: 'IA', description: 'Um modelo reescreve o resumo (mais natural). Configuras a ligação nas Definições.', icon: <Bot className="size-5" /> },
              ]}
            />
            <div className="flex justify-between">
              <Button variant="ghost" onClick={() => setStep(2)}>
                Voltar
              </Button>
              <Button variant="primary" onClick={() => void saveMode()} loading={busy}>
                Continuar
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 4 && (
        <Card title="Quando queres receber o resumo?" subtitle="Corre todos os dias a esta hora (e ao retomar o PC da suspensão), mesmo com a app fechada.">
          <div className="space-y-5">
            <div className="max-w-xs">
              <Field label="Hora do resumo diário">{(p) => <TextInput {...p} type="time" value={chosenTime} onChange={(e) => setTime(e.target.value)} />}</Field>
            </div>
            {scheduled ? (
              <Notice tone="good">{scheduled}</Notice>
            ) : (
              <Button onClick={() => void activate()} loading={busy} icon={<CalendarClock className="size-4" />}>
                Ativar agendamento
              </Button>
            )}
            <div className="flex justify-between">
              <Button variant="ghost" onClick={() => setStep(3)}>
                Voltar
              </Button>
              <Button variant="primary" onClick={() => void finish()} loading={busy} disabled={!scheduled}>
                Concluir
              </Button>
            </div>
          </div>
        </Card>
      )}
    </div>
  )
}
