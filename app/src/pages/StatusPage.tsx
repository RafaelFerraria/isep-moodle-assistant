import { AlertTriangle, CheckCircle2, CircleDashed, Eye, RefreshCw, Send, XCircle } from 'lucide-react'
import { Link } from 'react-router-dom'
import type { Check, HistoryRecord, Status } from '../api'
import { JobPanel } from '../components/JobPanel'
import { Badge, Button, Card, Notice, Spinner } from '../components/ui'
import { fmtRelative, parseLocal, fmtDateTime } from '../lib/format'
import { useApp } from '../lib/context'
import { useAsync, useJobRunner } from '../lib/hooks'

function CheckRow({ check }: { check: Check }) {
  const Icon = check.ok ? CheckCircle2 : check.required ? XCircle : CircleDashed
  const color = check.ok ? 'text-emerald-600' : check.required ? 'text-rose-600' : 'text-slate-400'
  const fixLink = !check.ok && (check.id === 'moodle' || check.id === 'ai' || check.id === 'session') ? '/definicoes' : null
  return (
    <li className="flex items-start gap-3 py-2">
      <Icon className={`mt-0.5 size-5 shrink-0 ${color}`} aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="text-sm text-slate-800 dark:text-slate-200">
          {check.label}
          <span className="sr-only">{check.ok ? ': ok' : check.required ? ': com problema' : ': opcional, não configurado'}</span>
        </p>
        {check.detail && <p className="text-xs text-slate-500 dark:text-slate-400">{check.detail}</p>}
      </div>
      {fixLink && (
        <Link to={fixLink} className="text-xs font-medium text-brand-600 hover:underline dark:text-indigo-400">
          Corrigir
        </Link>
      )}
    </li>
  )
}

function RunSummary({ record, now }: { record: HistoryRecord | null; now: Date }) {
  if (!record) return <p className="text-sm text-slate-500">Ainda não há execuções.</p>
  const d = record.details as Record<string, number | boolean | undefined>
  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={record.ok ? 'good' : 'bad'}>{record.ok ? 'Sucesso' : 'Falhou'}</Badge>
        <span className="text-sm text-slate-700 dark:text-slate-300">{fmtRelative(record.finished, now)}</span>
      </div>
      {record.kind === 'digest' && (
        <p className="text-xs text-slate-500 dark:text-slate-400">
          {d.skipped ? 'Sem novidades para enviar.' : `${d.emails_new ?? 0} email(s) novo(s), ${d.moodle_files ?? 0} ficheiro(s) do Moodle.`}
          {d.ai_fallback ? ' A IA falhou e usou-se o modo Regras.' : ''}
        </p>
      )}
      {record.kind === 'moodle_sync' && (
        <p className="text-xs text-slate-500 dark:text-slate-400">{d.new_files ?? 0} ficheiro(s) novo(s) ou atualizado(s).</p>
      )}
      {record.error && <p className="text-xs text-rose-600 dark:text-rose-400">{record.error}</p>}
    </div>
  )
}

export function StatusPage() {
  const { api } = useApp()
  const status = useAsync<Status>(() => api.status(), [api], 15000)
  const runner = useJobRunner(() => void status.reload())

  if (status.loading && !status.data) {
    return (
      <div className="flex items-center gap-2 p-8 text-sm text-slate-500" role="status">
        <Spinner /> A carregar o estado…
      </div>
    )
  }
  if (!status.data) {
    return (
      <div className="p-8">
        <Notice tone="bad">Não foi possível obter o estado: {status.error?.message}</Notice>
        <Button className="mt-4" onClick={() => void status.reload()} icon={<RefreshCw className="size-4" />}>
          Tentar de novo
        </Button>
      </div>
    )
  }

  const s = status.data
  const now = parseLocal(s.now) ?? new Date()
  const problems = s.checks.filter((c) => !c.ok && c.required)
  const busy = runner.running || !!s.active_job

  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Estado</h1>
          <p className="text-sm text-slate-500 dark:text-slate-400">Resumo diário do ISEP e sincronização do Moodle.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => void runner.start('digest_preview')} icon={<Eye className="size-4" />}>
            Pré-visualizar resumo
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void runner.start('digest')} icon={<Send className="size-4" />}>
            Enviar resumo agora
          </Button>
          <Button disabled={busy} onClick={() => void runner.start('moodle_sync')} icon={<RefreshCw className="size-4" />}>
            Sincronizar Moodle
          </Button>
        </div>
      </header>

      {problems.length === 0 ? (
        <Notice tone="good">
          <span className="flex items-center gap-2 font-medium">
            <CheckCircle2 className="size-5" aria-hidden /> Tudo a funcionar.
          </span>
        </Notice>
      ) : (
        <Notice tone="warn">
          <span className="flex items-center gap-2 font-medium">
            <AlertTriangle className="size-5" aria-hidden /> {problems.length === 1 ? '1 problema precisa de atenção' : `${problems.length} problemas precisam de atenção`}
          </span>
          <span className="mt-1 block text-xs">{problems.map((p) => p.label).join(' · ')}</span>
        </Notice>
      )}

      <JobPanel runner={runner} />

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <Card title="Próximo resumo">
          <p className="text-lg font-semibold text-slate-900 dark:text-slate-100">{fmtRelative(s.digest.next_run, now)}</p>
          <p className="text-xs text-slate-500 dark:text-slate-400">Agendado para as {s.digest.scheduled_time} todos os dias.</p>
        </Card>
        <Card title="Último resumo">
          <RunSummary record={s.digest.last} now={now} />
        </Card>
        <Card title="Moodle">
          <div className="space-y-1.5">
            <Badge tone={s.moodle.configured ? 'good' : 'warn'}>{s.moodle.configured ? `Ligado (${s.moodle.username})` : 'Sem credenciais'}</Badge>
            <RunSummary record={s.moodle.last} now={now} />
            {s.moodle.last_sync_date && <p className="text-xs text-slate-500">Último dia concluído: {fmtDateTime(s.moodle.last_sync_date)}</p>}
          </div>
        </Card>
        <Card title="Modo de resumo" action={<Link to="/definicoes" className="text-xs font-medium text-brand-600 hover:underline dark:text-indigo-400">Alterar</Link>}>
          <Badge tone={s.summary_mode.mode === 'ai' ? 'brand' : 'neutral'}>{s.summary_mode.mode === 'ai' ? 'IA' : 'Regras'}</Badge>
          <p className="mt-2 text-xs text-slate-500 dark:text-slate-400">{s.summary_mode.description}</p>
        </Card>
      </div>

      <Card title="Verificações" subtitle="O que é preciso para o resumo correr sozinho.">
        <ul className="divide-y divide-slate-100 dark:divide-slate-800">
          {s.checks.map((c) => (
            <CheckRow key={c.id} check={c} />
          ))}
        </ul>
      </Card>
    </div>
  )
}
