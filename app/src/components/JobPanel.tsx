import { X } from 'lucide-react'
import type { DigestResult, SyncResult } from '../api'
import type { JobRunner } from '../lib/hooks'
import { Badge, Button, Card, Notice, Pre, Spinner } from './ui'

const LABELS: Record<string, string> = {
  digest_preview: 'Pré-visualização do resumo',
  digest: 'Envio do resumo',
  moodle_sync: 'Sincronização do Moodle',
}

function DigestOutcome({ kind, result, error }: { kind: string; result: DigestResult | null; error: string | null }) {
  if (!result) return error ? <Notice tone="bad">{error}</Notice> : null
  if (result.error && !result.ok) return <Notice tone="bad">{result.error}</Notice>
  return (
    <div className="space-y-3">
      {result.ai_fallback && (
        <Notice tone="warn">A IA não respondeu; o resumo foi gerado pelo modo Regras. Verifica as definições do resumo.</Notice>
      )}
      {kind === 'digest' && result.sent && (
        <Notice tone="good">Resumo enviado para o Session ({result.emails_new} email(s) novo(s), {result.moodle_files} ficheiro(s) do Moodle).</Notice>
      )}
      {result.skipped && <Notice tone="info">Nada de novo para enviar desde a última vez.</Notice>}
      {kind === 'digest_preview' && result.summary && <Pre label="Pré-visualização do resumo">{result.summary}</Pre>}
      {kind === 'digest_preview' && !result.summary && result.skipped && null}
    </div>
  )
}

function SyncOutcome({ result, error }: { result: SyncResult | null; error: string | null }) {
  if (!result) return error ? <Notice tone="bad">{error}</Notice> : null
  if (result.busy) return <Notice tone="info">Já havia uma sincronização em curso.</Notice>
  if (!result.ok) return <Notice tone="bad">{result.error ?? 'A sincronização falhou.'}</Notice>
  return (
    <Notice tone="good">
      Sincronização concluída: {result.new_files} ficheiro(s) novo(s) ou atualizado(s), {result.errors} erro(s).
    </Notice>
  )
}

export function JobPanel({ runner }: { runner: JobRunner }) {
  const { job, error, running, dismiss } = runner
  if (!job && !error) return null
  const label = job ? (LABELS[job.kind] ?? job.kind) : 'Tarefa'

  return (
    <Card
      title={
        <span className="flex items-center gap-2">
          {label}
          {running ? (
            <Badge tone="brand">
              <Spinner className="size-3" /> Em curso
            </Badge>
          ) : job?.status === 'failed' || error ? (
            <Badge tone="bad">Falhou</Badge>
          ) : (
            <Badge tone="good">Concluída</Badge>
          )}
        </span>
      }
      action={
        !running && (
          <Button variant="ghost" onClick={dismiss} aria-label="Fechar" icon={<X className="size-4" />}>
            Fechar
          </Button>
        )
      }
    >
      <div className="space-y-3">
        {error && <Notice tone="bad">{error.message}</Notice>}
        {job && job.progress.length > 0 && (
          <Pre label="Progresso">{job.progress.join('\n')}</Pre>
        )}
        {job && job.status === 'failed' && <Notice tone="bad">{job.error ?? 'A tarefa falhou.'}</Notice>}
        {job && job.status === 'done' && job.kind.startsWith('digest') && (
          <DigestOutcome kind={job.kind} result={job.result as DigestResult | null} error={job.error} />
        )}
        {job && job.status === 'done' && job.kind === 'moodle_sync' && <SyncOutcome result={job.result as SyncResult | null} error={job.error} />}
      </div>
    </Card>
  )
}
