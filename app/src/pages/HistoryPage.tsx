import { CheckCircle2, FileText, RefreshCw, XCircle } from 'lucide-react'
import { useState } from 'react'
import type { Activity, HistoryKind, HistoryRecord, LogName } from '../api'
import { Badge, Button, Card, Notice, Pre, Select, Spinner } from '../components/ui'
import { Field } from '../components/ui'
import { cx } from '../components/ui'
import { fmtDateTime, fmtDuration, shortCourse, TRIGGER_LABEL } from '../lib/format'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'

type Tab = HistoryKind | 'logs'

const TABS: { id: Tab; label: string }[] = [
  { id: 'digest', label: 'Resumos' },
  { id: 'moodle_sync', label: 'Sincronizações' },
  { id: 'logs', label: 'Registos' },
]

function recordTitle(r: HistoryRecord): string {
  const d = r.details as Record<string, number | boolean | undefined>
  if (r.kind === 'moodle_sync') return r.ok ? `Sincronização (${d.new_files ?? 0} novo(s))` : 'Sincronização falhou'
  if (!r.ok) return 'Resumo falhou'
  return d.skipped ? 'Sem novidades' : `Resumo enviado (${d.emails_new ?? 0} email(s))`
}

function RecordList({ items, selected, onSelect }: { items: HistoryRecord[]; selected: string | null; onSelect: (id: string) => void }) {
  if (items.length === 0) return <p className="py-6 text-center text-sm text-slate-500">Ainda não há execuções registadas.</p>
  return (
    <ul className="divide-y divide-slate-100 dark:divide-slate-800" aria-label="Execuções">
      {items.map((r) => (
        <li key={r.id}>
          <button
            type="button"
            onClick={() => onSelect(r.id)}
            aria-pressed={selected === r.id}
            className={cx(
              'flex w-full items-center gap-3 px-2 py-3 text-left transition hover:bg-slate-50 dark:hover:bg-slate-800/60',
              selected === r.id && 'bg-brand-50 dark:bg-indigo-950/40',
            )}
          >
            {r.ok ? <CheckCircle2 className="size-5 shrink-0 text-emerald-600" aria-label="Sucesso" /> : <XCircle className="size-5 shrink-0 text-rose-600" aria-label="Falhou" />}
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm font-medium text-slate-900 dark:text-slate-100">{recordTitle(r)}</span>
              <span className="block text-xs text-slate-500 dark:text-slate-400">
                {fmtDateTime(r.finished)} · {fmtDuration(r.duration_s)}
              </span>
            </span>
            <Badge>{TRIGGER_LABEL[r.trigger] ?? r.trigger}</Badge>
          </button>
        </li>
      ))}
    </ul>
  )
}

function RecordDetail({ id }: { id: string }) {
  const { api } = useApp()
  const rec = useAsync(() => api.historyItem(id), [api, id])
  if (rec.loading && !rec.data) return <div className="flex items-center gap-2 text-sm text-slate-500"><Spinner /> A carregar…</div>
  if (!rec.data) return <Notice tone="bad">{rec.error?.message ?? 'Registo não encontrado.'}</Notice>
  const r = rec.data
  const files = ((r.details.files as { course?: string; name?: string; kind?: string }[] | undefined) ?? [])
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={r.ok ? 'good' : 'bad'}>{r.ok ? 'Sucesso' : 'Falhou'}</Badge>
        <Badge>{TRIGGER_LABEL[r.trigger] ?? r.trigger}</Badge>
        <span className="text-xs text-slate-500">{fmtDateTime(r.started)} · {fmtDuration(r.duration_s)}</span>
      </div>
      {r.error && <Notice tone="bad">{r.error}</Notice>}
      {r.kind === 'digest' && (
        <dl className="grid grid-cols-3 gap-3 text-center">
          {[
            ['Emails lidos', r.details.emails_found],
            ['Emails novos', r.details.emails_new],
            ['Ficheiros Moodle', r.details.moodle_files],
          ].map(([label, value]) => (
            <div key={String(label)} className="rounded-xl bg-slate-50 p-3 dark:bg-slate-800/60">
              <dd className="text-lg font-semibold text-slate-900 dark:text-slate-100">{String(value ?? 0)}</dd>
              <dt className="text-xs text-slate-500">{String(label)}</dt>
            </div>
          ))}
        </dl>
      )}
      {r.kind === 'digest' && Boolean(r.details.ai_fallback) && <Notice tone="warn">A IA falhou e usou-se o modo Regras.</Notice>}
      {r.summary && <Pre label="Resumo enviado">{r.summary}</Pre>}
      {r.kind === 'moodle_sync' && files.length > 0 && (
        <ul className="space-y-1 text-sm" aria-label="Ficheiros desta sincronização">
          {files.map((f, i) => (
            <li key={i} className="flex items-center gap-2">
              <FileText className="size-4 text-slate-400" aria-hidden />
              <span className="truncate">{f.name}</span>
              <span className="text-xs text-slate-500">{shortCourse(f.course ?? '')}</span>
              <Badge tone={f.kind === 'atualizado' ? 'warn' : 'good'}>{f.kind}</Badge>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function RecordsTab({ kind, items }: { kind: HistoryKind; items: HistoryRecord[] }) {
  const [selected, setSelected] = useState<string | null>(null)
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <RecordList items={items.filter((r) => r.kind === kind)} selected={selected} onSelect={setSelected} />
      </Card>
      <Card title="Detalhe">
        {selected ? <RecordDetail id={selected} /> : <p className="text-sm text-slate-500">Escolhe uma execução para ver o detalhe.</p>}
      </Card>
    </div>
  )
}

function LogsTab({ logs, onRefresh }: { logs: Activity['logs']; onRefresh: () => void }) {
  const [name, setName] = useState<LogName>('notifier')
  const lines = logs[name]
  return (
    <Card
      title="Registos"
      subtitle="Só se guardam os últimos 7 dias; o que é mais antigo é apagado automaticamente."
      action={
        <Button variant="ghost" onClick={onRefresh} icon={<RefreshCw className="size-4" />}>
          Atualizar
        </Button>
      }
    >
      <div className="mb-3 max-w-xs">
        <Field label="Ficheiro de registo">
          {(p) => (
            <Select {...p} value={name} onChange={(e) => setName(e.target.value as LogName)}>
              <option value="notifier">Resumo diário</option>
              <option value="sync">Sincronização do Moodle</option>
            </Select>
          )}
        </Field>
      </div>
      {lines.length > 0 ? <Pre label="Registos">{lines.join('\n')}</Pre> : <p className="text-sm text-slate-500">Sem registos.</p>}
    </Card>
  )
}

export function HistoryPage() {
  const { api } = useApp()
  const activity = useAsync<Activity>(() => api.activity(7), [api])
  const [tab, setTab] = useState<Tab>('digest')
  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header>
        <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Histórico</h1>
        <p className="text-sm text-slate-500 dark:text-slate-400">Os últimos 7 dias de resumos, sincronizações e registos, guardados neste PC.</p>
      </header>
      <div role="tablist" aria-label="Secções do histórico" className="flex flex-wrap gap-1 border-b border-slate-200 dark:border-slate-800">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            type="button"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            className={cx(
              '-mb-px border-b-2 px-4 py-2 text-sm font-medium transition',
              tab === t.id ? 'border-brand-600 text-brand-700 dark:text-indigo-300' : 'border-transparent text-slate-500 hover:text-slate-800 dark:hover:text-slate-200',
            )}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {activity.loading && !activity.data ? (
          <div className="flex items-center gap-2 p-4 text-sm text-slate-500"><Spinner /> A carregar…</div>
        ) : !activity.data ? (
          <Notice tone="bad">{activity.error?.message}</Notice>
        ) : tab === 'logs' ? (
          <LogsTab logs={activity.data.logs} onRefresh={() => void activity.reload()} />
        ) : (
          <RecordsTab key={tab} kind={tab} items={activity.data.history} />
        )}
      </div>
    </div>
  )
}
