import { AlertTriangle, Eye, RefreshCw, Send } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { HistoryRecord, Overview, Status } from '../api'
import { JobPanel } from '../components/JobPanel'
import { Badge, Button, Card, Notice, Pre, Spinner, cx } from '../components/ui'
import { fmtDateTime, fmtRelative, parseLocal } from '../lib/format'
import { useApp } from '../lib/context'
import { useAsync, useJobRunner } from '../lib/hooks'

const PERIODS = [7, 15, 30] as const

function counts(r: HistoryRecord) {
  const d = r.details as Record<string, number | boolean | undefined>
  return d.skipped ? 'Sem novidades' : `${d.emails_new ?? 0} email(s), ${d.moodle_files ?? 0} ficheiro(s) do Moodle`
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-xl bg-slate-50 p-4 dark:bg-slate-800/60">
      <p className="text-2xl font-semibold text-slate-900 dark:text-slate-100">{value}</p>
      <p className="text-xs text-slate-500">{label}</p>
    </div>
  )
}

function Highlight({ overview, status }: { overview: Overview; status: Status | null }) {
  const now = parseLocal(status?.now) ?? new Date()
  const today = overview.today
  if (today) {
    return (
      <Card
        title="Resumo de hoje"
        subtitle={`Enviado ${fmtRelative(today.finished, now)} · ${counts(today)}`}
        action={<Badge tone={today.ok ? 'good' : 'bad'}>{today.ok ? 'Enviado' : 'Falhou'}</Badge>}
        className="border-brand-200 ring-1 ring-brand-500/10 dark:border-indigo-900"
      >
        <Pre label="Resumo de hoje">{today.summary}</Pre>
      </Card>
    )
  }
  return (
    <Card title="Resumo de hoje" subtitle={status?.digest.next_run ? `Próximo envio ${fmtRelative(status.digest.next_run, now)}.` : undefined}>
      <p className="text-sm text-slate-600 dark:text-slate-400">Ainda não há resumo de hoje.</p>
      {overview.latest && (
        <div className="mt-4 space-y-2">
          <p className="text-xs font-medium text-slate-500">Último resumo ({fmtDateTime(overview.latest.finished)})</p>
          <Pre label="Último resumo">{overview.latest.summary}</Pre>
        </div>
      )}
    </Card>
  )
}

export function HomePage() {
  const { api } = useApp()
  const [days, setDays] = useState<(typeof PERIODS)[number]>(7)
  const overview = useAsync<Overview>(() => api.overview(days), [api, days])
  const status = useAsync<Status>(() => api.status(), [api], 30000)
  const runner = useJobRunner(() => {
    void overview.reload()
    void status.reload()
  })
  const busy = runner.running || !!status.data?.active_job

  if (overview.loading && !overview.data) {
    return (
      <div className="flex items-center gap-2 p-8 text-sm text-slate-500" role="status">
        <Spinner /> A carregar o resumo…
      </div>
    )
  }
  if (!overview.data) {
    return (
      <div className="p-8">
        <Notice tone="bad">Não foi possível obter o resumo: {overview.error?.message}</Notice>
        <Button className="mt-4" onClick={() => void overview.reload()} icon={<RefreshCw className="size-4" />}>
          Tentar de novo
        </Button>
      </div>
    )
  }

  const o = overview.data
  const problems = (status.data?.checks ?? []).filter((c) => !c.ok && c.required)

  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Resumo</h1>
          <p className="text-sm text-slate-500 dark:text-slate-400">O que há de novo nos teus emails e no Moodle.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => void runner.start('digest_preview')} icon={<Eye className="size-4" />}>
            Pré-visualizar resumo
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void runner.start('digest')} icon={<Send className="size-4" />}>
            Enviar resumo agora
          </Button>
        </div>
      </header>

      {problems.length > 0 && (
        <Notice tone="warn">
          <span className="flex items-center gap-2 font-medium">
            <AlertTriangle className="size-5" aria-hidden /> {problems.length === 1 ? '1 problema precisa de atenção' : `${problems.length} problemas precisam de atenção`}
          </span>
          <span className="mt-1 block text-xs">
            {problems.map((p) => p.label).join(' · ')} —{' '}
            <Link to="/definicoes" className="font-medium underline">
              ver nas Definições
            </Link>
          </span>
        </Notice>
      )}

      <JobPanel runner={runner} />
      <Highlight overview={o} status={status.data} />

      <section aria-labelledby="period-title" className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 id="period-title" className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            Últimos dias
          </h2>
          <div role="radiogroup" aria-label="Período" className="inline-flex rounded-lg border border-slate-200 p-0.5 dark:border-slate-700">
            {PERIODS.map((p) => (
              <label
                key={p}
                className={cx(
                  'cursor-pointer rounded-md px-3 py-1.5 text-sm font-medium transition',
                  days === p ? 'bg-brand-600 text-white' : 'text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800',
                )}
              >
                <input type="radio" name="period" className="sr-only" checked={days === p} onChange={() => setDays(p)} aria-label={`${p} dias`} />
                {p} dias
              </label>
            ))}
          </div>
        </div>

        <dl className="grid grid-cols-2 gap-3 md:grid-cols-4" aria-label="Totais do período">
          <Stat label="Resumos" value={o.totals.runs} />
          <Stat label="Emails novos" value={o.totals.emails} />
          <Stat label="Ficheiros do Moodle" value={o.totals.moodle_files} />
          <Stat label="Falhas" value={o.totals.failed} />
        </dl>

        {o.items.length === 0 ? (
          <Card>
            <p className="py-6 text-center text-sm text-slate-500">Sem resumos nos últimos {days} dias.</p>
          </Card>
        ) : (
          <Card>
            <ul aria-label="Resumos do período" className="divide-y divide-slate-100 dark:divide-slate-800">
              {o.items.map((r) => (
                <li key={r.id} className="py-2">
                  <details>
                    <summary className="flex cursor-pointer flex-wrap items-center gap-3 py-1 text-sm">
                      <span className="font-medium text-slate-900 dark:text-slate-100">{fmtDateTime(r.started)}</span>
                      <span className="text-slate-500">{r.ok ? counts(r) : (r.error ?? 'Falhou')}</span>
                      {!r.ok && <Badge tone="bad">Falhou</Badge>}
                    </summary>
                    {r.summary ? (
                      <div className="mt-2">
                        <Pre label={`Resumo de ${fmtDateTime(r.started)}`}>{r.summary}</Pre>
                      </div>
                    ) : (
                      <p className="mt-2 text-xs text-slate-500">Sem texto de resumo nesta execução.</p>
                    )}
                  </details>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </section>
    </div>
  )
}
