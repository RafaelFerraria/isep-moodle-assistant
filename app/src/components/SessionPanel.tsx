import { Bot, Copy, Send, Trash2, UserPlus } from 'lucide-react'
import { useState } from 'react'
import { ApiError } from '../api'
import type { SessionAccount, SessionState } from '../api'
import { useApp } from '../lib/context'
import { useAsync, useJobRunner } from '../lib/hooks'
import { JobPanel } from './JobPanel'
import { Badge, Button, Card, Field, Notice, Spinner, TextInput } from './ui'

function Step({ n, done, title, children }: { n: number; done: boolean; title: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-4">
      <span
        aria-hidden
        className={`mt-0.5 grid size-7 shrink-0 place-items-center rounded-full text-xs font-semibold ${done ? 'bg-emerald-600 text-white' : 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-200'}`}
      >
        {done ? '✓' : n}
      </span>
      <div className="min-w-0 flex-1 space-y-3 pb-2">
        <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
          {title}
          <span className="sr-only">{done ? ' (concluído)' : ' (por fazer)'}</span>
        </h3>
        {children}
      </div>
    </li>
  )
}

/**
 * Configuração do Session em três passos: criar o bot (automático), ligar o teu Session (deteta o ID quando
 * lhe envias uma mensagem, ou colas o ID) e testar. Usado na página do Session e no assistente inicial.
 */
export function SessionPanel({ onChange }: { onChange?: (s: SessionState) => void }) {
  const { api } = useApp()
  const session = useAsync<SessionState>(async () => {
    const s = await api.session()
    onChange?.(s)
    return s
  }, [api])
  const pair = useJobRunner(() => void session.reload())
  const test = useJobRunner()
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [manualId, setManualId] = useState('')
  const [manualError, setManualError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [account, setAccount] = useState<SessionAccount | null>(null)
  const [creatingAccount, setCreatingAccount] = useState(false)

  if (session.loading && !session.data) {
    return (
      <div className="flex items-center gap-2 text-sm text-slate-500" role="status">
        <Spinner /> A carregar o Session…
      </div>
    )
  }
  if (!session.data) return <Notice tone="bad">Não foi possível obter o estado do Session: {session.error?.message}</Notice>
  const s = session.data
  const busy = pair.running || test.running

  const createBot = async () => {
    setCreating(true)
    setError(null)
    try {
      await api.createSessionBot()
      await session.reload()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setCreating(false)
    }
  }

  const addManual = async () => {
    setManualError(null)
    try {
      await api.addSessionRecipient(manualId.trim())
      setManualId('')
      await session.reload()
    } catch (e) {
      setManualError(e instanceof ApiError ? (e.fields.session_id ?? e.message) : String(e))
    }
  }

  const remove = async (index: number) => {
    setError(null)
    try {
      await api.removeSessionRecipient(index)
      await session.reload()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    }
  }

  const createAccount = async () => {
    setCreatingAccount(true)
    setError(null)
    try {
      setAccount(await api.createSessionAccount())
      await session.reload()
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setCreatingAccount(false)
    }
  }

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(s.bot_id)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className="space-y-5">
      {!s.node_ok && (
        <Notice tone="bad">
          O Node.js não está instalado e é preciso para falar com o Session. Instala-o em nodejs.org (versão LTS) e abre a app outra vez.
        </Notice>
      )}
      {!s.script_ok && <Notice tone="bad">Falta o ficheiro session_sender.cjs. Reinstala a app.</Notice>}
      {error && <Notice tone="bad">{error}</Notice>}

      <ol className="space-y-4">
        <Step n={1} done={s.bot_created} title="Bot do Session">
          {s.bot_created ? (
            <div className="space-y-2">
              <p className="text-sm text-slate-600 dark:text-slate-400">O bot é quem te envia os resumos. Este é o ID dele:</p>
              <div className="flex flex-wrap items-center gap-2">
                <code aria-label="ID do bot" className="break-all rounded-lg bg-slate-100 px-2.5 py-1.5 font-mono text-xs dark:bg-slate-800">
                  {s.bot_id}
                </code>
                <Button variant="ghost" onClick={() => void copy()} icon={<Copy className="size-4" />}>
                  {copied ? 'Copiado' : 'Copiar'}
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <p className="text-sm text-slate-600 dark:text-slate-400">A app cria uma conta de Session só para ti. Não precisas de fazer mais nada.</p>
              <Button variant="primary" onClick={() => void createBot()} loading={creating} disabled={!s.node_ok || !s.script_ok} icon={<Bot className="size-4" />}>
                Criar bot automaticamente
              </Button>
            </div>
          )}
        </Step>

        <Step n={2} done={s.configured} title="Ligar o teu Session">
          {!s.bot_created ? (
            <p className="text-sm text-slate-500">Cria primeiro o bot.</p>
          ) : (
            <div className="space-y-4">
              <ol className="list-inside list-decimal space-y-1 text-sm text-slate-600 dark:text-slate-400">
                <li>Abre o Session no telemóvel ou no PC.</li>
                <li>Inicia uma nova conversa com o ID do bot (copia-o acima).</li>
                <li>Envia uma mensagem qualquer, por exemplo «Olá».</li>
              </ol>
              <Button variant="primary" onClick={() => void pair.start('session_pair')} disabled={busy} loading={pair.running} icon={<UserPlus className="size-4" />}>
                Detetar o meu Session
              </Button>
              <JobPanel runner={pair} />
              <details className="text-sm">
                <summary className="cursor-pointer text-slate-600 dark:text-slate-400">Ainda não tenho o Session</summary>
                <div className="mt-3 space-y-3">
                  <p className="text-slate-600 dark:text-slate-400">
                    A app cria-te uma conta de Session. Depois instalas o Session (getsession.org) e restauras a conta com uma frase de recuperação que vou mostrar.
                  </p>
                  <Button onClick={() => void createAccount()} loading={creatingAccount} disabled={!s.node_ok || !s.script_ok}>
                    Criar a minha conta Session
                  </Button>
                </div>
              </details>
              {account && (
                <div role="alert" className="space-y-3 rounded-xl border border-amber-300 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950/40">
                  <p className="text-sm font-semibold text-amber-900 dark:text-amber-200">Guarda esta frase agora. Só a vês uma vez e a app não a guarda.</p>
                  <p aria-label="Frase de recuperação" className="select-all rounded-lg bg-white px-3 py-2 font-mono text-sm text-slate-900 dark:bg-slate-900 dark:text-slate-100">
                    {account.mnemonic}
                  </p>
                  <ol className="list-inside list-decimal space-y-1 text-sm text-amber-900 dark:text-amber-200">
                    <li>Instala o Session (getsession.org) no telemóvel ou no PC.</li>
                    <li>Escolhe «Restaurar conta» e cola a frase acima.</li>
                    <li>Aceita o pedido de mensagem do bot quando chegar o primeiro resumo.</li>
                  </ol>
                  <Button variant="secondary" onClick={() => setAccount(null)}>
                    Já guardei a frase
                  </Button>
                </div>
              )}
              <details className="text-sm">
                <summary className="cursor-pointer text-slate-600 dark:text-slate-400">Prefiro colar o meu Session ID</summary>
                <div className="mt-3 flex flex-wrap items-end gap-3">
                  <div className="min-w-64 flex-1">
                    <Field label="O meu Session ID" hint="66 caracteres, começa por 05." error={manualError ?? undefined}>
                      {(p) => <TextInput {...p} value={manualId} onChange={(e) => setManualId(e.target.value)} spellCheck={false} className="font-mono" />}
                    </Field>
                  </div>
                  <Button onClick={() => void addManual()} disabled={!manualId.trim()}>
                    Adicionar
                  </Button>
                </div>
              </details>
            </div>
          )}
        </Step>

        <Step n={3} done={false} title="Destinatários e teste">
          {s.recipients.length === 0 ? (
            <p className="text-sm text-slate-500">Ainda não há destinatários.</p>
          ) : (
            <ul aria-label="Destinatários" className="divide-y divide-slate-100 dark:divide-slate-800">
              {s.recipients.map((r, i) => (
                <li key={`${r}-${i}`} className="flex items-center gap-3 py-2">
                  <span className="flex-1 font-mono text-xs">{r}</span>
                  <Badge tone="good">Ativo</Badge>
                  <Button variant="ghost" onClick={() => void remove(i)} aria-label={`Remover ${r}`} icon={<Trash2 className="size-4" />}>
                    Remover
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <Button onClick={() => void test.start('session_test')} disabled={!s.configured || busy} loading={test.running} icon={<Send className="size-4" />}>
            Enviar mensagem de teste
          </Button>
          <JobPanel runner={test} />
        </Step>
      </ol>
    </div>
  )
}

export function SessionCard() {
  return (
    <Card title="Session" subtitle="Onde recebes o resumo. Tudo é configurado a partir daqui, sem terminal.">
      <SessionPanel />
    </Card>
  )
}
