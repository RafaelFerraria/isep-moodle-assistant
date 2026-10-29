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

describe('SettingsPage: perfil, email, regras, Moodle, notificações e verificações', () => {
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

  it('mostra o curso guardado e deixa mudá-lo com o botão Guardar', async () => {
    const { user, mock } = await loaded()
    expect(await screen.findByLabelText('Curso')).toHaveValue('Engenharia Informática')
    expect(screen.getByLabelText('Ano')).toHaveValue('3')
    await user.selectOptions(screen.getByLabelText('Ano'), '4')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    expect(mock.settings.student.year).toBe(4)
  })

  it('liga o email dentro da app a partir das Definições', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    mock.settings.email = { method: 'imap', address: '', connected: false }
    renderWithApi(<SettingsPage />, { mock })
    await user.type(await screen.findByLabelText('O teu email da escola'), 'aluno@escola.pt')
    await user.click(screen.getByRole('button', { name: 'Ligar o meu email' }))
    expect(await screen.findByLabelText('Código de autorização')).toHaveTextContent('ABCD-1234')
    mock.deviceApproved = true
    expect(await screen.findByText(/Email ligado/)).toBeInTheDocument()
    expect(mock.settings.email.address).toBe('aluno@escola.pt')
  })

  it('quem ainda lê pelo Thunderbird vê o aviso e pode passar a usar só a app', async () => {
    const { user } = await loaded()
    expect(await screen.findByText(/Estás a ler os emails pelo Thunderbird/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Passar a usar só a app' }))
    expect(await screen.findByRole('button', { name: 'Ligar o meu email' })).toBeInTheDocument()
  })

  it('edita as regras do resumo, valida o que foi escrito e só guarda quando está certo', async () => {
    const { user, mock } = await loaded()
    expect(await screen.findByRole('heading', { name: 'Regras do resumo' })).toBeInTheDocument()
    const important = screen.getByLabelText('Palavras importantes')
    const save = screen.getByRole('button', { name: /Guardar alterações/ })

    await user.type(important, 'x')
    expect(screen.getByText(/«x» é demasiado curto/)).toBeInTheDocument()
    expect(save).toBeDisabled()
    expect(screen.getByText('Corrige os campos assinalados para poderes guardar.')).toBeInTheDocument()

    await user.clear(important)
    await user.type(important, 'teste final{enter}Teste Final{enter}{enter}exame')
    await user.type(screen.getByLabelText('Remetentes a ignorar'), 'marketing@escola.pt')
    expect(save).toBeEnabled()
    await user.click(save)
    await screen.findByText(/Definições guardadas/)
    expect(mock.settings.summary.rules).toEqual({ important_keywords: ['teste final', 'exame'], ignore_senders: ['marketing@escola.pt'], ignore_subjects: [] })
    expect(screen.getByLabelText('Palavras importantes')).toHaveValue('teste final\nexame')
  })

  it('o limite de entradas e de tamanho também é validado', async () => {
    const { user } = await loaded()
    const field = await screen.findByLabelText('Assuntos a ignorar')
    await user.click(field)
    await user.paste(Array.from({ length: 31 }, (_, i) => `assunto${i}`).join('\n'))
    expect(screen.getByText(/No máximo 30 entradas \(tens 31\)/)).toBeInTheDocument()
    await user.clear(field)
    await user.paste('a'.repeat(61))
    expect(screen.getByText(/demasiado longo \(máximo 60 caracteres\)/)).toBeInTheDocument()
  })

  it('as regras só aparecem no modo Regras; com IA a configuração da IA fica logo ali', async () => {
    const { user } = await loaded()
    await screen.findByRole('heading', { name: 'Regras do resumo' })
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    expect(screen.queryByRole('heading', { name: 'Regras do resumo' })).not.toBeInTheDocument()
    expect(screen.getByText('Como ligar à IA')).toBeInTheDocument()
  })

  it('mostra a pasta de downloads predefinida e deixa desativar o descarregamento automático', async () => {
    const { user, mock } = await loaded()
    const folder = await screen.findByLabelText('Pasta de destino')
    expect(folder).toHaveTextContent('C:/Users/aluno/Downloads/Moodle_ISEP')
    expect(screen.getByText('Predefinida')).toBeInTheDocument()
    const toggle = screen.getByRole('checkbox', { name: /Descarregar automaticamente/ })
    expect(toggle).toBeChecked()
    await user.click(toggle)
    await waitFor(() => expect(mock.settings.moodle.auto_download).toBe(false))
    await waitFor(() => expect(screen.getByRole('checkbox', { name: /Descarregar automaticamente/ })).not.toBeChecked())
  })

  it('muda a pasta de downloads e repõe a predefinida', async () => {
    const { user, mock } = await loaded()
    await user.click(await screen.findByRole('button', { name: /Alterar pasta/ })) // sem seletor do sistema: abre o campo
    const field = screen.getByLabelText('Pasta de destino')
    await user.clear(field)
    await user.type(field, 'D:/Faculdade/Moodle')
    await user.click(screen.getByRole('button', { name: 'Guardar pasta' }))
    await waitFor(() => expect(mock.settings.moodle.download_dir).toBe('D:/Faculdade/Moodle'))
    expect(await screen.findByLabelText('Pasta de destino')).toHaveTextContent('D:/Faculdade/Moodle')
    expect(screen.queryByText('Predefinida')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Repor a predefinida/ }))
    await waitFor(() => expect(mock.settings.moodle.download_dir_is_default).toBe(true))
    expect(await screen.findByText('Predefinida')).toBeInTheDocument()
  })

  it('usa o seletor de pastas do sistema quando a app desktop o oferece', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport()
    window.isepBridge = { request: async () => ({ status: 500, data: null }), pickFolder: async () => 'E:/Escolhida' }
    try {
      renderWithApi(<SettingsPage />, { mock })
      await user.click(await screen.findByRole('button', { name: /Alterar pasta/ }))
      await waitFor(() => expect(mock.settings.moodle.download_dir).toBe('E:/Escolhida'))
    } finally {
      delete window.isepBridge
    }
  })

  it('mostra o Moodle da instituição e permite indicar outro endereço', async () => {
    const { user, mock } = await loaded()
    expect(await screen.findByText('Detetado pela tua instituição')).toBeInTheDocument()
    expect(screen.getByText('https://moodle.isep.ipp.pt')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Alterar' }))
    const field = screen.getByLabelText('Endereço do Moodle')
    await user.clear(field)
    await user.type(field, 'moodle.outra.pt')
    await user.click(screen.getByRole('button', { name: 'Verificar e guardar' }))
    await waitFor(() => expect(mock.settings.moodle.url).toBe('https://moodle.outra.pt'))
    expect(await screen.findByText('Indicado por ti')).toBeInTheDocument()
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
