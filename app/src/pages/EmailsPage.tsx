import { CheckCircle2, Mail, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { EmailsResult } from '../api'
import { Badge, Button, Card, Field, Notice, Select, Spinner } from '../components/ui'
import { fmtDateTime } from '../lib/format'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'

const SOURCE_LABEL: Record<string, string> = { thunderbird: 'Thunderbird (configuração anterior)', imap: 'o teu email da escola, lido pela app' }

export function EmailsPage() {
  const { api } = useApp()
  const [days, setDays] = useState(7)
  const emails = useAsync<EmailsResult>(() => api.emails(days, 100), [api, days])

  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Emails</h1>
          <p className="text-sm text-slate-500 dark:text-slate-400">
            {emails.data ? <>Origem: {SOURCE_LABEL[emails.data.source] ?? emails.data.source}. Só lês aqui; nada é apagado nem marcado.</> : 'Os emails carregados neste PC.'}
          </p>
        </div>
        <div className="flex items-end gap-2">
          <div className="w-40">
            <Field label="Período">
              {(p) => (
                <Select {...p} value={days} onChange={(e) => setDays(Number(e.target.value))}>
                  <option value={1}>Último dia</option>
                  <option value={3}>3 dias</option>
                  <option value={7}>7 dias</option>
                  <option value={15}>15 dias</option>
                  <option value={30}>30 dias</option>
                </Select>
              )}
            </Field>
          </div>
          <Button onClick={() => void emails.reload()} icon={<RefreshCw className="size-4" />}>
            Atualizar
          </Button>
        </div>
      </header>

      {emails.loading && !emails.data ? (
        <div className="flex items-center gap-2 text-sm text-slate-500" role="status">
          <Spinner /> A carregar os emails…
        </div>
      ) : !emails.data ? (
        <Notice tone="bad">Não foi possível obter os emails: {emails.error?.message}</Notice>
      ) : emails.data.error ? (
        <Notice tone="warn">
          {emails.data.error}{' '}
          <Link to="/definicoes" className="font-medium underline">
            Abrir as Definições
          </Link>
        </Notice>
      ) : emails.data.items.length === 0 ? (
        <Card>
          <p className="py-6 text-center text-sm text-slate-500">Sem emails nos últimos {days} dia(s).</p>
        </Card>
      ) : (
        <Card subtitle={`${emails.data.total} email(s)${emails.data.total > emails.data.items.length ? `, a mostrar ${emails.data.items.length}` : ''}`}>
          <ul aria-label="Emails" className="divide-y divide-slate-100 dark:divide-slate-800">
            {emails.data.items.map((m, i) => (
              <li key={`${m.message_id}-${i}`} className="flex gap-3 py-3">
                <Mail className="mt-0.5 size-5 shrink-0 text-slate-400" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="flex flex-wrap items-center gap-2 text-sm font-medium text-slate-900 dark:text-slate-100">
                    {m.subject}
                    {m.in_digest && (
                      <Badge tone="good">
                        <CheckCircle2 className="size-3" aria-hidden /> No resumo
                      </Badge>
                    )}
                  </p>
                  <p className="truncate text-xs text-slate-500">
                    {m.sender} · {fmtDateTime(m.received_time)}
                  </p>
                  {m.preview && <p className="mt-1 line-clamp-2 text-sm text-slate-600 dark:text-slate-400">{m.preview}</p>}
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  )
}
