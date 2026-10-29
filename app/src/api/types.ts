// Tipos do contrato da API local (ver docs/API.md). Os campos de segredo são só de escrita.

export type JobStatus = 'running' | 'done' | 'failed'
export type EmailMethod = 'thunderbird' | 'imap'
export type HistoryKind = 'digest' | 'moodle_sync'
export type SummaryMode = 'rules' | 'ai'
export type AiProvider = 'cli' | 'api'
export type AiCli = 'claude' | 'gemini' | 'codex' | 'custom'
export type AiApiType = 'openai' | 'anthropic' | 'gemini' | 'openai_compatible'

export interface Check {
  id: string
  label: string
  ok: boolean
  detail: string
  required: boolean
}

export interface HistoryRecord {
  id: string
  kind: HistoryKind
  trigger: 'scheduled' | 'manual' | 'api'
  ok: boolean
  started: string
  finished: string
  duration_s: number | null
  error: string | null
  details: Record<string, unknown>
  /** Só vem preenchido no detalhe (GET /history/{id}); na lista só existe `has_summary`. */
  summary: string | null
  has_summary?: boolean
}

export interface TaskInfo {
  name: string
  state: string
  last_run: string | null
  next_run: string | null
  last_result: number | null
  /** Programa que a tarefa executa e se ainda existe. */
  target?: string | null
  target_ok?: boolean | null
}

export interface Job<R = unknown> {
  id: string
  kind: string
  status: JobStatus
  started: string
  finished: string | null
  progress: string[]
  progress_total: number
  result: R | null
  error: string | null
}

export interface DigestResult {
  ok: boolean
  sent: boolean
  skipped: boolean
  preview: boolean
  emails_found: number
  emails_new: number
  moodle_files: number
  ai_used: boolean
  ai_fallback: boolean
  summary: string | null
  error: string | null
  log: string[]
}

export interface SyncResult {
  ok: boolean
  busy: boolean
  new_files: number
  errors: number
  error: string | null
  files: { course?: string; name?: string; kind?: string }[]
}

export interface Status {
  version: string
  now: string
  ok: boolean
  checks: Check[]
  summary_mode: { mode: SummaryMode; description: string }
  digest: {
    last: HistoryRecord | null
    last_ok: HistoryRecord | null
    next_run: string | null
    scheduled_time: string
  }
  moodle: {
    configured: boolean
    username: string
    last_sync_date: string | null
    run_after_hour: number
    download_dir: string
    sync_dir: string | null
    last: HistoryRecord | null
  }
  tasks: TaskInfo[]
  active_job: Job | null
}

export interface Settings {
  summary: {
    mode: SummaryMode
    ai_provider: AiProvider
    ai_cli: AiCli
    ai_cli_command: string | string[]
    ai_api_type: AiApiType
    ai_model: string
    ai_base_url: string
    ai_api_key_set: boolean
    ai_api_key_env: string
    ai_api_key_env_set: boolean
    rules: SummaryRules
  }
  schedule: { time: string }
  destination: { session_recipients: string[]; session_configured: boolean }
  email: { method: EmailMethod; address: string; connected: boolean; password_set: boolean; oauth_connected: boolean; imap_host: string; imap_port: number }
  setup: { done: boolean }
  student: Student
  moodle: {
    sync_dir: string
    download_dir: string
    default_download_dir: string
    download_dir_is_default: boolean
    auto_download: boolean
    /** Endereço do Moodle a usar (`null` se a instituição não tem um conhecido e ainda não foi indicado). */
    url: string | null
    url_source: 'custom' | 'institution' | 'default' | 'missing'
  }
}

export interface SummaryRules {
  important_keywords: string[]
  ignore_senders: string[]
  ignore_subjects: string[]
}

export interface Student {
  institution_id: string
  institution_name: string
  course: string
  year: number
  configured: boolean
}

export interface Institution {
  id: string
  name: string
  has_moodle: boolean
}

/** Alteração parcial: mesma forma que `Settings` mais os campos só de escrita. */
export interface SettingsPatch {
  summary?: Partial<Omit<Settings['summary'], 'ai_api_key_set' | 'ai_api_key_env' | 'ai_api_key_env_set' | 'rules'>> & {
    rules?: Partial<SummaryRules>
    ai_api_key?: string
    clear_ai_api_key?: boolean
  }
  schedule?: Partial<Settings['schedule']>
  destination?: { session_id?: string }
  email?: { method?: EmailMethod; address?: string; imap_host?: string; imap_port?: number; password?: string; clear_password?: boolean; sign_out?: boolean }
  setup?: { done?: boolean }
  moodle?: { sync_dir?: string; download_dir?: string; url?: string; auto_download?: boolean }
  student?: { institution_id?: string; institution_name?: string; course?: string; year?: number }
}

export interface SettingsUpdateResult {
  settings: Settings
  effects: string[]
  reschedule?: { ok: boolean; message: string }
}

export interface SummaryTestResult {
  text: string
  ai_requested: boolean
  ai_fallback: boolean
  ok: boolean
}

export interface NewFile {
  ts: string
  kind: 'novo' | 'atualizado'
  course: string
  section: string
  name: string
  path: string
  size: number
}

export type LogName = 'notifier' | 'sync'

// ---- páginas

export interface Overview {
  days: number
  /** Resumo de hoje (em destaque). Sem ele, `latest` é o mais recente do período. */
  today: HistoryRecord | null
  latest: HistoryRecord | null
  items: HistoryRecord[]
  totals: { runs: number; sent: number; failed: number; emails: number; moodle_files: number }
}

export interface EmailItem {
  subject: string
  sender: string
  received_time: string
  preview: string
  message_id: string
  /** Já entrou num resumo enviado. */
  in_digest: boolean
}

export interface EmailsResult {
  source: string
  days: number
  total: number
  items: EmailItem[]
  error: string | null
}

export type FileState = 'pending' | 'downloading' | 'done' | 'unchanged' | 'error'

export interface ProgressItem {
  course: string
  section: string
  name: string
  state: FileState
  size: number
  error: string | null
}

export interface MoodleFiles {
  running: boolean
  started: string
  finished: string | null
  courses_total: number
  courses_done: number
  current_course: string
  percent: number
  counts: Record<FileState, number>
  items: ProgressItem[]
  downloaded: NewFile[]
  moodle: { configured: boolean; username: string; download_dir: string; last_sync_date: string | null }
}

export interface Activity {
  days: number
  history: HistoryRecord[]
  logs: { notifier: string[]; sync: string[] }
}

export interface SessionState {
  node_ok: boolean
  script_ok: boolean
  bot_created: boolean
  bot_id: string
  recipients: string[]
  configured: boolean
}

export interface SetupState {
  done: boolean
  email_method: EmailMethod
  steps: Check[]
  ready: boolean
  active_job: Job | null
}

export interface ThunderbirdState {
  /** O Thunderbird da app está instalado. */
  embedded: boolean
  /** Vem dentro da app: não há nada para instalar. */
  bundled?: boolean
  version: string | null
  /** A conta da escola já foi adicionada ao Thunderbird da app e há uma caixa sincronizada. */
  profile_ready: boolean
  /** Há um Thunderbird instalado no sistema (o do utilizador). */
  system_found: boolean
  source: 'embedded' | 'system' | 'none'
}

export interface DeviceLogin {
  device_code: string
  user_code: string
  verification_uri: string
  interval: number
  expires_in: number
}

export interface SessionAccount {
  session_id: string
  /** Frase de recuperação: só se mostra uma vez, a app não a guarda. */
  mnemonic: string
  state: SessionState
}

export interface OAuthStart {
  auth_url: string
  redirect_uri: string
}

export interface OAuthFinish {
  ok: boolean
  error?: string
}

export interface EmailTest {
  ok: boolean
  count?: number
  error?: string
}

export type DevicePoll = { status: 'pending' } | { status: 'done' } | { status: 'failed'; message: string }
