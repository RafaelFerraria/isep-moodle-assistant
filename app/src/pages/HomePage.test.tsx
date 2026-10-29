import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { HomePage } from './HomePage'

const NOW = () => new Date(2026, 9, 26, 19, 40)

describe('HomePage', () => {
  it('destaca o resumo de hoje e totaliza os últimos 7 dias', async () => {
    renderWithApi(<HomePage />, { now: NOW })
    const today = await screen.findByLabelText('Resumo de hoje')
    expect(today).toHaveTextContent('seg 26/10 16:00')
    const totals = screen.getByLabelText('Totais do período')
    expect(within(totals).getByText('Resumos').previousSibling).toHaveTextContent('2')
    expect(within(totals).getByText('Emails novos').previousSibling).toHaveTextContent('3')
    expect(within(totals).getByText('Ficheiros do Moodle').previousSibling).toHaveTextContent('1')
  })

  it('muda o período para 15 e 30 dias e pede os dados certos ao servidor', async () => {
    const user = userEvent.setup()
    const { mock } = renderWithApi(<HomePage />, { now: NOW })
    await screen.findByLabelText('Resumo de hoje')
    await user.click(screen.getByRole('radio', { name: '15 dias' }))
    await waitFor(() => expect(mock.requests.some((r) => r.path === '/overview?days=15')).toBe(true))
    await user.click(screen.getByRole('radio', { name: '30 dias' }))
    await waitFor(() => expect(mock.requests.some((r) => r.path === '/overview?days=30')).toBe(true))
    expect(screen.getByRole('radio', { name: '30 dias' })).toBeChecked()
  })

  it('lista os resumos do período e abre o texto de cada um', async () => {
    const user = userEvent.setup()
    renderWithApi(<HomePage />, { now: NOW })
    const list = await screen.findByRole('list', { name: 'Resumos do período' })
    const items = within(list).getAllByRole('listitem')
    expect(items).toHaveLength(2)
    await user.click(within(items[1]).getByText(/2 email\(s\)/))
    expect(within(items[1]).getByText(/LAPR3: Sprint 1/)).toBeVisible()
  })

  it('sem resumo hoje mostra o último e quando é o próximo envio', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.history = mock.history.filter((r) => r.id !== 'c3d4e5f6a1b2')
    renderWithApi(<HomePage />, { mock })
    expect(await screen.findByText('Ainda não há resumo de hoje.')).toBeInTheDocument()
    expect(screen.getByLabelText('Último resumo')).toHaveTextContent('LAPR3: Sprint 1')
    expect(screen.getByText(/Próximo envio amanhã às 18:00/)).toBeInTheDocument()
  })

  it('sem nenhum resumo no período explica que está vazio', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.history = []
    renderWithApi(<HomePage />, { mock })
    expect(await screen.findByText('Sem resumos nos últimos 7 dias.')).toBeInTheDocument()
  })

  it('avisa dos problemas obrigatórios e remete para as Definições', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.session.configured = false
    mock.settings.destination.session_configured = false
    renderWithApi(<HomePage />, { mock })
    expect(await screen.findByText('1 problema precisa de atenção')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'ver nas Definições' })).toHaveAttribute('href', '/definicoes')
  })

  it('pré-visualiza sem enviar e depois envia o resumo', async () => {
    const user = userEvent.setup()
    const { mock } = renderWithApi(<HomePage />, { now: NOW })
    await user.click(await screen.findByRole('button', { name: /Pré-visualizar resumo/ }))
    expect(await screen.findByLabelText('Pré-visualização do resumo')).toHaveTextContent('RESUMO ISEP')
    expect(mock.requests.some((r) => r.path === '/digest/run')).toBe(false)
    await user.click(screen.getByRole('button', { name: /Enviar resumo agora/ }))
    expect(await screen.findByText(/Resumo enviado para o Session/)).toBeInTheDocument()
  })

  it('desativa os botões enquanto há uma tarefa em curso', async () => {
    const user = userEvent.setup()
    renderWithApi(<HomePage />, { now: NOW, jobDurationMs: 200 })
    await user.click(await screen.findByRole('button', { name: /Enviar resumo agora/ }))
    expect(await screen.findByText('Em curso')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Pré-visualizar resumo/ })).toBeDisabled()
    await waitFor(() => expect(screen.getByText('Concluída')).toBeInTheDocument(), { timeout: 2000 })
  })

  it('mostra o erro e deixa tentar de novo quando o servidor falha', async () => {
    const mock = new MockTransport({ now: NOW })
    mock.request = async () => ({ status: 500, data: { detail: 'explodiu' } })
    renderWithApi(<HomePage />, { mock })
    expect(await screen.findByText(/Não foi possível obter o resumo: explodiu/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Tentar de novo/ })).toBeInTheDocument()
  })
})
