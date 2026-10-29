import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { SettingsPage } from './SettingsPage'

async function loaded(mock = new MockTransport()) {
  const utils = renderWithApi(<SettingsPage />, { mock })
  await screen.findByRole('heading', { name: 'Definições' })
  return { ...utils, user: userEvent.setup() }
}

describe('SettingsPage: emails, notificações e verificações', () => {
  it('mostra Session ativo e Email, SMS e WhatsApp como «Em breve» (servidor)', async () => {
    await loaded()
    const list = await screen.findByRole('list', { name: 'Canais de notificação' })
    const items = within(list).getAllByRole('listitem')
    expect(items).toHaveLength(4)
    expect(items[0]).toHaveTextContent('Session')
    expect(items[0]).toHaveTextContent('Ativo')
    for (const [i, name] of [[1, 'Email'], [2, 'SMS'], [3, 'WhatsApp']] as const) {
      expect(items[i]).toHaveTextContent(name)
      expect(items[i]).toHaveTextContent('Em breve')
    }
    expect(within(items[0]).getByRole('link', { name: 'Configurar' })).toHaveAttribute('href', '/session')
  })

  it('muda a origem dos emails para a conta do ISEP e guarda', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'Conta do ISEP (serviço da app)' }))
    expect(screen.getByText(/Guarda as alterações para poderes ligar a conta/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    expect(mock.settings.email.method).toBe('graph')
    expect(await screen.findByRole('button', { name: /Ligar a conta Microsoft do ISEP/ })).toBeInTheDocument()
  })

  it('liga a conta Microsoft a partir das Definições', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.settings.email.method = 'graph'
    renderWithApi(<SettingsPage />, { mock })
    await user.click(await screen.findByRole('button', { name: /Ligar a conta Microsoft do ISEP/ }))
    expect(await screen.findByLabelText('Código de autorização')).toHaveTextContent('ABCD-1234')
    mock.deviceApproved = true
    expect(await screen.findByText(/Conta Microsoft ligada/)).toBeInTheDocument()
  })

  it('rejeita um endereço de email inválido', async () => {
    const { user } = await loaded()
    const field = screen.getByLabelText('O teu email do ISEP')
    await user.clear(field)
    await user.type(field, 'sem-arroba')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText('endereço de email inválido')).toBeInTheDocument()
  })

  it('mostra as verificações (movidas do antigo ecrã de estado) e distingue as opcionais', async () => {
    const mock = new MockTransport()
    mock.moodle.configured = false
    await loaded(mock)
    const checks = (await screen.findByRole('heading', { name: 'Verificações' })).closest('section')!
    await waitFor(() => expect(within(checks).getByText(/Thunderbird instalado/)).toBeInTheDocument())
    expect(within(checks).getByText(/Credenciais do Moodle/)).toBeInTheDocument()
    expect(within(checks).getByText(': opcional, não configurado')).toBeInTheDocument()
  })
})
