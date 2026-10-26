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
  it('mostra o estado e navega entre as três secções', async () => {
    const user = userEvent.setup()
    renderApp(new MockTransport())
    expect(await screen.findByRole('heading', { name: 'Estado' })).toBeInTheDocument()
    const nav = screen.getByRole('navigation', { name: 'Principal' })
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
    expect(await screen.findByRole('heading', { name: 'Estado' })).toBeInTheDocument()
  })

  it('pede para reiniciar quando o token é recusado', async () => {
    renderApp({ request: async () => ({ status: 401, data: { detail: 'Token em falta ou inválido.' } }) })
    expect(await screen.findByText(/O servidor recusou o token/)).toBeInTheDocument()
  })
})
