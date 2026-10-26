import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { StatusPage } from './StatusPage'

const NOW = () => new Date(2026, 9, 26, 19, 40)

describe('StatusPage', () => {
  it('mostra o estado geral, os cartões e as verificações', async () => {
    renderWithApi(<StatusPage />, { now: NOW })
    expect(await screen.findByText('Tudo a funcionar.')).toBeInTheDocument()
    expect(screen.getByText('Próximo resumo')).toBeInTheDocument()
    expect(screen.getByText('amanhã às 18:00')).toBeInTheDocument()
    expect(screen.getByText('Ligado (1234567)')).toBeInTheDocument()
    expect(screen.getByText('Regras (local, sem enviar emails a terceiros)')).toBeInTheDocument()
    const checks = screen.getByRole('heading', { name: 'Verificações' }).closest('section')!
    expect(within(checks).getByText('Thunderbird instalado')).toBeInTheDocument()
  })

  it('avisa quando há problemas obrigatórios e aponta para as definições', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.settings.destination.session_configured = false
    renderWithApi(<StatusPage />, { mock })
    expect(await screen.findByText('1 problema precisa de atenção')).toBeInTheDocument()
    expect(screen.getAllByText('Destino Session configurado').length).toBeGreaterThan(0)
    expect(screen.getByRole('link', { name: 'Corrigir' })).toHaveAttribute('href', '/definicoes')
  })

  it('distingue verificações opcionais não configuradas dos problemas', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.moodle.configured = false
    renderWithApi(<StatusPage />, { mock })
    expect(await screen.findByText('Tudo a funcionar.')).toBeInTheDocument()
    expect(screen.getByText(/Credenciais do Moodle/)).toBeInTheDocument()
    expect(screen.getByText('Sem credenciais')).toBeInTheDocument()
  })

  it('pré-visualiza o resumo sem enviar', async () => {
    const user = userEvent.setup()
    const { mock } = renderWithApi(<StatusPage />, { now: NOW })
    await user.click(await screen.findByRole('button', { name: /Pré-visualizar resumo/ }))
    expect(await screen.findByLabelText('Pré-visualização do resumo')).toHaveTextContent('RESUMO ISEP')
    expect(mock.requests.some((r) => r.method === 'POST' && r.path === '/digest/preview')).toBe(true)
    expect(mock.requests.some((r) => r.path === '/digest/run')).toBe(false)
    expect(screen.getByText('Concluída')).toBeInTheDocument()
  })

  it('envia o resumo e mostra a confirmação', async () => {
    const user = userEvent.setup()
    renderWithApi(<StatusPage />, { now: NOW })
    await user.click(await screen.findByRole('button', { name: /Enviar resumo agora/ }))
    expect(await screen.findByText(/Resumo enviado para o Session/)).toBeInTheDocument()
  })

  it('sincroniza o Moodle e mostra o resultado', async () => {
    const user = userEvent.setup()
    renderWithApi(<StatusPage />, { now: NOW })
    await user.click(await screen.findByRole('button', { name: /Sincronizar Moodle/ }))
    expect(await screen.findByText(/Sincronização concluída: 1 ficheiro\(s\)/)).toBeInTheDocument()
  })

  it('desativa os botões enquanto há uma tarefa em curso', async () => {
    const user = userEvent.setup()
    renderWithApi(<StatusPage />, { now: NOW, jobDurationMs: 200 })
    await user.click(await screen.findByRole('button', { name: /Enviar resumo agora/ }))
    expect(await screen.findByText('Em curso')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Pré-visualizar resumo/ })).toBeDisabled()
    expect(screen.getByRole('button', { name: /Sincronizar Moodle/ })).toBeDisabled()
    await waitFor(() => expect(screen.getByText('Concluída')).toBeInTheDocument(), { timeout: 2000 })
    expect(screen.getByRole('button', { name: /Pré-visualizar resumo/ })).toBeEnabled()
  })

  it('segue a tarefa que já estava a correr em vez de falhar no 409', async () => {
    const user = userEvent.setup()
    const mock = new MockTransport({ now: NOW, jobDurationMs: 300 })
    renderWithApi(<StatusPage />, { mock })
    const send = await screen.findByRole('button', { name: /Enviar resumo agora/ })
    await mock.request('POST', '/moodle/sync') // outra janela arrancou uma sincronização entretanto
    await user.click(send)
    expect(await screen.findByText('Sincronização do Moodle')).toBeInTheDocument()
    expect(screen.queryByText(/Já existe uma tarefa em execução/)).not.toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('Concluída')).toBeInTheDocument(), { timeout: 2000 })
  })

  it('mostra o erro quando o servidor falha', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.request = async () => ({ status: 500, data: { detail: 'explodiu' } })
    renderWithApi(<StatusPage />, { mock })
    expect(await screen.findByText(/Não foi possível obter o estado: explodiu/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Tentar de novo/ })).toBeInTheDocument()
  })

  it('o último resumo indica quando a IA falhou e se usou o modo Regras', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.history.push({ ...mock.history[0], id: 'zz', finished: '2026-10-26T19:30:00', details: { emails_new: 1, moodle_files: 0, ai_fallback: true } })
    renderWithApi(<StatusPage />, { mock })
    expect(await screen.findByText(/A IA falhou e usou-se o modo Regras/)).toBeInTheDocument()
  })
})
