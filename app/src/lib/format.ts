// Formatação em português de Portugal. O servidor envia datas locais sem fuso ("2026-10-26T18:00:00").

const WEEKDAYS = ['dom', 'seg', 'ter', 'qua', 'qui', 'sex', 'sáb']

export function parseLocal(iso: string | null | undefined): Date | null {
  if (!iso) return null
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(iso)
  if (!m) return null
  return new Date(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0))
}

const pad = (n: number) => String(n).padStart(2, '0')

/** "seg 26/10 18:00" */
export function fmtDateTime(iso: string | null | undefined): string {
  const d = parseLocal(iso)
  if (!d) return '—'
  return `${WEEKDAYS[d.getDay()]} ${pad(d.getDate())}/${pad(d.getMonth() + 1)} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** "hoje às 18:00", "amanhã às 18:00", "há 3 h", "há 2 dias". `now` vem do servidor para os testes serem estáveis. */
export function fmtRelative(iso: string | null | undefined, now: Date = new Date()): string {
  const d = parseLocal(iso)
  if (!d) return '—'
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  const dayDiff = Math.round((startOfDay(d).getTime() - startOfDay(now).getTime()) / 86_400_000)
  if (d > now) {
    if (dayDiff === 0) return `hoje às ${time}`
    if (dayDiff === 1) return `amanhã às ${time}`
    return `${fmtDateTime(iso)}`
  }
  const mins = Math.floor((now.getTime() - d.getTime()) / 60_000)
  if (mins < 1) return 'agora mesmo'
  if (mins < 60) return `há ${mins} min`
  if (mins < 60 * 24 && dayDiff === 0) return `há ${Math.floor(mins / 60)} h`
  if (dayDiff === -1) return `ontem às ${time}`
  return fmtDateTime(iso)
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate())
}

export function fmtDuration(seconds: number | null | undefined): string {
  if (seconds == null) return '—'
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`
  return `${Math.floor(seconds / 60)} min ${Math.round(seconds % 60)} s`
}

export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

/** Remove o prefixo "DEI - " e o sufixo do semestre do nome da disciplina. */
export function shortCourse(name: string): string {
  return name
    .replace(/^DEI\s*-\s*/, '')
    .replace(/\s*-\s*\d+º\s*Semestre.*$/i, '')
    .trim()
}

export const TRIGGER_LABEL: Record<string, string> = { scheduled: 'Agendado', manual: 'Manual', api: 'Pela app' }
