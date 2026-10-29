import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { SessionPage } from './SessionPage'

const SID = `05${'cd'.repeat(32)}`

describe('SessionPage', () => {
  it('com tudo configurado mostra o bot, o destinatário e deixa testar o envio', async () => {
    const user = userEvent.setup()
    renderWithApi(<SessionPage />)
    expect(await screen.findByLabelText('ID do bot')).toHaveTextContent('05bb11…c0de22')
    expect(within(screen.getByRole('list', { name: 'Destinatários' })).getByText('05a79f…d22077')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Enviar mensagem de teste/ }))
    expect(await screen.findByText(/Mensagem de teste enviada/)).toBeInTheDocument()
  })

  it('cria o bot automaticamente quando ainda não existe', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.session = { ...mock.session, bot_created: false, bot_id: '', recipients: [], configured: false }
    renderWithApi(<SessionPage />, { mock })
    expect(screen.queryByLabelText('ID do bot')).not.toBeInTheDocument()
    await user.click(await screen.findByRole('button', { name: /Criar bot automaticamente/ }))
    expect(await screen.findByLabelText('ID do bot')).toHaveTextContent('05cc22…beef33')
    expect(screen.getByRole('button', { name: /Enviar mensagem de teste/ })).toBeDisabled()
  })

  it('deteta o Session do utilizador quando ele escreve ao bot', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.session = { ...mock.session, recipients: [], configured: false }
    renderWithApi(<SessionPage />, { mock })
    await user.click(await screen.findByRole('button', { name: /Detetar o meu Session/ }))
    expect(await screen.findByText(/Session ligado/)).toBeInTheDocument()
    expect(within(await screen.findByRole('list', { name: 'Destinatários' })).getByText('05abab…abab')).toBeInTheDocument()
  })

  it('aceita um Session ID colado e rejeita um inválido', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.session = { ...mock.session, recipients: [], configured: false }
    renderWithApi(<SessionPage />, { mock })
    await user.click(await screen.findByText('Prefiro colar o meu Session ID'))
    const field = screen.getByLabelText('O meu Session ID')
    await user.type(field, '123')
    await user.click(screen.getByRole('button', { name: 'Adicionar' }))
    expect(await screen.findByText('O Session ID deve ter 66 caracteres e começar por 05.')).toBeInTheDocument()
    await user.clear(field)
    await user.type(field, SID)
    await user.click(screen.getByRole('button', { name: 'Adicionar' }))
    expect(await screen.findByText('05cdcd…cdcd')).toBeInTheDocument()
  })

  it('cria uma conta Session para quem não tem, mostra a frase de recuperação uma vez e associa-a', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.session = { ...mock.session, recipients: [], configured: false }
    renderWithApi(<SessionPage />, { mock })
    await user.click(await screen.findByText('Ainda não tenho o Session'))
    await user.click(screen.getByRole('button', { name: 'Criar a minha conta Session' }))
    expect(await screen.findByLabelText('Frase de recuperação')).toHaveTextContent('um dois tres')
    expect(screen.getByText(/Só a vês uma vez e a app não a guarda/)).toBeInTheDocument()
    expect(within(await screen.findByRole('list', { name: 'Destinatários' })).getByText('05efef…efef')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Já guardei a frase' }))
    expect(screen.queryByLabelText('Frase de recuperação')).not.toBeInTheDocument()
  })

  it('remove um destinatário', async () => {
    const user = userEvent.setup()
    renderWithApi(<SessionPage />)
    await user.click(await screen.findByRole('button', { name: 'Remover 05a79f…d22077' }))
    expect(await screen.findByText('Ainda não há destinatários.')).toBeInTheDocument()
  })

  it('avisa quando o Node.js não está instalado e não deixa criar o bot', async () => {
    const mock = new MockTransport()
    mock.session = { ...mock.session, node_ok: false, bot_created: false, bot_id: '', recipients: [], configured: false }
    renderWithApi(<SessionPage />, { mock })
    expect(await screen.findByText(/O Node.js não está instalado/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Criar bot automaticamente/ })).toBeDisabled()
  })
})
