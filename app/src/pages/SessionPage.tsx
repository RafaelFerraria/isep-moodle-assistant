import { SessionCard } from '../components/SessionPanel'

export function SessionPage() {
  return (
    <div className="space-y-6 p-6 lg:p-8">
      <header>
        <h1 className="text-2xl font-semibold text-slate-900 dark:text-slate-100">Session</h1>
        <p className="text-sm text-slate-500 dark:text-slate-400">Cria o bot, liga o teu Session e testa o envio, tudo a partir da app.</p>
      </header>
      <SessionCard />
    </div>
  )
}
