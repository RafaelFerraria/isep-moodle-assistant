import { createContext, useContext } from 'react'
import type { AssistantApi } from '../api'

export interface AppContextValue {
  api: AssistantApi
  /** Intervalo de consulta das tarefas em segundo plano (ms). Os testes usam um valor pequeno. */
  pollMs: number
  demo: boolean
}

export const AppContext = createContext<AppContextValue | null>(null)

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('useApp fora do <AppContext.Provider>')
  return ctx
}
