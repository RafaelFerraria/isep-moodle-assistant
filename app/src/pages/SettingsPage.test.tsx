import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import type { Settings } from '../api'
import { renderWithApi } from '../test/utils'
import { SettingsPage, buildPatch, toDraft } from './SettingsPage'

const SID = '05' + 'ab'.repeat(32)

async function loaded() {
  const utils = renderWithApi(<SettingsPage />)
  await screen.findByRole('heading', { name: 'Definições' })
  return { ...utils, user: userEvent.setup() }
}

function lastSettingsPatch(mock: MockTransport) {
  const calls = mock.requests.filter((r) => r.method === 'PUT' && r.path === '/settings')
  return calls[calls.length - 1]?.body
}

describe('buildPatch', () => {
  const base = new MockTransport().settings as Settings

  it('não envia nada quando nada mudou', () => {
    expect(buildPatch(base, toDraft(base))).toEqual({})
  })

  it('envia só os campos alterados e os segredos apenas se foram escritos', () => {
    const d = { ...toDraft(base), mode: 'ai' as const, time: '09:30', newApiKey: ' sk-novo ', sessionId: ` ${SID} ` }
    expect(buildPatch(base, d)).toEqual({
      summary: { mode: 'ai', ai_api_key: 'sk-novo' },
      schedule: { time: '09:30' },
      destination: { session_id: SID },
    })
  })

  it('pede para remover o token só quando não há um novo', () => {
    expect(buildPatch(base, { ...toDraft(base), clearApiKey: true })).toEqual({ summary: { clear_ai_api_key: true } })
    expect(buildPatch(base, { ...toDraft(base), clearApiKey: true, newApiKey: 'k' })).toEqual({ summary: { ai_api_key: 'k' } })
  })

  it('converte o comando em lista para texto no rascunho', () => {
    const s = { ...base, summary: { ...base.summary, ai_cli_command: ['meu-cli', '-p'] } }
    expect(toDraft(s).aiCliCommand).toBe('meu-cli -p')
  })
})

describe('SettingsPage', () => {
  it('começa em modo Regras, sem campos de IA e sem alterações por guardar', async () => {
    await loaded()
    expect(screen.getByRole('radio', { name: 'Regras' })).toBeChecked()
    expect(screen.queryByLabelText('Fornecedor da API')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Guardar alterações/ })).toBeDisabled()
    expect(screen.getByText('Sem alterações por guardar.')).toBeInTheDocument()
  })

  it('escolher IA mostra o aviso de privacidade e as opções de ligação', async () => {
    const { user } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    expect(screen.getByText(/são enviados ao fornecedor escolhido/)).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Conta com login (CLI)' })).toBeChecked()
    expect(screen.getByLabelText('Ferramenta (CLI)')).toHaveValue('claude')
    expect(screen.getByRole('button', { name: /Guardar alterações/ })).toBeEnabled()
  })

  it('guarda a escolha de IA com CLI e confirma', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    await user.selectOptions(screen.getByLabelText('Ferramenta (CLI)'), 'gemini')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText(/Definições guardadas/)).toBeInTheDocument()
    expect(lastSettingsPatch(mock)).toEqual({ summary: { mode: 'ai', ai_cli: 'gemini' } })
    expect(mock.settings.summary).toMatchObject({ mode: 'ai', ai_cli: 'gemini' })
    expect(screen.getByRole('button', { name: /Guardar alterações/ })).toBeDisabled()
  })

  it('CLI personalizada exige o comando e mostra o erro no campo', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    await user.selectOptions(screen.getByLabelText('Ferramenta (CLI)'), 'custom')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    const field = await screen.findByLabelText('Comando')
    expect(field).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByText("obrigatório quando a CLI é 'custom'")).toBeInTheDocument()
    expect(mock.settings.summary.mode).toBe('rules') // nada foi gravado
    await user.type(field, 'meu-cli -p')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText(/Definições guardadas/)).toBeInTheDocument()
  })

  it('token de API: campo de palavra-passe, enviado ao guardar e nunca mostrado depois', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    await user.click(screen.getByRole('radio', { name: 'API com token' }))
    const key = screen.getByLabelText('Token de API')
    expect(key).toHaveAttribute('type', 'password')
    await user.type(key, 'sk-segredo-123')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    expect(mock.lastApiKey).toBe('sk-segredo-123')
    expect(screen.getByText(/Token guardado/)).toBeInTheDocument()
    expect(document.body.textContent).not.toContain('sk-segredo-123')
    expect(screen.queryByDisplayValue('sk-segredo-123')).not.toBeInTheDocument()
  })

  it('remover o token só acontece ao guardar', async () => {
    const mock = new MockTransport()
    mock.settings.summary = { ...mock.settings.summary, mode: 'ai', ai_provider: 'api', ai_api_key_set: true }
    const { user } = { ...renderWithApi(<SettingsPage />, { mock }), user: userEvent.setup() }
    await screen.findByText(/Token guardado/)
    await user.click(screen.getByRole('button', { name: 'Remover' }))
    expect(screen.getByText('O token vai ser removido ao guardar.')).toBeInTheDocument()
    expect(mock.settings.summary.ai_api_key_set).toBe(true)
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    expect(mock.settings.summary.ai_api_key_set).toBe(false)
  })

  it('API compatível exige URL e modelo', async () => {
    const { user } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    await user.click(screen.getByRole('radio', { name: 'API com token' }))
    await user.selectOptions(screen.getByLabelText('Fornecedor da API'), 'openai_compatible')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findAllByText('obrigatório para APIs compatíveis com OpenAI')).toHaveLength(2)
    expect(screen.getByLabelText('URL do endpoint')).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByLabelText('Modelo')).toHaveAttribute('aria-invalid', 'true')
  })

  it('valida a hora e mostra o erro junto ao campo', async () => {
    const { user, mock } = await loaded()
    const time = screen.getByLabelText('Hora do resumo diário')
    await user.clear(time)
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText('formato HH:MM')).toBeInTheDocument()
    expect(mock.settings.schedule.time).toBe('18:00')
    expect(screen.getByText('Há campos inválidos.')).toBeInTheDocument()
  })

  it('mudar a hora avisa que a agenda do Windows foi atualizada', async () => {
    const { user } = await loaded()
    const time = screen.getByLabelText('Hora do resumo diário')
    await user.clear(time)
    await user.type(time, '09:30')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText(/Agenda do Windows atualizada/)).toBeInTheDocument()
  })

  it('Session ID inválido é rejeitado e válido atualiza o destino sem o mostrar por inteiro', async () => {
    const { user } = await loaded()
    const field = screen.getByLabelText('Novo Session ID')
    await user.type(field, '123')
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    expect(await screen.findByText('deve ter 66 caracteres hexadecimais e começar por 05')).toBeInTheDocument()
    await user.clear(field)
    await user.type(field, SID)
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    expect(screen.getByText('05abab…abab', { exact: false })).toBeInTheDocument()
    expect(screen.getByLabelText('Novo Session ID')).toHaveValue('')
    expect(document.body.textContent).not.toContain(SID)
  })

  it('descartar repõe os valores guardados', async () => {
    const { user } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    await user.click(screen.getByRole('button', { name: /Descartar/ }))
    expect(screen.getByRole('radio', { name: 'Regras' })).toBeChecked()
    expect(screen.getByRole('button', { name: /Guardar alterações/ })).toBeDisabled()
  })

  it('o teste do resumo exige guardar primeiro e depois mostra o resultado', async () => {
    const { user } = await loaded()
    const test = screen.getByRole('button', { name: /Gerar resumo de exemplo/ })
    expect(test).toBeEnabled()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    expect(screen.getByRole('button', { name: /Gerar resumo de exemplo|Testar ligação à IA/ })).toBeDisabled()
    expect(screen.getByText('Guarda as alterações antes de testar.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await screen.findByText(/Definições guardadas/)
    await user.click(screen.getByRole('button', { name: /Testar ligação à IA/ }))
    expect(await screen.findByText('A IA respondeu corretamente.')).toBeInTheDocument()
    expect(screen.getByLabelText('Resumo de exemplo')).toHaveTextContent('RESUMO ISEP (IA)')
  })

  it('o teste avisa quando a IA falhou e o resumo caiu para as regras', async () => {
    const mock = new MockTransport()
    mock.settings.summary = { ...mock.settings.summary, mode: 'ai', ai_provider: 'api', ai_api_key_set: false }
    const user = userEvent.setup()
    renderWithApi(<SettingsPage />, { mock })
    await user.click(await screen.findByRole('button', { name: /Testar ligação à IA/ }))
    expect(await screen.findByText(/A IA não respondeu; o resumo caiu para o modo Regras/)).toBeInTheDocument()
  })

  it('valida e guarda as credenciais do Moodle sem as mostrar', async () => {
    const { user, mock } = await loaded()
    const card = screen.getByRole('heading', { name: 'Conta do Moodle' }).closest('section')!
    const save = within(card).getByRole('button', { name: 'Validar e guardar' })
    expect(save).toBeDisabled()
    await user.type(within(card).getByLabelText('Palavra-passe do Moodle'), 'errada')
    await user.click(save)
    expect(await within(card).findByText(/O Moodle recusou o login/)).toBeInTheDocument()
    await user.clear(within(card).getByLabelText('Palavra-passe do Moodle'))
    await user.type(within(card).getByLabelText('Palavra-passe do Moodle'), 'boa-password')
    await user.click(save)
    expect(await within(card).findByText(/Credenciais validadas e guardadas \(1234567\)/)).toBeInTheDocument()
    expect(within(card).getByLabelText('Palavra-passe do Moodle')).toHaveValue('')
    expect(JSON.stringify(mock.settings)).not.toContain('boa-password')
  })

  it('mostra o erro quando as definições não carregam', async () => {
    const mock = new MockTransport()
    mock.request = async () => ({ status: 500, data: { detail: 'sem disco' } })
    renderWithApi(<SettingsPage />, { mock })
    expect(await screen.findByText(/Não foi possível carregar as definições: sem disco/)).toBeInTheDocument()
  })

  it('erro de rede ao guardar aparece na barra e não perde o rascunho', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('radio', { name: 'IA' }))
    const original = mock.request.bind(mock)
    mock.request = async (method, path, body) =>
      method === 'PUT' && path === '/settings' ? { status: 500, data: { detail: 'disco cheio' } } : original(method, path, body)
    await user.click(screen.getByRole('button', { name: /Guardar alterações/ }))
    await waitFor(() => expect(screen.getByText('disco cheio')).toBeInTheDocument())
    expect(screen.getByRole('radio', { name: 'IA' })).toBeChecked()
  })
})
