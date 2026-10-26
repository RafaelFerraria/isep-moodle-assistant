// Tipos do contrato da API local (ver docs/API.md). Os campos de segredo são só de escrita.

export type JobStatus = 'running' | 'done' | 'failed'
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
  }
  schedule: { time: string }
  destination: { session_recipients: string[]; session_configured: boolean }
  email: { method: string; address: string }
  moodle: { sync_dir: string; download_dir: string }
}

/** Alteração parcial: mesma forma que `Settings` mais os campos só de escrita. */
export interface SettingsPatch {
  summary?: Partial<Omit<Settings['summary'], 'ai_api_key_set' | 'ai_api_key_env' | 'ai_api_key_env_set'>> & {
    ai_api_key?: string
    clear_ai_api_key?: boolean
  }
  schedule?: Partial<Settings['schedule']>
  destination?: { session_id?: string }
  moodle?: Partial<Settings['moodle']>
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
