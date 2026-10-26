import { Activity, History, RefreshCw, Settings as SettingsIcon, WifiOff } from 'lucide-react'
import { HashRouter, NavLink, Navigate, Route, Routes } from 'react-router-dom'
import type { ReactNode } from 'react'
import { isDemoMode } from './api'
import type { AssistantApi } from './api'
import { Button, Notice, Spinner, cx } from './components/ui'
import { AppContext } from './lib/context'
import { useAsync } from './lib/hooks'
import { HistoryPage } from './pages/HistoryPage'
import { SettingsPage } from './pages/SettingsPage'
import { StatusPage } from './pages/StatusPage'
import { useApp } from './lib/context'

const NAV = [
  { to: '/', label: 'Estado', icon: Activity, end: true },
  { to: '/historico', label: 'Histórico', icon: History, end: false },
  { to: '/definicoes', label: 'Definições', icon: SettingsIcon, end: false },
]

function Sidebar({ version, demo }: { version: string; demo: boolean }) {
  return (
    <aside className="flex w-full shrink-0 flex-col border-b border-slate-200 bg-white lg:fixed lg:inset-y-0 lg:left-0 lg:w-64 lg:border-b-0 lg:border-r dark:border-slate-800 dark:bg-slate-900">
      <div className="flex items-center gap-3 px-5 py-5">
        <div className="grid size-9 place-items-center rounded-xl bg-brand-600 text-sm font-bold text-white" aria-hidden>
          IA
        </div>
        <div>
          <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">ISEP Assistant</p>
          <p className="text-xs text-slate-500">{demo ? 'Modo demonstração' : `v${version}`}</p>
        </div>
      </div>
      <nav aria-label="Principal" className="flex gap-1 px-3 pb-3 lg:flex-col">
        {NAV.map(({ to, label, icon: Icon, end }) => (
          <NavLink
            key={to}
            to={to}
            end={end}
            className={({ isActive }) =>
              cx(
                'flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition',
                isActive
                  ? 'bg-brand-50 text-brand-700 dark:bg-indigo-950/50 dark:text-indigo-300'
                  : 'text-slate-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800',
              )
            }
          >
            <Icon className="size-5" aria-hidden />
            {label}
          </NavLink>
        ))}
      </nav>
    </aside>
  )
}

/** Só mostra a app quando o servidor local responde; senão explica e deixa tentar de novo. */
function ConnectionGate({ children }: { children: (version: string) => ReactNode }) {
  const { api } = useApp()
  const health = useAsync(() => api.health(), [api])

  if (health.loading && !health.data) {
    return (
      <div className="grid h-full place-items-center" role="status">
        <div className="flex items-center gap-2 text-sm text-slate-500">
          <Spinner /> A ligar ao servidor local…
        </div>
      </div>
    )
  }
  if (!health.data) {
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="max-w-md space-y-4 text-center">
          <WifiOff className="mx-auto size-10 text-slate-400" aria-hidden />
          <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100">Sem ligação ao servidor local</h1>
          <Notice tone="bad">{health.error?.isUnauthorized ? 'O servidor recusou o token. Reinicia a aplicação.' : (health.error?.message ?? 'Erro desconhecido.')}</Notice>
          <p className="text-xs text-slate-500">O servidor arranca com a aplicação. Se o problema continuar, confirma que o processo «isep-assistant-api» está a correr.</p>
          <Button variant="primary" onClick={() => void health.reload()} icon={<RefreshCw className="size-4" />}>
            Tentar de novo
          </Button>
        </div>
      </div>
    )
  }
  return <>{children(health.data.version)}</>
}

export default function App({ api, pollMs = 1000 }: { api: AssistantApi; pollMs?: number }) {
  const demo = isDemoMode()
  return (
    <AppContext.Provider value={{ api, pollMs, demo }}>
      <HashRouter>
        <ConnectionGate>
          {(version) => (
            <div className="min-h-full lg:pl-64">
              <Sidebar version={version} demo={demo} />
              <main>
                <Routes>
                  <Route path="/" element={<StatusPage />} />
                  <Route path="/historico" element={<HistoryPage />} />
                  <Route path="/definicoes" element={<SettingsPage />} />
                  <Route path="*" element={<Navigate to="/" replace />} />
                </Routes>
              </main>
            </div>
          )}
        </ConnectionGate>
      </HashRouter>
    </AppContext.Provider>
  )
}
