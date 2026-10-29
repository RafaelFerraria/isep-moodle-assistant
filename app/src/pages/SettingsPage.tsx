import { Bell, Bot, CheckCircle2, CircleDashed, Cpu, KeyRound, Save, Shield, TestTube2, Undo2, XCircle } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { ApiError } from '../api'
import type { AiApiType, AiCli, AiProvider, Check, Settings, SettingsPatch, SettingsUpdateResult, SummaryMode, SummaryTestResult } from '../api'
import { EmailSetup } from '../components/EmailSetup'
import { MoodleCredentials } from '../components/MoodleCredentials'
import { MoodleSetup } from '../components/MoodleSetup'
import { RulesEditor, textToList, rulesToText, validateRules } from '../components/RulesEditor'
import type { RulesText } from '../components/RulesEditor'
import { StudentFields, validateStudent } from '../components/StudentFields'
import type { StudentValue } from '../components/StudentFields'
import { Badge, Button, Card, Choice, Field, Notice, Pre, Select, Spinner, TextInput } from '../components/ui'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'

// ------------------------------------------------------------------ rascunho <-> definições

export interface Draft {
  mode: SummaryMode
  aiProvider: AiProvider
  aiCli: AiCli
  aiCliCommand: string
  aiApiType: AiApiType
  aiModel: string
  aiBaseUrl: string
  newApiKey: string
  clearApiKey: boolean
  time: string
  rules: RulesText
  student: StudentValue
  sessionId: string
  syncDir: string
}

export function toDraft(s: Settings): Draft {
  const cmd = s.summary.ai_cli_command
  return {
    mode: s.summary.mode,
    aiProvider: s.summary.ai_provider,
    aiCli: s.summary.ai_cli,
    aiCliCommand: Array.isArray(cmd) ? cmd.join(' ') : cmd,
    aiApiType: s.summary.ai_api_type,
    aiModel: s.summary.ai_model,
    aiBaseUrl: s.summary.ai_base_url,
    newApiKey: '',
    clearApiKey: false,
    time: s.schedule.time,
    rules: rulesToText(s.summary.rules),
    student: { institutionId: s.student.institution_id, institutionName: s.student.institution_id === 'outra' ? s.student.institution_name : '', course: s.student.course, year: s.student.year },
    sessionId: '',
    syncDir: s.moodle.sync_dir,
  }
}

/** Só envia o que mudou (e os segredos só quando o utilizador os escreveu). */
export function buildPatch(s: Settings, d: Draft): SettingsPatch {
  const patch: SettingsPatch = {}
  const summary: NonNullable<SettingsPatch['summary']> = {}
  const cur = toDraft(s)
  if (d.mode !== cur.mode) summary.mode = d.mode
  if (d.aiProvider !== cur.aiProvider) summary.ai_provider = d.aiProvider
  if (d.aiCli !== cur.aiCli) summary.ai_cli = d.aiCli
  if (d.aiCliCommand !== cur.aiCliCommand) summary.ai_cli_command = d.aiCliCommand
  if (d.aiApiType !== cur.aiApiType) summary.ai_api_type = d.aiApiType
  if (d.aiModel !== cur.aiModel) summary.ai_model = d.aiModel
  if (d.aiBaseUrl !== cur.aiBaseUrl) summary.ai_base_url = d.aiBaseUrl
  if (d.newApiKey.trim()) summary.ai_api_key = d.newApiKey.trim()
  else if (d.clearApiKey) summary.clear_ai_api_key = true
  if (d.time !== cur.time) patch.schedule = { time: d.time }
  const rules: NonNullable<NonNullable<SettingsPatch['summary']>['rules']> = {}
  for (const key of ['important_keywords', 'ignore_senders', 'ignore_subjects'] as const) {
    const next = textToList(d.rules[key])
    if (JSON.stringify(next) !== JSON.stringify(textToList(cur.rules[key]))) rules[key] = next
  }
  if (Object.keys(rules).length) summary.rules = rules
  if (Object.keys(summary).length) patch.summary = summary
  const student: NonNullable<SettingsPatch['student']> = {}
  if (d.student.institutionId !== cur.student.institutionId) student.institution_id = d.student.institutionId
  if (d.student.institutionId === 'outra' && d.student.institutionName.trim() !== cur.student.institutionName.trim()) student.institution_name = d.student.institutionName.trim()
  if (d.student.course.trim() !== cur.student.course.trim()) student.course = d.student.course.trim()
  if (d.student.year !== cur.student.year) student.year = d.student.year
  if (Object.keys(student).length) patch.student = student
  if (d.sessionId.trim()) patch.destination = { session_id: d.sessionId.trim() }
  const moodle: NonNullable<SettingsPatch['moodle']> = {}
  if (d.syncDir !== cur.syncDir) moodle.sync_dir = d.syncDir
  if (Object.keys(moodle).length) patch.moodle = moodle
  return patch
}

const DEFAULT_MODEL: Record<AiApiType, string> = {
  openai: 'gpt-4o-mini',
  anthropic: 'claude-haiku-5-5',
  gemini: 'gemini-2.0-flash',
  openai_compatible: '',
}

// ------------------------------------------------------------------ secções

export function SummarySection({ s, d, set, errors }: { s: Settings; d: Draft; set: (p: Partial<Draft>) => void; errors: Record<string, string> }) {
  const keyStored = s.summary.ai_api_key_set && !d.clearApiKey
  const [replacing, setReplacing] = useState(false)
  return (
    <Card title="Resumo" subtitle="Como é escrito o resumo diário.">
      <div className="space-y-5">
        <Choice<SummaryMode>
          legend="Modo de resumo"
          value={d.mode}
          onChange={(mode) => set({ mode })}
          options={[
            { value: 'rules', title: 'Regras', description: 'Local e privado: os emails nunca saem do teu PC.', icon: <Shield className="size-5" /> },
            { value: 'ai', title: 'IA', description: 'Um modelo reescreve o resumo (mais natural).', icon: <Bot className="size-5" /> },
          ]}
        />
        {errors['summary.mode'] && <p role="alert" className="text-xs text-rose-600">{errors['summary.mode']}</p>}

        {d.mode === 'rules' && <RulesEditor value={d.rules} onChange={(rules) => set({ rules })} serverErrors={errors} />}

        {d.mode === 'ai' && (
          <div className="space-y-5 border-t border-slate-100 pt-5 dark:border-slate-800">
            <Notice tone="warn">Com IA, o assunto e o início do texto dos emails são enviados ao fornecedor escolhido.</Notice>
            <Choice<AiProvider>
              legend="Como ligar à IA"
              value={d.aiProvider}
              onChange={(aiProvider) => set({ aiProvider })}
              options={[
                { value: 'cli', title: 'Conta com login (CLI)', description: 'Usa uma ferramenta onde já fizeste login. Sem token.', icon: <Cpu className="size-5" /> },
                { value: 'api', title: 'API com token', description: 'OpenAI, Anthropic, Google ou compatível.', icon: <KeyRound className="size-5" /> },
              ]}
            />

            {d.aiProvider === 'cli' ? (
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Ferramenta (CLI)" error={errors['summary.ai_cli']}>
                  {(p) => (
                    <Select {...p} value={d.aiCli} onChange={(e) => set({ aiCli: e.target.value as AiCli })}>
                      <option value="claude">Claude Code (conta Claude)</option>
                      <option value="gemini">Gemini CLI / Antigravity (conta Google)</option>
                      <option value="codex">Codex CLI (conta ChatGPT)</option>
                      <option value="custom">Outro comando…</option>
                    </Select>
                  )}
                </Field>
                {d.aiCli === 'custom' && (
                  <Field label="Comando" hint="O texto é enviado por stdin. Ex.: meu-comando -p" error={errors['summary.ai_cli_command']}>
                    {(p) => <TextInput {...p} value={d.aiCliCommand} onChange={(e) => set({ aiCliCommand: e.target.value })} placeholder="meu-comando -p" />}
                  </Field>
                )}
              </div>
            ) : (
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Fornecedor da API" error={errors['summary.ai_api_type']}>
                  {(p) => (
                    <Select {...p} value={d.aiApiType} onChange={(e) => set({ aiApiType: e.target.value as AiApiType })}>
                      <option value="openai">OpenAI (ChatGPT)</option>
                      <option value="anthropic">Anthropic (Claude)</option>
                      <option value="gemini">Google (Gemini)</option>
                      <option value="openai_compatible">Compatível com OpenAI</option>
                    </Select>
                  )}
                </Field>
                <Field label="Modelo" error={errors['summary.ai_model']} hint={DEFAULT_MODEL[d.aiApiType] ? `Predefinido: ${DEFAULT_MODEL[d.aiApiType]}` : undefined}>
                  {(p) => <TextInput {...p} value={d.aiModel} onChange={(e) => set({ aiModel: e.target.value })} placeholder={DEFAULT_MODEL[d.aiApiType]} />}
                </Field>
                {d.aiApiType === 'openai_compatible' && (
                  <div className="sm:col-span-2">
                    <Field label="URL do endpoint" error={errors['summary.ai_base_url']} hint="Ex.: https://api.exemplo.com/v1/chat/completions">
                      {(p) => <TextInput {...p} value={d.aiBaseUrl} onChange={(e) => set({ aiBaseUrl: e.target.value })} placeholder="https://…/v1/chat/completions" />}
                    </Field>
                  </div>
                )}
                <div className="sm:col-span-2">
                  {keyStored && !replacing ? (
                    <div className="flex flex-wrap items-center gap-3 rounded-xl bg-slate-50 px-4 py-3 text-sm dark:bg-slate-800/60">
                      <KeyRound className="size-4 text-emerald-600" aria-hidden />
                      <span>Token guardado (por segurança não é mostrado).</span>
                      <Button variant="ghost" onClick={() => setReplacing(true)}>Substituir</Button>
                      <Button variant="danger" onClick={() => set({ clearApiKey: true })}>Remover</Button>
                    </div>
                  ) : (
                    <Field
                      label="Token de API"
                      error={errors['summary.ai_api_key']}
                      hint={
                        s.summary.ai_api_key_env_set
                          ? `A variável ${s.summary.ai_api_key_env} está definida; podes deixar em branco.`
                          : `Guardado só neste PC. Em alternativa, define a variável de ambiente ${s.summary.ai_api_key_env}.`
                      }
                    >
                      {(p) => <TextInput {...p} type="password" autoComplete="off" value={d.newApiKey} onChange={(e) => set({ newApiKey: e.target.value, clearApiKey: false })} placeholder="sk-…" />}
                    </Field>
                  )}
                  {d.clearApiKey && <p className="mt-2 text-xs text-amber-700 dark:text-amber-400">O token vai ser removido ao guardar.</p>}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </Card>
  )
}

export function TestSection({ dirty, mode }: { dirty: boolean; mode: SummaryMode }) {
  const { api } = useApp()
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<SummaryTestResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const run = async () => {
    setBusy(true)
    setResult(null)
    setError(null)
    try {
      setResult(await api.testSummary())
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Card title="Testar" subtitle="Gera um resumo de exemplo com as definições guardadas (não lê emails nem envia nada).">
      <div className="space-y-3">
        <Button onClick={() => void run()} loading={busy} disabled={dirty} icon={<TestTube2 className="size-4" />}>
          {mode === 'ai' ? 'Testar ligação à IA' : 'Gerar resumo de exemplo'}
        </Button>
        {dirty && <p className="text-xs text-slate-500">Guarda as alterações antes de testar.</p>}
        {error && <Notice tone="bad">{error}</Notice>}
        {result && result.ai_fallback && <Notice tone="warn">A IA não respondeu; o resumo caiu para o modo Regras. Confirma o login da CLI ou o token.</Notice>}
        {result && result.ok && result.ai_requested && <Notice tone="good">A IA respondeu corretamente.</Notice>}
        {result && <Pre label="Resumo de exemplo">{result.text}</Pre>}
      </div>
    </Card>
  )
}

function NotificationsSection({ s }: { s: Settings }) {
  const soon = ['Email', 'SMS', 'WhatsApp']
  return (
    <Card title="Notificações" subtitle="Por onde recebes o resumo.">
      <ul aria-label="Canais de notificação" className="divide-y divide-slate-100 dark:divide-slate-800">
        <li className="flex items-center gap-3 py-3">
          <Bell className="size-5 text-brand-600" aria-hidden />
          <span className="flex-1 text-sm font-medium text-slate-900 dark:text-slate-100">
            Session
            <span className="block text-xs font-normal text-slate-500">
              {s.destination.session_configured ? `A enviar para ${s.destination.session_recipients.join(', ')}.` : 'Ainda não está configurado.'}{' '}
              <Link to="/session" className="font-medium text-brand-600 hover:underline dark:text-indigo-400">
                Configurar
              </Link>
            </span>
          </span>
          <Badge tone={s.destination.session_configured ? 'good' : 'warn'}>{s.destination.session_configured ? 'Ativo' : 'Por configurar'}</Badge>
        </li>
        {soon.map((name) => (
          <li key={name} className="flex items-center gap-3 py-3 opacity-70">
            <Bell className="size-5 text-slate-400" aria-hidden />
            <span className="flex-1 text-sm font-medium text-slate-700 dark:text-slate-300">
              {name}
              <span className="block text-xs font-normal text-slate-500">Previsto para a versão com servidor.</span>
            </span>
            <Badge>Em breve</Badge>
          </li>
        ))}
      </ul>
    </Card>
  )
}

function ChecksSection({ checks }: { checks: Check[] }) {
  return (
    <Card title="Verificações" subtitle="O que é preciso para o resumo correr sozinho.">
      <ul className="divide-y divide-slate-100 dark:divide-slate-800">
        {checks.map((c) => {
          const Icon = c.ok ? CheckCircle2 : c.required ? XCircle : CircleDashed
          const color = c.ok ? 'text-emerald-600' : c.required ? 'text-rose-600' : 'text-slate-400'
          return (
            <li key={c.id} className="flex items-start gap-3 py-2">
              <Icon className={`mt-0.5 size-5 shrink-0 ${color}`} aria-hidden />
              <div className="min-w-0 flex-1">
                <p className="text-sm text-slate-800 dark:text-slate-200">
                  {c.label}
                  <span className="sr-only">{c.ok ? ': ok' : c.required ? ': com problema' : ': opcional, não configurado'}</span>
                </p>
                {c.detail && <p className="text-xs text-slate-500 dark:text-slate-400">{c.detail}</p>}
              </div>
            </li>
          )
        })}
      </ul>
    </Card>
  )
}

// ------------------------------------------------------------------ página

export function SettingsPage() {
  const { api } = useApp()
  const settings = useAsync<Settings>(() => api.settings(), [api])
  const status = useAsync(() => api.status(), [api])
  const [draft, setDraft] = useState<Draft | null>(null)
  const [saving, setSaving] = useState(false)
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [result, setResult] = useState<SettingsUpdateResult | null>(null)
  const [failure, setFailure] = useState<string | null>(null)

  useEffect(() => {
    if (settings.data) setDraft(toDraft(settings.data))
  }, [settings.data])

  if (!settings.data || !draft) {
    return settings.error ? (
      <div className="p-8"><Notice tone="bad">Não foi possível carregar as definições: {settings.error.message}</Notice></div>
    ) : (
      <div className="flex items-center gap-2 p-8 text-sm text-slate-500" role="status"><Spinner /> A carregar as definições…</div>
    )
  }

  const s = settings.data
  const patch = buildPatch(s, draft)
  const dirty = Object.keys(patch).length > 0
  const set = (p: Partial<Draft>) => {
    setDraft({ ...draft, ...p })
    setResult(null)
  }

  const save = async () => {
    setSaving(true)
    setErrors({})
    setFailure(null)
    setResult(null)
    try {
      const r = await api.updateSettings(patch)
      setResult(r)
      await settings.reload()
      void status.reload()
    } catch (e) {
      if (e instanceof ApiError && e.status === 422) setErrors(e.fields)
      else setFailure(e instanceof ApiError ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  const localErrors = { ...validateRules(draft.rules), ...(draft.student.institutionId ? validateStudent(draft.student) : {}) }
  const invalid = Object.keys(localErrors).length > 0
  const shown = new Set(['summary.mode', 'summary.ai_cli', 'summary.ai_cli_command', 'summary.ai_api_type', 'summary.ai_model', 'summary.ai_base_url', 'summary.ai_api_key', 'schedule.time', 'destination.session_id', 'moodle.sync_dir', 'moodle.download_dir', 'email.method', 'email.address'])
  const orphan = Object.entries(errors).filter(([k]) => (!shown.has(k) && !k.startsWith('summary.rules.') && !k.startsWith('student.')) || (draft.mode === 'rules' && k.startsWith('summary.ai')))

  return (
    <div className="space-y-6 p-6 pb-28 lg:p-8 lg:pb-28">
      <header>
        <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Definições</h1>
        <p className="text-sm text-slate-500 dark:text-slate-400">As alterações só se aplicam depois de carregares em «Guardar».</p>
      </header>

      <Card title="O teu curso" subtitle="Instituição, curso e ano. Fica guardado só neste PC (mais tarde irá para a tua conta online).">
        <StudentFields value={draft.student} onChange={(student) => set({ student })} errors={errors} />
      </Card>

      <Card title="Email" subtitle="A app lê o email da tua escola diretamente, sem precisares de mais nenhum programa.">
        <EmailSetup email={s.email} onChanged={() => { void settings.reload(); void status.reload() }} />
      </Card>

      <SummarySection s={s} d={draft} set={set} errors={errors} />
      <TestSection dirty={dirty} mode={s.summary.mode} />

      <Card title="Agenda" subtitle="Hora a que o resumo corre todos os dias (e ao retomar da suspensão).">
        <div className="max-w-xs">
          <Field label="Hora do resumo diário" error={errors['schedule.time']}>
            {(p) => <TextInput {...p} type="time" value={draft.time} onChange={(e) => set({ time: e.target.value })} />}
          </Field>
        </div>
      </Card>

      <NotificationsSection s={s} />

      <Card title="Destino" subtitle="Onde recebes o resumo (Session).">
        <div className="space-y-3">
          <p className="text-sm text-slate-700 dark:text-slate-300">
            {s.destination.session_configured ? (
              <>Enviar para: <span className="font-mono">{s.destination.session_recipients.join(', ')}</span></>
            ) : (
              'Ainda não há destino configurado.'
            )}
          </p>
          <Field label="Novo Session ID" hint="66 caracteres, começa por 05. Deixa em branco para manter o atual." error={errors['destination.session_id']}>
            {(p) => <TextInput {...p} value={draft.sessionId} onChange={(e) => set({ sessionId: e.target.value })} spellCheck={false} className="font-mono" />}
          </Field>
        </div>
      </Card>

      <MoodleSetup moodle={s.moodle} onChanged={() => { void settings.reload(); void status.reload() }} />
      <MoodleCredentials username={status.data?.moodle.username ?? ''} onSaved={() => void status.reload()} />

      {status.data && <ChecksSection checks={status.data.checks} />}

      <Card title="Pastas" subtitle="Avançado.">
        <div className="max-w-xl">
          <Field label="Pasta do sincronizador" error={errors['moodle.sync_dir']}>
            {(p) => <TextInput {...p} value={draft.syncDir} onChange={(e) => set({ syncDir: e.target.value })} />}
          </Field>
        </div>
      </Card>

      <div className="fixed inset-x-0 bottom-0 z-10 border-t border-slate-200 bg-white/95 px-6 py-3 backdrop-blur dark:border-slate-800 dark:bg-slate-900/95 lg:left-64">
        <div className="mx-auto flex max-w-5xl flex-wrap items-center gap-3">
          <Button variant="primary" onClick={() => void save()} loading={saving} disabled={!dirty || invalid} icon={<Save className="size-4" />}>
            Guardar alterações
          </Button>
          <Button variant="ghost" onClick={() => { setDraft(toDraft(s)); setErrors({}); setResult(null) }} disabled={!dirty || saving} icon={<Undo2 className="size-4" />}>
            Descartar
          </Button>
          <div className="min-w-0 flex-1 text-sm">
            {failure && <span role="alert" className="text-rose-600">{failure}</span>}
            {Object.keys(errors).length > 0 && (
              <span role="alert" className="text-rose-600">
                Há campos inválidos.{orphan.length > 0 && ` ${orphan.map(([k, v]) => `${k}: ${v}`).join('; ')}`}
              </span>
            )}
            {result && !Object.keys(errors).length && (
              <span role="status" className="text-emerald-700 dark:text-emerald-400">
                Definições guardadas.
                {result.reschedule && (result.reschedule.ok ? ' Agenda do Windows atualizada.' : ` Não foi possível atualizar a agenda do Windows: ${result.reschedule.message}`)}
              </span>
            )}
            {invalid && <span role="alert" className="text-rose-600">Corrige os campos assinalados para poderes guardar.</span>}
            {!dirty && !result && !failure && !invalid && <span className="text-slate-500">Sem alterações por guardar.</span>}
          </div>
        </div>
      </div>
    </div>
  )
}
