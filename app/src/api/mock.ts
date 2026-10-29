// Servidor simulado em memória: serve a app em modo demonstração (sem backend) e os testes.
// Reproduz as regras do contrato real: validação com erros por campo, segredos só de escrita, uma tarefa de cada vez.
import type { Transport, TransportResponse } from './transport'
import type { Activity, DevicePoll, EmailItem, EmailsResult, HistoryRecord, Job, MoodleFiles, NewFile, Overview, SessionState, SetupState, Settings, SettingsPatch, Status } from './types'

export interface MockOptions {
  /** Duração simulada das tarefas (ms). 0 = terminam no pedido seguinte ao arranque. */
  jobDurationMs?: number
  now?: () => Date
}

const SESSION_RE = /^05[0-9a-fA-F]{64}$/
const TIME_RE = /^([01]?\d|2[0-3]):[0-5]\d$/

function iso(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

export class MockTransport implements Transport {
  settings: Settings
  history: HistoryRecord[]
  newFiles: NewFile[]
  moodle = { configured: true, username: '1234567', last_sync_date: null as string | null }
  extraChecks: Status['checks'] = []
  session: SessionState = { node_ok: true, script_ok: true, bot_created: true, bot_id: '05bb11…c0de22', recipients: ['05a79f…d22077'], configured: true }
  emails: EmailItem[] = []
  emailError: string | null = null
  moodleProgress: MoodleFiles
  /** Os testes põem `true` para simular que o utilizador autorizou a conta Microsoft. */
  deviceApproved = false
  logs: Activity['logs'] = { notifier: ['[2026-10-26 18:00:01] === INÍCIO DA EXECUÇÃO DO NOTIFICADOR ISEP ==='], sync: ['[2026-10-26 18:00:05] A autenticar no Moodle do ISEP...'] }
  /** Último token recebido (só para os testes verificarem que chegou ao servidor). */
  lastApiKey = ''
  private jobs = new Map<string, Job>()
  private order: string[] = []
  private readonly opts: Required<MockOptions>
  requests: { method: string; path: string; body?: unknown }[] = []

  constructor(options: MockOptions = {}) {
    this.opts = { jobDurationMs: options.jobDurationMs ?? 0, now: options.now ?? (() => new Date()) }
    const now = this.opts.now()
    const ago = (h: number) => iso(new Date(now.getTime() - h * 3600_000))
    this.settings = {
      summary: {
        mode: 'rules',
        ai_provider: 'cli',
        ai_cli: 'claude',
        ai_cli_command: '',
        ai_api_type: 'openai',
        ai_model: '',
        ai_base_url: '',
        ai_api_key_set: false,
        ai_api_key_env: 'OPENAI_API_KEY',
        ai_api_key_env_set: false,
      },
      schedule: { time: '18:00' },
      destination: { session_recipients: ['05a79f…d22077'], session_configured: true },
      email: { method: 'thunderbird', address: '1234567@isep.ipp.pt', connected: false },
      setup: { done: true },
      moodle: { sync_dir: 'C:/isep-moodle-assistant/MoodleSync', download_dir: 'C:/Users/aluno/Downloads/Moodle_ISEP' },
    }
    this.history = [
      {
        id: 'a1b2c3d4e5f6', kind: 'digest', trigger: 'scheduled', ok: true, started: ago(25), finished: ago(25),
        duration_s: 41.2, error: null, summary: '📬 RESUMO ISEP — dom 25/10 18:00\n2 emails novos\n\n━━━━━━━━━━━━━━━━━━━━━━━━\n⏰ PRAZOS\n🟠 LAPR3: Sprint 1 — Status Report\n   Entrega: HOJE 23:59',
        details: { emails_found: 12, emails_new: 2, moodle_files: 1, sent: true, skipped: false, ai_used: false, ai_fallback: false },
      },
      {
        id: 'b2c3d4e5f6a1', kind: 'moodle_sync', trigger: 'scheduled', ok: false, started: ago(24), finished: ago(24),
        duration_s: 12.5, error: '2 erro(s) durante a sincronização', summary: null,
        details: { new_files: 0, errors: 2, files: [] },
      },
      {
        id: 'c3d4e5f6a1b2', kind: 'digest', trigger: 'manual', ok: true, started: ago(2), finished: ago(2),
        duration_s: 38.9, error: null, summary: '📬 RESUMO ISEP — seg 26/10 16:00\n1 emails novos',
        details: { emails_found: 3, emails_new: 1, moodle_files: 0, sent: true, skipped: false, ai_used: true, ai_fallback: false },
      },
    ].map((r) => ({ ...r, has_summary: !!r.summary })) as HistoryRecord[]
    this.emails = [
      { subject: 'LAPR3: Sprint 2 - Entrega', sender: '"Docente LAPR3" <lapr3@isep.ipp.pt>', received_time: ago(1), preview: 'A entrega do Sprint 2 é amanhã às 23:59.', message_id: '<e1@isep>', in_digest: false },
      { subject: 'Ementa Semanal - Bar edifício E', sender: 'ISEP <estudantes@isep.ipp.pt>', received_time: ago(26), preview: 'Segue a ementa da semana.', message_id: '<e2@isep>', in_digest: true },
    ]
    this.moodleProgress = {
      running: false, started: ago(3), finished: ago(3), courses_total: 6, courses_done: 6, current_course: '', percent: 100,
      counts: { pending: 0, downloading: 0, done: 1, unchanged: 3, error: 0 },
      items: [{ course: 'DEI - Laboratório / Projeto III', section: 'Sprint 2', name: 'Enunciado Sprint 2.pdf', state: 'done', size: 482000, error: null }],
      downloaded: [], moodle: { configured: true, username: '1234567', download_dir: 'C:/Users/aluno/Downloads/Moodle_ISEP', last_sync_date: null },
    }
    this.newFiles = [
      { ts: ago(3), kind: 'novo', course: 'DEI - Laboratório / Projeto III', section: 'Sprint 2', name: 'Enunciado Sprint 2.pdf', path: 'LAPR3/Enunciado Sprint 2.pdf', size: 482000 },
      { ts: ago(30), kind: 'atualizado', course: 'DEI - Estruturas de Informação', section: 'Aulas', name: 'Slides T3.pdf', path: 'ESINF/Slides T3.pdf', size: 913000 },
    ]
  }

  // ---------------------------------------------------------------- Transport

  async request(method: string, path: string, body?: unknown): Promise<TransportResponse> {
    this.requests.push({ method, path, body })
    const url = new URL(path, 'http://mock')
    const route = `${method} ${url.pathname}`
    const q = url.searchParams

    if (route === 'GET /health') return ok({ ok: true, version: 'mock' })
    if (route === 'GET /status') return ok(this.status())
    if (route === 'GET /settings') return ok(this.settings)
    if (route === 'PUT /settings') return this.putSettings(body as SettingsPatch)
    if (route === 'POST /summary/test') return ok(this.testSummary())
    if (route === 'POST /schedule/register') return ok({ ok: true, message: 'Tarefas registadas.' })
    if (route === 'PUT /moodle/credentials') return this.putCredentials(body as { username: string; password: string })
    if (route === 'POST /digest/preview') return this.startJob('digest_preview', true)
    if (route === 'POST /digest/run') return this.startJob('digest', false)
    if (route === 'POST /moodle/sync') return this.startJob('moodle_sync', false)
    if (route === 'GET /jobs') return ok({ jobs: [...this.order].reverse().map((id) => this.jobs.get(id)) })
    const job = /^GET \/jobs\/(.+)$/.exec(route)
    if (job) return this.getJob(decodeURIComponent(job[1]), Number(q.get('since_line') ?? 0))
    if (route === 'GET /history') {
      const kind = q.get('kind')
      const items = this.history
        .filter((r) => !kind || r.kind === kind)
        .slice()
        .reverse()
        .slice(Number(q.get('offset') ?? 0), Number(q.get('offset') ?? 0) + Number(q.get('limit') ?? 30))
        .map((r) => ({ ...r, summary: null, has_summary: !!r.summary }))
      return ok({ items })
    }
    const item = /^GET \/history\/(.+)$/.exec(route)
    if (item) {
      const rec = this.history.find((r) => r.id === decodeURIComponent(item[1]))
      return rec ? ok(rec) : fail(404, 'Registo desconhecido.')
    }
    if (route === 'GET /overview') return ok(this.overview(Number(q.get('days') ?? 7)))
    if (route === 'GET /emails') return ok(this.emailsResult(Number(q.get('days') ?? 7)))
    if (route === 'GET /moodle/files') return ok({ ...this.moodleProgress, downloaded: this.newFiles, moodle: { ...this.moodleProgress.moodle, configured: this.moodle.configured, username: this.moodle.username } })
    if (route === 'GET /activity') return ok(this.activity(Number(q.get('days') ?? 7)))
    if (route === 'GET /setup') return ok(this.setupState())
    if (route === 'GET /session') return ok(this.session)
    if (route === 'POST /session/bot') return this.createBot((body as { force?: boolean } | undefined)?.force === true)
    if (route === 'POST /session/pair') return this.startJob('session_pair', false)
    if (route === 'POST /session/test') return this.startJob('session_test', false)
    if (route === 'POST /session/recipients') return this.addRecipient((body as { session_id: string }).session_id)
    const rm = /^DELETE \/session\/recipients\/(.+)$/.exec(route)
    if (rm) return this.removeRecipient(Number(rm[1]))
    if (route === 'POST /email/connect/start') {
      return ok({ device_code: 'dc-mock', user_code: 'ABCD-1234', verification_uri: 'https://microsoft.com/devicelogin', interval: 1, expires_in: 900 })
    }
    if (route === 'POST /email/connect/poll') return ok(this.pollDevice())
    if (route === 'GET /moodle/new-files') return ok({ items: this.newFiles.slice(0, Number(q.get('limit') ?? 100)) })
    const log = /^GET \/logs\/(.+)$/.exec(route)
    if (log) {
      return ['notifier', 'sync'].includes(log[1])
        ? ok({ name: log[1], lines: [`[${iso(this.opts.now())}] (mock) sem registos reais`] })
        : fail(404, 'Log desconhecido (notifier ou sync).')
    }
    return fail(404, 'Não encontrado.')
  }

  // ---------------------------------------------------------------- estado

  private status(): Status {
    const s = this.settings
    const mode = s.summary.mode
    const checks: Status['checks'] = [
      { id: 'config', label: 'Configuração criada', ok: true, detail: '', required: true },
      { id: 'session', label: 'Destino Session configurado', ok: s.destination.session_configured, detail: '', required: true },
      { id: 'thunderbird', label: 'Thunderbird instalado', ok: true, detail: '', required: true },
      { id: 'node', label: 'Node.js instalado', ok: true, detail: 'Necessário para enviar para o Session.', required: true },
      { id: 'moodle', label: 'Credenciais do Moodle', ok: this.moodle.configured, detail: this.moodle.configured ? '' : 'Define o utilizador e a palavra-passe do Moodle.', required: false },
      { id: 'tasks', label: 'Tarefas agendadas no Windows', ok: true, detail: '2 tarefa(s) registada(s).', required: true },
      { id: 'internet', label: 'Ligação ao Moodle', ok: true, detail: '', required: true },
      mode === 'ai'
        ? this.aiCheck()
        : { id: 'ai', label: 'Resumo por IA', ok: true, detail: "Modo 'Regras' (local) ativo.", required: false },
      ...this.extraChecks,
    ]
    const last = this.history.filter((r) => r.kind === 'digest').slice(-1)[0] ?? null
    const lastOk = this.history.filter((r) => r.kind === 'digest' && r.ok).slice(-1)[0] ?? null
    const sync = this.history.filter((r) => r.kind === 'moodle_sync').slice(-1)[0] ?? null
    const now = this.opts.now()
    const [h, m] = s.schedule.time.split(':').map(Number)
    const next = new Date(now)
    next.setHours(h, m, 0, 0)
    if (next <= now) next.setDate(next.getDate() + 1)
    const active = [...this.jobs.values()].find((j) => j.status === 'running') ?? null
    return {
      version: 'mock',
      now: iso(now),
      ok: checks.every((c) => c.ok || !c.required),
      checks,
      summary_mode: {
        mode,
        description:
          mode === 'rules'
            ? 'Regras (local, sem enviar emails a terceiros)'
            : s.summary.ai_provider === 'cli'
              ? `IA via CLI com login (${s.summary.ai_cli})`
              : `IA via API (${s.summary.ai_api_type}, modelo ${s.summary.ai_model || 'predefinido'})`,
      },
      digest: { last: last && strip(last), last_ok: lastOk && strip(lastOk), next_run: iso(next), scheduled_time: s.schedule.time },
      moodle: {
        configured: this.moodle.configured,
        username: this.moodle.username,
        last_sync_date: this.moodle.last_sync_date,
        run_after_hour: 18,
        download_dir: s.moodle.download_dir,
        sync_dir: s.moodle.sync_dir,
        last: sync && strip(sync),
      },
      tasks: [{ name: 'ISEP_Email_Session_Daily', state: 'Ready', last_run: null, next_run: iso(next), last_result: 0 }],
      active_job: active,
    }
  }

  private aiCheck(): Status['checks'][number] {
    const s = this.settings.summary
    const hasKey = s.ai_api_key_set || s.ai_api_key_env_set
    if (s.ai_provider === 'api') {
      return { id: 'ai', label: 'Resumo por IA', ok: hasKey, detail: hasKey ? 'Token de API disponível.' : `Sem token (define-o ou usa a variável ${s.ai_api_key_env}).`, required: true }
    }
    return { id: 'ai', label: 'Resumo por IA', ok: true, detail: `CLI '${s.ai_cli}' encontrada.`, required: true }
  }

  private testSummary() {
    const ai = this.settings.summary.mode === 'ai'
    const bad = ai && this.settings.summary.ai_provider === 'api' && !(this.settings.summary.ai_api_key_set || this.settings.summary.ai_api_key_env_set)
    return {
      text: bad
        ? '📬 RESUMO ISEP — exemplo\n\n⚠️ IA indisponível (Sem token de API); resumo gerado por regras.'
        : ai
          ? '📬 RESUMO ISEP (IA)\n\n⏰ PRAZOS\n• LAPR3 — entrega do Sprint 2 amanhã às 23:59.'
          : '📬 RESUMO ISEP — exemplo\n1 emails novos\n\n⏰ PRAZOS\n🟠 LAPR3: Sprint 2 - Entrega\n   Entrega: AMANHÃ 23:59',
      ai_requested: ai,
      ai_fallback: bad,
      ok: !bad,
    }
  }

  private overview(days: number): Overview {
    const now = this.opts.now()
    const since = now.getTime() - days * 86_400_000
    const todayKey = iso(now).slice(0, 10)
    const digests = this.history
      .filter((r) => r.kind === 'digest' && new Date(r.started.replace(' ', 'T')).getTime() >= since)
      .slice()
      .reverse()
    const withText = digests.filter((r) => r.summary)
    const num = (r: HistoryRecord, k: string) => Number((r.details as Record<string, unknown>)[k] ?? 0)
    return {
      days,
      today: withText.find((r) => r.started.startsWith(todayKey)) ?? null,
      latest: withText[0] ?? null,
      items: digests,
      totals: {
        runs: digests.length,
        sent: digests.filter((r) => (r.details as Record<string, unknown>).sent === true).length,
        failed: digests.filter((r) => !r.ok).length,
        emails: digests.reduce((a, r) => a + num(r, 'emails_new'), 0),
        moodle_files: digests.reduce((a, r) => a + num(r, 'moodle_files'), 0),
      },
    }
  }

  private emailsResult(days: number): EmailsResult {
    if (this.emailError) return { source: this.settings.email.method, days, total: 0, items: [], error: this.emailError }
    return { source: this.settings.email.method, days, total: this.emails.length, items: this.emails, error: null }
  }

  private activity(days: number): Activity {
    return { days, history: this.history.slice().reverse().map(strip), logs: this.logs }
  }

  private setupState(): SetupState {
    const s = this.settings
    const emailOk = s.email.method === 'graph' ? s.email.connected : true
    const steps: Status['checks'] = [
      { id: 'email', label: 'Emails', ok: emailOk, detail: '', required: true },
      { id: 'moodle', label: 'Moodle', ok: this.moodle.configured, detail: '', required: false },
      { id: 'session', label: 'Session', ok: this.session.configured, detail: '', required: true },
      { id: 'summary', label: 'Tipo de resumo', ok: true, detail: '', required: false },
      { id: 'schedule', label: 'Agendamento', ok: true, detail: '', required: true },
    ]
    return { done: s.setup.done, email_method: s.email.method, steps, ready: steps.every((c) => c.ok || !c.required), active_job: null }
  }

  private createBot(force: boolean): TransportResponse {
    if (this.session.bot_created && !force) return ok(this.session)
    this.session = { ...this.session, bot_created: true, bot_id: '05cc22…beef33', recipients: [], configured: false }
    return ok(this.session)
  }

  private addRecipient(id: string): TransportResponse {
    if (!SESSION_RE.test(id.trim())) return fail(422, 'Session ID inválido.', { errors: { session_id: 'O Session ID deve ter 66 caracteres e começar por 05.' } })
    const masked = `${id.slice(0, 6)}…${id.slice(-4)}`
    if (!this.session.recipients.includes(masked)) this.session.recipients = [...this.session.recipients, masked]
    this.session = { ...this.session, configured: this.session.bot_created && this.session.recipients.length > 0 }
    return ok(this.session)
  }

  private removeRecipient(index: number): TransportResponse {
    if (!Number.isInteger(index) || index < 0 || index >= this.session.recipients.length) return fail(404, 'Destinatário desconhecido.')
    this.session = { ...this.session, recipients: this.session.recipients.filter((_, i) => i !== index) }
    this.session.configured = this.session.bot_created && this.session.recipients.length > 0
    return ok(this.session)
  }

  private pollDevice(): DevicePoll {
    if (!this.deviceApproved) return { status: 'pending' }
    this.settings = { ...this.settings, email: { ...this.settings.email, method: 'graph', connected: true } }
    return { status: 'done' }
  }

  // ---------------------------------------------------------------- escrita

  private putSettings(patch: SettingsPatch): TransportResponse {
    const errors: Record<string, string> = {}
    const next: Settings = structuredClone(this.settings)
    const sm = patch.summary ?? {}
    if (sm.mode !== undefined) {
      if (sm.mode === 'rules' || sm.mode === 'ai') next.summary.mode = sm.mode
      else errors['summary.mode'] = "deve ser um de ['ai', 'rules']"
    }
    if (sm.ai_provider !== undefined) {
      if (sm.ai_provider === 'cli' || sm.ai_provider === 'api') next.summary.ai_provider = sm.ai_provider
      else errors['summary.ai_provider'] = "deve ser um de ['api', 'cli']"
    }
    if (sm.ai_cli !== undefined) next.summary.ai_cli = sm.ai_cli
    if (sm.ai_cli_command !== undefined) next.summary.ai_cli_command = sm.ai_cli_command
    if (sm.ai_api_type !== undefined) {
      next.summary.ai_api_type = sm.ai_api_type
      next.summary.ai_api_key_env = { openai: 'OPENAI_API_KEY', anthropic: 'ANTHROPIC_API_KEY', gemini: 'GEMINI_API_KEY', openai_compatible: 'AI_API_KEY' }[sm.ai_api_type]
    }
    if (sm.ai_model !== undefined) next.summary.ai_model = sm.ai_model
    if (sm.ai_base_url !== undefined) {
      if (sm.ai_base_url && !/^https?:\/\//.test(sm.ai_base_url)) errors['summary.ai_base_url'] = 'deve começar por http:// ou https://'
      else next.summary.ai_base_url = sm.ai_base_url
    }
    if (sm.clear_ai_api_key) {
      this.lastApiKey = ''
      next.summary.ai_api_key_set = false
    }
    if (sm.ai_api_key) {
      this.lastApiKey = sm.ai_api_key.trim()
      next.summary.ai_api_key_set = true
    }
    if (next.summary.mode === 'ai') {
      const cmd = next.summary.ai_cli_command
      if (next.summary.ai_provider === 'cli' && next.summary.ai_cli === 'custom' && (!cmd || cmd.length === 0)) {
        errors['summary.ai_cli_command'] = "obrigatório quando a CLI é 'custom'"
      }
      if (next.summary.ai_provider === 'api' && next.summary.ai_api_type === 'openai_compatible') {
        if (!next.summary.ai_base_url) errors['summary.ai_base_url'] = 'obrigatório para APIs compatíveis com OpenAI'
        if (!next.summary.ai_model) errors['summary.ai_model'] = 'obrigatório para APIs compatíveis com OpenAI'
      }
    }
    const effects: string[] = []
    if (patch.schedule?.time !== undefined) {
      if (TIME_RE.test(patch.schedule.time)) {
        const [h, m] = patch.schedule.time.split(':')
        const normalized = `${h.padStart(2, '0')}:${m}`
        if (normalized !== next.schedule.time) effects.push('reschedule')
        next.schedule.time = normalized
      } else errors['schedule.time'] = 'formato HH:MM'
    }
    if (patch.destination?.session_id !== undefined) {
      const sid = patch.destination.session_id.trim()
      if (SESSION_RE.test(sid)) next.destination = { session_configured: true, session_recipients: [`${sid.slice(0, 6)}…${sid.slice(-4)}`] }
      else errors['destination.session_id'] = 'deve ter 66 caracteres hexadecimais e começar por 05'
    }
    if (patch.email?.method !== undefined) {
      if (patch.email.method === 'thunderbird' || patch.email.method === 'graph') next.email.method = patch.email.method
      else errors['email.method'] = "deve ser um de ['graph', 'thunderbird']"
    }
    if (patch.email?.address !== undefined) {
      if (patch.email.address === '' || patch.email.address.includes('@')) next.email.address = patch.email.address
      else errors['email.address'] = 'endereço de email inválido'
    }
    if (patch.setup?.done !== undefined) next.setup.done = patch.setup.done
    if (patch.moodle?.download_dir !== undefined) next.moodle.download_dir = patch.moodle.download_dir
    if (patch.moodle?.sync_dir !== undefined) next.moodle.sync_dir = patch.moodle.sync_dir
    if (Object.keys(errors).length) return fail(422, 'Definições inválidas.', { errors })
    this.settings = next
    const result: Record<string, unknown> = { settings: next, effects }
    if (effects.includes('reschedule')) result.reschedule = { ok: true, message: 'Tarefas registadas.' }
    return ok(result)
  }

  private putCredentials(body: { username: string; password: string }): TransportResponse {
    if (!body.username || !body.password) return fail(400, 'Utilizador e palavra-passe são obrigatórios.')
    if (body.password === 'errada') return fail(400, 'O Moodle recusou o login (confirma o número e a palavra-passe).')
    this.moodle = { ...this.moodle, configured: true, username: body.username.split('@')[0] }
    return ok({ ok: true, username: this.moodle.username })
  }

  // ---------------------------------------------------------------- tarefas

  private startJob(kind: string, preview: boolean): TransportResponse {
    const running = [...this.jobs.values()].find((j) => j.status === 'running')
    if (running) return fail(409, 'Já existe uma tarefa em execução.', { active_job_id: running.id })
    const job: Job = {
      id: `job${this.order.length + 1}`, kind, status: 'running', started: iso(this.opts.now()), finished: null,
      progress: [], progress_total: 0, result: null, error: null,
    }
    this.jobs.set(job.id, job)
    this.order.push(job.id)
    const lines =
      kind === 'session_pair'
        ? ['À espera da tua mensagem para o bot.', 'Mensagem recebida: destinatário guardado.']
        : kind === 'session_test'
          ? ['A enviar resumo para o Session (Destino: 05a79f...)...', 'Mensagem Session enviada com sucesso para 05a79f...!']
          : kind === 'moodle_sync'
        ? ['A autenticar no Moodle do ISEP...', 'Foram encontradas 6 disciplinas.', 'Sincronização terminada: 1 ficheiros novos/atualizados, 0 erros.']
        : ['A procurar emails recebidos...', 'Emails encontrados: 3', 'A gerar resumo e priorização dos emails...', preview ? 'Resumo gerado com sucesso!' : 'Mensagem Session enviada com sucesso.']
    const finish = () => {
      job.progress = lines
      job.progress_total = lines.length
      job.status = 'done'
      job.finished = iso(this.opts.now())
      if (kind === 'session_pair') {
        this.addRecipient(`05${'ab'.repeat(32)}`)
        job.result = { session_id: '05abab…abab' }
      } else if (kind === 'session_test') {
        job.result = { ok: true }
      } else if (kind === 'moodle_sync') {
        job.result = { ok: true, busy: false, new_files: 1, errors: 0, error: null, files: [{ course: 'LAPR3', name: 'Enunciado.pdf', kind: 'novo' }] }
        this.moodle.last_sync_date = iso(this.opts.now()).slice(0, 10)
        this.history.push({ id: `s${this.history.length}`, kind: 'moodle_sync', trigger: 'api', ok: true, started: job.started, finished: job.finished, duration_s: 3, error: null, summary: null, details: { new_files: 1, errors: 0, files: [{ course: 'LAPR3', name: 'Enunciado.pdf', kind: 'novo' }] } })
      } else {
        const text = this.testSummary().text
        job.result = { ok: true, sent: !preview, skipped: false, preview, emails_found: 3, emails_new: 1, moodle_files: 0, ai_used: this.settings.summary.mode === 'ai', ai_fallback: false, summary: text, error: null, log: lines }
        if (!preview) this.history.push({ id: `d${this.history.length}`, kind: 'digest', trigger: 'api', ok: true, started: job.started, finished: job.finished, duration_s: 4, error: null, summary: text, details: { emails_found: 3, emails_new: 1, moodle_files: 0, sent: true, skipped: false, ai_used: false, ai_fallback: false } })
      }
    }
    if (this.opts.jobDurationMs <= 0) queueMicrotask(finish)
    else setTimeout(finish, this.opts.jobDurationMs)
    return ok(job, 202)
  }

  private getJob(id: string, since: number): TransportResponse {
    const job = this.jobs.get(id)
    if (!job) return fail(404, 'Tarefa desconhecida.')
    return ok({ ...job, progress: job.progress.slice(since) })
  }
}

function strip(r: HistoryRecord): HistoryRecord {
  return { ...r, summary: null, has_summary: !!r.summary }
}
function ok(data: unknown, status = 200): TransportResponse {
  return { status, data: structuredClone(data) }
}
function fail(status: number, detail: string, extra: Record<string, unknown> = {}): TransportResponse {
  return { status, data: { detail, ...extra } }
}
