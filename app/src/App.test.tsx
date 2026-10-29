import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { AssistantApi, MockTransport } from './api'
import type { Transport } from './api'
import App from './App'

function renderApp(transport: Transport) {
  return render(<App api={new AssistantApi(transport)} pollMs={5} />)
}

describe('App', () => {
  it('mostra o resumo e navega entre as secções', async () => {
    const user = userEvent.setup()
    renderApp(new MockTransport())
    expect(await screen.findByRole('heading', { name: 'Resumo' })).toBeInTheDocument()
    const nav = screen.getByRole('navigation', { name: 'Principal' })
    for (const [link, heading] of [['Emails', 'Emails'], ['Moodle', 'Moodle'], ['Session', 'Session']]) {
      await user.click(await screen.findByRole('link', { name: link }))
      expect(await screen.findByRole('heading', { level: 1, name: heading })).toBeInTheDocument()
    }
    await user.click(await screen.findByRole('link', { name: 'Histórico' }))
    expect(await screen.findByRole('heading', { name: 'Histórico' })).toBeInTheDocument()
    await user.click(screen.getByRole('link', { name: 'Definições' }))
    expect(await screen.findByRole('heading', { name: 'Definições' })).toBeInTheDocument()
    expect(nav).toBeInTheDocument()
  })

  it('explica a falta de ligação e deixa tentar de novo', async () => {
    const user = userEvent.setup()
    let up = false
    const mock = new MockTransport()
    renderApp({
      request: async (m, p, b) => {
        if (!up) throw new TypeError('ECONNREFUSED')
        return mock.request(m, p, b)
      },
    })
    expect(await screen.findByText('Sem ligação ao servidor local')).toBeInTheDocument()
    expect(screen.getByText('Sem ligação ao servidor local.')).toBeInTheDocument()
    up = true
    await user.click(screen.getByRole('button', { name: /Tentar de novo/ }))
    expect(await screen.findByRole('heading', { name: 'Resumo' })).toBeInTheDocument()
  })

  it('na primeira utilização abre o assistente de configuração em vez da app', async () => {
    const mock = new MockTransport()
    mock.settings.setup.done = false
    renderApp(mock)
    expect(await screen.findByRole('heading', { name: 'Vamos configurar o ISEP Assistant' })).toBeInTheDocument()
    expect(screen.queryByRole('navigation', { name: 'Principal' })).not.toBeInTheDocument()
  })

  it('pede para reiniciar quando o token é recusado', async () => {
    renderApp({ request: async () => ({ status: 401, data: { detail: 'Token em falta ou inválido.' } }) })
    expect(await screen.findByText(/O servidor recusou o token/)).toBeInTheDocument()
  })
})
