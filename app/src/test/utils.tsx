import { render } from '@testing-library/react'
import type { ReactElement } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { AssistantApi, MockTransport } from '../api'
import type { MockOptions } from '../api'
import { AppContext } from '../lib/context'

/** Renderiza uma página com um servidor simulado (sem latência; tarefas terminam de imediato). */
export function renderWithApi(ui: ReactElement, options: MockOptions & { route?: string; mock?: MockTransport } = {}) {
  const mock = options.mock ?? new MockTransport({ jobDurationMs: options.jobDurationMs ?? 0, now: options.now })
  const api = new AssistantApi(mock)
  const utils = render(
    <AppContext.Provider value={{ api, pollMs: 5, demo: false }}>
      <MemoryRouter initialEntries={[options.route ?? '/']}>{ui}</MemoryRouter>
    </AppContext.Provider>,
  )
  return { ...utils, mock, api }
}
