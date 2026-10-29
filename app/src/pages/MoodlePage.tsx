import { CheckCircle2, CircleDashed, FileText, RefreshCw, XCircle } from 'lucide-react'
import type { FileState, MoodleFiles, ProgressItem } from '../api'
import { JobPanel } from '../components/JobPanel'
import { Badge, Button, Card, Notice, Spinner } from '../components/ui'
import { fmtBytes, fmtDateTime, shortCourse } from '../lib/format'
import { useApp } from '../lib/context'
import { useAsync, useJobRunner } from '../lib/hooks'

function ProgressBar({ percent, label }: { percent: number; label: string }) {
  return (
    <div role="progressbar" aria-label={label} aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100} className="h-2.5 w-full overflow-hidden rounded-full bg-slate-200 dark:bg-slate-700">
      <div className="h-full rounded-full bg-brand-600 transition-all" style={{ width: `${percent}%` }} />
    </div>
  )
}

function FileList({ title, items, empty, icon }: { title: string; items: ProgressItem[]; empty: string; icon: React.ReactNode }) {
  return (
    <Card title={`${title} (${items.length})`}>
      {items.length === 0 ? (
        <p className="py-3 text-sm text-slate-500">{empty}</p>
      ) : (
        <ul aria-label={title} className="divide-y divide-slate-100 dark:divide-slate-800">
          {items.map((f, i) => (
            <li key={`${f.course}-${f.name}-${i}`} className="flex items-center gap-3 py-2">
              {icon}
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-slate-900 dark:text-slate-100">{f.name}</span>
                <span className="block truncate text-xs text-slate-500">{shortCourse(f.course)}{f.section ? ` · ${f.section}` : ''}</span>
              </span>
              {f.error && <span className="text-xs text-rose-600">{f.error}</span>}
              {f.size > 0 && <span className="text-xs text-slate-500">{fmtBytes(f.size)}</span>}
            </li>
          ))}
        </ul>
      )}
    </Card>
  )
}

export function MoodlePage() {
  const { api } = useApp()
  const files = useAsync<MoodleFiles>(() => api.moodleFiles(100), [api], 3000)
  const runner = useJobRunner(() => void files.reload())

  if (files.loading && !files.data) {
    return (
      <div className="flex items-center gap-2 p-8 text-sm text-slate-500" role="status">
        <Spinner /> A carregar os ficheiros…
      </div>
    )
  }
  if (!files.data) {
    return (
      <div className="p-8">
        <Notice tone="bad">Não foi possível obter os ficheiros: {files.error?.message}</Notice>
      </div>
    )
  }

  const f = files.data
  const by = (...states: FileState[]) => f.items.filter((i) => states.includes(i.state))
  const running = f.running || runner.running
  const hasRun = f.started !== ''

  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Moodle</h1>
          <p className="text-sm text-slate-500 dark:text-slate-400">Ficheiros descarregados, em curso e por descarregar.</p>
        </div>
        <Button variant="primary" disabled={running || !f.moodle.configured} onClick={() => void runner.start('moodle_sync')} icon={<RefreshCw className="size-4" />}>
          Sincronizar agora
        </Button>
      </header>

      {!f.moodle.configured && <Notice tone="warn">Ainda não há credenciais do Moodle. Define-as nas Definições.</Notice>}
      <JobPanel runner={runner} />

      <Card title="Sincronização">
        {!hasRun ? (
          <p className="text-sm text-slate-500">Ainda não houve nenhuma sincronização.</p>
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={f.running ? 'brand' : f.counts.error > 0 ? 'warn' : 'good'}>
                {f.running ? 'Em curso' : f.counts.error > 0 ? 'Concluída com erros' : 'Concluída'}
              </Badge>
              <span className="text-xs text-slate-500">
                {fmtDateTime(f.started)}
                {f.finished ? ` → ${fmtDateTime(f.finished)}` : ''}
              </span>
            </div>
            <ProgressBar percent={f.percent} label="Progresso da sincronização" />
            <p className="text-sm text-slate-600 dark:text-slate-400">
              {f.courses_total > 0 ? `Disciplina ${Math.min(f.courses_done + (f.running ? 1 : 0), f.courses_total)} de ${f.courses_total}` : 'A preparar…'}
              {f.running && f.current_course ? ` — ${shortCourse(f.current_course)}` : ''}
              <span className="ml-2 font-medium">{f.percent}%</span>
            </p>
            <dl className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {(
                [
                  ['Pendentes', f.counts.pending],
                  ['A descarregar', f.counts.downloading],
                  ['Novos', f.counts.done],
                  ['Erros', f.counts.error],
                ] as const
              ).map(([label, value]) => (
                <div key={label} className="rounded-xl bg-slate-50 p-3 dark:bg-slate-800/60">
                  <dd className="text-xl font-semibold text-slate-900 dark:text-slate-100">{value}</dd>
                  <dt className="text-xs text-slate-500">{label}</dt>
                </div>
              ))}
            </dl>
            <p className="text-xs text-slate-500">O progresso é por ficheiro e por disciplina: cada ficheiro é descarregado inteiro.</p>
          </div>
        )}
      </Card>

      <div className="grid gap-4 xl:grid-cols-3">
        <FileList title="Em progresso" items={by('downloading')} empty="Nada a descarregar neste momento." icon={<Spinner className="size-4 text-brand-600" />} />
        <FileList title="Pendentes" items={by('pending')} empty="Nenhum ficheiro à espera." icon={<CircleDashed className="size-4 text-slate-400" aria-hidden />} />
        <FileList title="Com erro" items={by('error')} empty="Sem erros." icon={<XCircle className="size-4 text-rose-600" aria-hidden />} />
      </div>

      <Card title={`Já descarregados (${f.downloaded.length})`} subtitle="Ficheiros novos ou atualizados registados pela app.">
        {f.downloaded.length === 0 ? (
          <p className="py-3 text-sm text-slate-500">Ainda não há ficheiros descarregados.</p>
        ) : (
          <table className="w-full text-left text-sm">
            <thead className="text-xs uppercase tracking-wide text-slate-500">
              <tr>
                <th className="py-2 pr-3 font-medium">Ficheiro</th>
                <th className="py-2 pr-3 font-medium">Disciplina</th>
                <th className="py-2 pr-3 font-medium">Quando</th>
                <th className="py-2 font-medium">Tamanho</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
              {f.downloaded.map((d) => (
                <tr key={`${d.path}-${d.ts}`}>
                  <td className="py-2 pr-3">
                    <span className="flex items-center gap-2">
                      <FileText className="size-4 text-slate-400" aria-hidden />
                      {d.name}
                      <Badge tone={d.kind === 'atualizado' ? 'warn' : 'good'}>{d.kind}</Badge>
                    </span>
                  </td>
                  <td className="py-2 pr-3 text-slate-600 dark:text-slate-400">{shortCourse(d.course)}</td>
                  <td className="py-2 pr-3 text-slate-600 dark:text-slate-400">{fmtDateTime(d.ts)}</td>
                  <td className="py-2 text-slate-600 dark:text-slate-400">{fmtBytes(d.size)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {f.counts.unchanged > 0 && (
          <p className="mt-3 flex items-center gap-2 text-xs text-slate-500">
            <CheckCircle2 className="size-4 text-emerald-600" aria-hidden /> {f.counts.unchanged} ficheiro(s) já estavam atualizados na última sincronização.
          </p>
        )}
      </Card>
    </div>
  )
}
