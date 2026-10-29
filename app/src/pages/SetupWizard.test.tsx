import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { SetupWizard } from './SetupWizard'

/** Instalação nova: nada configurado. */
function fresh() {
  const mock = new MockTransport()
  mock.settings.setup.done = false
  mock.settings.student = { institution_id: '', institution_name: '', course: '', year: 0, configured: false }
  mock.settings.email = { method: 'imap', address: '', connected: false, password_set: false, oauth_connected: false, imap_host: '', imap_port: 993 }
  mock.settings.moodle = { ...mock.settings.moodle, url: null, url_source: 'missing' }
  mock.session = { ...mock.session, bot_created: false, bot_id: '', recipients: [], configured: false }
  mock.moodle.configured = false
  return mock
}

async function fillStudent(user: ReturnType<typeof userEvent.setup>, institution = 'uminho') {
  await user.selectOptions(await screen.findByLabelText('Instituição'), institution)
  await user.type(screen.getByLabelText('Curso'), 'Engenharia Informática')
  await user.selectOptions(screen.getByLabelText('Ano'), '2')
}

describe('SetupWizard', () => {
  it('começa por perguntar onde estudas e não avança sem instituição, curso e ano', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    expect(await screen.findByRole('heading', { name: 'Onde estudas?' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    expect(await screen.findByText('Escolhe a tua instituição.')).toBeInTheDocument()
    expect(screen.getByText('Indica o teu curso.')).toBeInTheDocument()
    expect(screen.getByText('Escolhe o ano.')).toBeInTheDocument()
    expect(mock.requests.some((r) => r.method === 'PUT')).toBe(false)
  })

  it('filtra as instituições pela pesquisa e guarda o perfil', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await user.type(await screen.findByLabelText('Pesquisar instituição'), 'minho')
    const select = screen.getByLabelText('Instituição')
    expect(within(select).getByRole('option', { name: 'Universidade do Minho' })).toBeInTheDocument()
    expect(within(select).queryByRole('option', { name: 'Universidade do Porto' })).not.toBeInTheDocument()
    await fillStudent(user)
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    expect(await screen.findByRole('heading', { name: 'O teu email' })).toBeInTheDocument()
    expect(mock.settings.student).toMatchObject({ institution_id: 'uminho', course: 'Engenharia Informática', year: 2, configured: true })
  })

  it('«outra instituição» pede o nome', async () => {
    const user = userEvent.setup()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock: fresh() })
    await user.selectOptions(await screen.findByLabelText('Instituição'), 'outra')
    await user.type(screen.getByLabelText('Curso'), 'Design')
    await user.selectOptions(screen.getByLabelText('Ano'), '1')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    expect(await screen.findByText('Indica o nome da instituição.')).toBeInTheDocument()
  })

  async function toEmailStep(user: ReturnType<typeof userEvent.setup>, mock: MockTransport) {
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await fillStudent(user)
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await screen.findByRole('heading', { name: 'O teu email' })
  }

  it('o email faz-se dentro da app: email, palavra-passe e servidor, sem Thunderbird nem janelas de terceiros', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    await toEmailStep(user, mock)
    expect(screen.queryByText(/Thunderbird/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Guardar e testar a ligação' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Configurar mais tarde' })).toBeInTheDocument()

    await user.type(screen.getByLabelText('O teu email da escola'), '1234567@isep.ipp.pt')
    // o servidor preenche-se sozinho pelo fornecedor do email
    expect(screen.getByLabelText('Servidor IMAP')).toHaveValue('outlook.office365.com')
    expect(screen.getByLabelText('Porta')).toHaveValue('993')
    expect(screen.getByRole('button', { name: 'Guardar e testar a ligação' })).toBeDisabled() // falta a palavra-passe

    await user.type(screen.getByLabelText('Palavra-passe do email'), 'segredo-da-escola')
    await user.click(screen.getByRole('button', { name: 'Guardar e testar a ligação' }))
    expect(await screen.findByText(/Ligado\. Encontrei 2 email\(s\) nos últimos 3 dias\./)).toBeInTheDocument()

    expect(mock.settings.email).toMatchObject({ method: 'imap', address: '1234567@isep.ipp.pt', imap_host: 'outlook.office365.com', imap_port: 993, password_set: true, connected: true })
    expect(mock.lastEmailPassword).toBe('segredo-da-escola')
    // a palavra-passe sai do campo e nunca é mostrada
    expect(screen.getByLabelText('Palavra-passe do email')).toHaveValue('')
    expect(document.body.textContent).not.toContain('segredo-da-escola')
    expect(await screen.findByRole('button', { name: 'Continuar' })).toBeEnabled()
  })

  it('um fornecedor desconhecido pede o servidor; com um servidor mal escrito mostra o erro', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    await toEmailStep(user, mock)
    await user.type(screen.getByLabelText('O teu email da escola'), 'aluno@escola-pequena.pt')
    expect(screen.getByLabelText('Servidor IMAP')).toHaveValue('')
    await user.type(screen.getByLabelText('Palavra-passe do email'), 'x')
    expect(screen.getByRole('button', { name: 'Guardar e testar a ligação' })).toBeDisabled()
    await user.type(screen.getByLabelText('Servidor IMAP'), 'sem espaços')
    await user.click(screen.getByRole('button', { name: 'Guardar e testar a ligação' }))
    expect(await screen.findByText('endereço do servidor inválido (ex.: outlook.office365.com)')).toBeInTheDocument()
    await user.clear(screen.getByLabelText('Servidor IMAP'))
    await user.type(screen.getByLabelText('Servidor IMAP'), 'mail.escola-pequena.pt')
    await user.click(screen.getByRole('button', { name: 'Guardar e testar a ligação' }))
    expect(await screen.findByText(/Ligado\./)).toBeInTheDocument()
    expect(mock.settings.email.imap_host).toBe('mail.escola-pequena.pt')
  })

  it('se o servidor recusar a palavra-passe, explica porquê e deixa corrigir', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.emailTest = { ok: false, error: 'Não foi possível ler os emails: o servidor recusou o email ou a palavra-passe (LOGIN failed.)' }
    await toEmailStep(user, mock)
    await user.type(screen.getByLabelText('O teu email da escola'), 'aluno@isep.ipp.pt')
    await user.type(screen.getByLabelText('Palavra-passe do email'), 'errada')
    await user.click(screen.getByRole('button', { name: 'Guardar e testar a ligação' }))
    expect(await screen.findByText(/recusou o email ou a palavra-passe/)).toBeInTheDocument()
    expect(document.body.textContent).not.toContain('errada')
    mock.emailTest = { ok: true, count: 5 }
    await user.type(screen.getByLabelText('Palavra-passe do email'), 'certa')
    await user.click(screen.getByRole('button', { name: 'Guardar e testar a ligação' }))
    expect(await screen.findByText(/Encontrei 5 email\(s\)/)).toBeInTheDocument()
  })

  it('no Moodle usa o da instituição quando existe e pede e valida o endereço quando não existe', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await fillStudent(user, 'uminho') // sem Moodle conhecido
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Configurar mais tarde' }))
    await screen.findByRole('heading', { name: 'Moodle' })
    expect(screen.getByText(/Não conheço o Moodle da tua instituição/)).toBeInTheDocument()

    const field = screen.getByLabelText('Endereço do Moodle')
    await user.type(field, 'https://exemplo.pt')
    await user.click(screen.getByRole('button', { name: 'Verificar e guardar' }))
    expect(await screen.findByText(/não parece ser um Moodle/)).toBeInTheDocument()
    await user.clear(field)
    await user.type(field, 'moodle.uminho.pt')
    await user.click(screen.getByRole('button', { name: 'Verificar e guardar' }))
    expect(await screen.findByText('https://moodle.uminho.pt')).toBeInTheDocument()
    expect(screen.getByText('Indicado por ti')).toBeInTheDocument()
  })

  it('percorre os seis passos, cria o bot, edita regras e conclui', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    const done = vi.fn()
    renderWithApi(<SetupWizard onDone={done} />, { mock })

    await fillStudent(user, 'ubi')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Configurar mais tarde' })) // email
    expect(await screen.findByText('Detetado pela tua instituição')).toBeInTheDocument() // Moodle da UBI
    await user.click(screen.getByRole('button', { name: 'Continuar' }))

    await screen.findByRole('button', { name: /Criar bot automaticamente/ })
    expect(screen.getByRole('button', { name: 'Continuar' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: /Criar bot automaticamente/ }))
    await screen.findByLabelText('ID do bot')
    await user.click(screen.getByRole('button', { name: /Detetar o meu Session/ }))
    await screen.findByText(/Session ligado/)
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continuar' })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: 'Continuar' }))

    // Resumo: regras editáveis à vista
    expect(await screen.findByRole('heading', { name: 'Regras do resumo' })).toBeInTheDocument()
    await user.type(screen.getByLabelText('Palavras importantes'), 'teste{enter}exame')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))

    expect(await screen.findByRole('heading', { name: 'Quando queres receber o resumo?' })).toBeInTheDocument()
    expect(mock.settings.summary.rules.important_keywords).toEqual(['teste', 'exame'])
    expect(screen.getByRole('button', { name: 'Concluir' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: /Ativar agendamento/ }))
    expect(await screen.findByText('Tarefas registadas.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Concluir' }))
    await waitFor(() => expect(done).toHaveBeenCalled())
    expect(mock.settings.setup.done).toBe(true)
  })

  it('a IA configura-se no próprio passo do resumo, sem ir às Definições', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.session = { ...mock.session, bot_created: true, bot_id: '05bb11…c0de22', recipients: ['05a79f…d22077'], configured: true }
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await fillStudent(user, 'ubi')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Configurar mais tarde' }))
    await user.click(await screen.findByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Continuar' })) // Session já configurado

    await user.click(await screen.findByRole('radio', { name: 'IA' }))
    expect(screen.getByText(/Com IA, o assunto e o início do texto dos emails/)).toBeInTheDocument()
    await user.click(screen.getByRole('radio', { name: 'API com token' }))
    await user.type(screen.getByLabelText('Token de API'), 'sk-teste')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await screen.findByRole('heading', { name: 'Quando queres receber o resumo?' })
    expect(mock.settings.summary).toMatchObject({ mode: 'ai', ai_provider: 'api', ai_api_key_set: true })
    expect(mock.lastApiKey).toBe('sk-teste')
  })

  it('regras inválidas bloqueiam o passo e explicam o que corrigir', async () => {
    const user = userEvent.setup()
    const mock = fresh()
    mock.session = { ...mock.session, bot_created: true, recipients: ['05a79f…d22077'], configured: true }
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock })
    await fillStudent(user, 'ubi')
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Configurar mais tarde' }))
    await user.click(await screen.findByRole('button', { name: 'Continuar' }))
    await user.click(await screen.findByRole('button', { name: 'Continuar' }))
    await user.type(await screen.findByLabelText('Remetentes a ignorar'), 'x')
    expect(screen.getByText(/«x» é demasiado curto/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    expect(screen.queryByRole('heading', { name: 'Quando queres receber o resumo?' })).not.toBeInTheDocument()
  })

  it('mostra em que passo está e deixa voltar atrás', async () => {
    const user = userEvent.setup()
    renderWithApi(<SetupWizard onDone={() => {}} />, { mock: fresh() })
    await fillStudent(user)
    await user.click(screen.getByRole('button', { name: 'Continuar' }))
    const steps = within(await screen.findByRole('navigation', { name: 'Passos da configuração' }))
    expect(steps.getByText('Email').closest('li')).toHaveAttribute('aria-current', 'step')
    await user.click(screen.getByRole('button', { name: 'Voltar' }))
    expect(await screen.findByRole('heading', { name: 'Onde estudas?' })).toBeInTheDocument()
  })
})
