import { CalendarClock, Check } from 'lucide-react'
import { useEffect, useState } from 'react'
import { ApiError } from '../api'
import type { SessionState, Settings } from '../api'
import { EmailSetup } from '../components/EmailSetup'
import { MoodleCredentials } from '../components/MoodleCredentials'
import { MoodleSetup } from '../components/MoodleSetup'
import { validateRules } from '../components/RulesEditor'
import { SessionPanel } from '../components/SessionPanel'
import { StudentFields, validateStudent } from '../components/StudentFields'
import type { StudentValue } from '../components/StudentFields'
import { Button, Card, Field, Notice, Spinner, TextInput, cx } from '../components/ui'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'
import { SummarySection, TestSection, buildPatch, toDraft } from './SettingsPage'
import type { Draft } from './SettingsPage'

const STEPS = ['O teu curso', 'Email', 'Moodle', 'Session', 'Resumo', 'Agendamento'] as const

/** Primeiro arranque: leva o utilizador do zero a um resumo diário a funcionar, sem terminal. */
export function SetupWizard({ onDone }: { onDone: () => void }) {
  const { api } = useApp()
  const settings = useAsync<Settings>(() => api.settings(), [api])
  const [step, setStep] = useState(0)
  const [student, setStudent] = useState<StudentValue | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [sessionOk, setSessionOk] = useState<boolean | null>(null)
  const [time, setTime] = useState<string | null>(null)
  const [scheduled, setScheduled] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({})

  useEffect(() => {
    if (!settings.data) return
    setStudent((cur) => cur ?? toDraft(settings.data!).student)
    setDraft((cur) => cur ?? toDraft(settings.data!))
  }, [settings.data])

  if (!settings.data || !student || !draft) {
    return (
      <div className="grid h-full place-items-center" role="status">
        <div className="flex items-center gap-2 text-sm text-slate-500">
          <Spinner /> A preparar a configuração…
        </div>
      </div>
    )
  }

  const s = settings.data
  const chosenTime = time ?? s.schedule.time
  const sessionReady = sessionOk ?? s.destination.session_configured
  const emailReady = s.email.method === 'imap' && s.email.connected
  const reload = () => void settings.reload()
  const next = () => {
    setError(null)
    setFieldErrors({})
    setStep((n) => Math.min(n + 1, STEPS.length - 1))
  }
  const back = () => {
    setError(null)
    setFieldErrors({})
    setStep((n) => Math.max(n - 1, 0))
  }

  const run = async (work: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    setFieldErrors({})
    try {
      await work()
    } catch (e) {
      if (e instanceof ApiError && e.status === 422 && Object.keys(e.fields).length) setFieldErrors(e.fields)
      else setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  const saveStudent = () => {
    const local = validateStudent(student)
    if (Object.keys(local).length) {
      setFieldErrors(local)
      return Promise.resolve()
    }
    return run(async () => {
      await api.updateSettings({
        student: {
          institution_id: student.institutionId,
          ...(student.institutionId === 'outra' ? { institution_name: student.institutionName.trim() } : {}),
          course: student.course.trim(),
          year: student.year,
        },
      })
      await settings.reload()
      next()
    })
  }

  const saveSummary = () => {
    const local = validateRules(draft.rules)
    if (Object.keys(local).length) {
      setFieldErrors(Object.fromEntries(Object.entries(local).map(([k, v]) => [`summary.rules.${k}`, v])))
      return Promise.resolve()
    }
    return run(async () => {
      const patch = buildPatch(s, draft)
      const only = { ...(patch.summary ? { summary: patch.summary } : {}) }
      if (Object.keys(only).length) await api.updateSettings(only)
      await settings.reload()
      next()
    })
  }

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

  const setD = (p: Partial<Draft>) => setDraft({ ...draft, ...p })

  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6 lg:p-10">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Vamos configurar o ISEP Assistant</h1>
        <p className="text-sm text-slate-500 dark:text-slate-400">São 6 passos e demora poucos minutos. Podes mudar tudo depois nas Definições.</p>
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
        <Card title="Onde estudas?" subtitle="Escolhe a instituição, o curso e o ano. Fica guardado só neste PC.">
          <div className="space-y-5">
            <StudentFields value={student} onChange={setStudent} errors={fieldErrors} />
            <div className="flex justify-end">
              <Button variant="primary" onClick={() => void saveStudent()} loading={busy}>
                Continuar
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 1 && (
        <div className="space-y-4">
          <Card title="O teu email" subtitle="A app lê os emails da tua escola diretamente, sem precisares de mais nenhum programa. Só lê; nunca apaga nem envia.">
            <EmailSetup email={s.email} onChanged={reload} />
          </Card>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={back}>
              Voltar
            </Button>
            <Button variant="primary" onClick={next}>
              {emailReady ? 'Continuar' : 'Configurar mais tarde'}
            </Button>
          </div>
        </div>
      )}

      {step === 2 && (
        <div className="space-y-4">
          <MoodleSetup moodle={s.moodle} onChanged={reload} />
          <MoodleCredentials username="" onSaved={reload} />
          <p className="text-xs text-slate-500">Sem o Moodle, o resumo só inclui emails. Podes ligá-lo mais tarde.</p>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={back}>
              Voltar
            </Button>
            <Button variant="primary" onClick={next}>
              Continuar
            </Button>
          </div>
        </div>
      )}

      {step === 3 && (
        <div className="space-y-4">
          <Card title="Session" subtitle="É aqui que recebes o resumo.">
            <SessionPanel onChange={(st: SessionState) => setSessionOk(st.configured && st.node_ok)} />
          </Card>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={back}>
              Voltar
            </Button>
            <Button variant="primary" onClick={next} disabled={!sessionReady}>
              Continuar
            </Button>
          </div>
        </div>
      )}

      {step === 4 && (
        <div className="space-y-4">
          <SummarySection s={s} d={draft} set={setD} errors={fieldErrors} />
          <TestSection dirty={Object.keys(buildPatch(s, draft)).length > 0} mode={s.summary.mode} />
          <div className="flex justify-between">
            <Button variant="ghost" onClick={back}>
              Voltar
            </Button>
            <Button variant="primary" onClick={() => void saveSummary()} loading={busy}>
              Continuar
            </Button>
          </div>
        </div>
      )}

      {step === 5 && (
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
              <Button variant="ghost" onClick={back}>
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
