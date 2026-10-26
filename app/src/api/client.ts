import type { Transport } from './transport'
import type {
  DigestResult,
  HistoryKind,
  HistoryRecord,
  Job,
  LogName,
  NewFile,
  Settings,
  SettingsPatch,
  SettingsUpdateResult,
  Status,
  SummaryTestResult,
  SyncResult,
} from './types'

/** Erro devolvido pela API (ou falha de ligação, com `status === 0`). */
export class ApiError extends Error {
  readonly status: number
  /** Erros por campo (422), p. ex. `{ 'schedule.time': 'formato HH:MM' }`. */
  readonly fields: Record<string, string>
  /** Preenchido no 409: já existe uma tarefa em curso. */
  readonly activeJobId: string | null

  constructor(status: number, message: string, fields: Record<string, string> = {}, activeJobId: string | null = null) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.fields = fields
    this.activeJobId = activeJobId
  }

  get isNetwork(): boolean {
    return this.status === 0
  }
  get isUnauthorized(): boolean {
    return this.status === 401 || this.status === 403
  }
}

function errorFrom(status: number, data: unknown): ApiError {
  const body = (data ?? {}) as { detail?: unknown; errors?: Record<string, string>; active_job_id?: string }
  const detail = typeof body.detail === 'string' ? body.detail : `Erro ${status}`
  return new ApiError(status, detail, body.errors ?? {}, body.active_job_id ?? null)
}

export interface HistoryQuery {
  kind?: HistoryKind
  limit?: number
  offset?: number
}

export class AssistantApi {
  private readonly transport: Transport

  constructor(transport: Transport) {
    this.transport = transport
  }

  private async call<T>(method: string, path: string, body?: unknown): Promise<T> {
    let res
    try {
      res = await this.transport.request(method, path, body)
    } catch {
      throw new ApiError(0, 'Sem ligação ao servidor local.')
    }
    if (res.status >= 400) throw errorFrom(res.status, res.data)
    return res.data as T
  }

  health = () => this.call<{ ok: boolean; version: string }>('GET', '/health')
  status = () => this.call<Status>('GET', '/status')
  settings = () => this.call<Settings>('GET', '/settings')
  updateSettings = (patch: SettingsPatch) => this.call<SettingsUpdateResult>('PUT', '/settings', patch)
  testSummary = () => this.call<SummaryTestResult>('POST', '/summary/test')
  registerSchedule = () => this.call<{ ok: boolean; message: string }>('POST', '/schedule/register')
  setMoodleCredentials = (username: string, password: string) =>
    this.call<{ ok: boolean; username: string }>('PUT', '/moodle/credentials', { username, password })

  startDigestPreview = () => this.call<Job<DigestResult>>('POST', '/digest/preview')
  startDigestRun = () => this.call<Job<DigestResult>>('POST', '/digest/run')
  startMoodleSync = () => this.call<Job<SyncResult>>('POST', '/moodle/sync')
  job = <R = unknown>(id: string, sinceLine = 0) => this.call<Job<R>>('GET', `/jobs/${encodeURIComponent(id)}?since_line=${sinceLine}`)

  history = (q: HistoryQuery = {}) => {
    const params = new URLSearchParams()
    if (q.kind) params.set('kind', q.kind)
    params.set('limit', String(q.limit ?? 30))
    if (q.offset) params.set('offset', String(q.offset))
    return this.call<{ items: HistoryRecord[] }>('GET', `/history?${params}`)
  }
  historyItem = (id: string) => this.call<HistoryRecord>('GET', `/history/${encodeURIComponent(id)}`)
  newFiles = (limit = 100) => this.call<{ items: NewFile[] }>('GET', `/moodle/new-files?limit=${limit}`)
  logs = (name: LogName, lines = 200) => this.call<{ name: string; lines: string[] }>('GET', `/logs/${name}?lines=${lines}`)
}
