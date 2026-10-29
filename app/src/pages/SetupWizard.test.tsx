import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { SetupWizard } from './SetupWizard'

function fresh() {
  const mock = new MockTransport()
  mock.settings.setup.done = false
  mock.settings.email.method = 'graph'
  mock.session = { ...mock.session, bot_created: false, bot_id: '', recipients: [], configured: false }
  mock.moodle.configured = false
  return mock
}

describe('SetupWizard', () => {
  it('começa pelos emails e só deixa continuar com a conta Microsoft ligada', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    expect(await screen.findByRole('heading', { name: 'De onde vêm os teus emails?' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Continuar' })).toBeDisabled()

    await user.click(screen.getByRole('button', { name: /Ligar a conta Microsoft do ISEP/ }))
    expect(await screen.findByLabelText('Código de autorização')).toHaveTextContent('ABCD-1234')
    expect(screen.getByRole('link', { name: /microsoft.com\/devicelogin/ })).toHaveAttribute('href', 'https://microsoft.com/devicelogin')

    mock.deviceApproved = true
    expect(await screen.findByText(/Conta Microsoft ligada/)).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continuar' })).toBeEnabled())
  })

  it('com o Thunderbird basta continuar', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.settings.email.method = 'thunderbird'
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await screen.findByRole('heading', { name: 'De onde vêm os teus emails?' })
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    expect(await screen.findByRole('heading', { name: 'Conta do Moodle' })).toBeInTheDocument()
    expect(mock.requests.some((r) => r.method === 'PUT' && r.path === '/settings')).toBe(true)
  })

  it('percorre os cinco passos, cria o bot, agenda e conclui', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.settings.email.method = 'thunderbird'
    const done = vi.fn()
    renderWithApi(<SetupWizard onDone={done} />, { mock })

    await user.click(await screen.findByRole('button', { name: 'Continuar' })) // emails
    expect(await screen.findByRole('heading', { name: 'Conta do Moodle' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Saltar este passo' }))

    // Session: o passo não avança sem destinatário
    await screen.findByRole('button', { name: /Criar bot automaticamente/ })
    expect(screen.getByRole('button', { name: 'Continuar' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: /Criar bot automaticamente/ }))
    await screen.findByLabelText('ID do bot')
    await user.click(screen.getByRole('button', { name: /Detetar o meu Session/ }))
    await screen.findByText(/Session ligado/)
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continuar' })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: 'Continuar' }))

    // Resumo
    expect(await screen.findByRole('heading', { name: 'Como queres o resumo?' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Continuar' }))

    // Agendamento
    expect(await screen.findByRole('heading', { name: 'Quando queres receber o resumo?' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Concluir' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: /Ativar agendamento/ }))
    expect(await screen.findByText('Tarefas registadas.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Concluir' }))
    await waitFor(() => expect(done).toHaveBeenCalled())
    expect(mock.settings.setup.done).toBe(true)
    expect(mock.requests.some((r) => r.path === '/schedule/register')).toBe(true)
  })

  it('mostra em que passo está e deixa voltar atrás', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.settings.email.method = 'thunderbird'
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await user.click(await screen.findByRole('button', { name: 'Continuar' }))
    const steps = within(await screen.findByRole('navigation', { name: 'Passos da configuração' }))
    expect(steps.getByText('Moodle').closest('li')).toHaveAttribute('aria-current', 'step')
    await user.click(screen.getByRole('button', { name: 'Voltar' }))
    expect(await screen.findByRole('heading', { name: 'De onde vêm os teus emails?' })).toBeInTheDocument()
  })
})
