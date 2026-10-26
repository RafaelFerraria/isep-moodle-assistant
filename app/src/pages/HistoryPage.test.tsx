import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { HistoryPage } from './HistoryPage'

async function loaded(mock = new MockTransport()) {
  const utils = renderWithApi(<HistoryPage />, { mock })
  await screen.findByRole('heading', { name: 'Histórico' })
  return { ...utils, user: userEvent.setup() }
}

describe('HistoryPage', () => {
  it('lista os resumos, mais recentes primeiro', async () => {
    await loaded()
    const list = await screen.findByRole('list', { name: 'Execuções' })
    const items = within(list).getAllByRole('listitem')
    expect(items).toHaveLength(2)
    expect(items[0]).toHaveTextContent('Resumo enviado (1 email(s))')
    expect(items[0]).toHaveTextContent('Manual')
    expect(items[1]).toHaveTextContent('Resumo enviado (2 email(s))')
    expect(items[1]).toHaveTextContent('Agendado')
  })

  it('mostra o resumo enviado ao escolher uma execução', async () => {
    const { user } = await loaded()
    expect(screen.getByText('Escolhe uma execução para ver o detalhe.')).toBeInTheDocument()
    await user.click(await screen.findByRole('button', { name: /Resumo enviado \(2 email\(s\)\)/ }))
    expect(await screen.findByLabelText('Resumo enviado')).toHaveTextContent('LAPR3: Sprint 1')
    expect(screen.getByText('Emails novos').previousSibling).toHaveTextContent('2')
  })

  it('separa as sincronizações e mostra o erro de uma que falhou', async () => {
    const { user } = await loaded()
    await user.click(screen.getByRole('tab', { name: 'Sincronizações' }))
    const row = await screen.findByRole('button', { name: /Sincronização falhou/ })
    await user.click(row)
    expect(await screen.findByText('2 erro(s) durante a sincronização')).toBeInTheDocument()
    expect(screen.getByLabelText('Falhou')).toBeInTheDocument()
  })

  it('lista os ficheiros do Moodle com tipo e disciplina', async () => {
    const { user } = await loaded()
    await user.click(screen.getByRole('tab', { name: 'Ficheiros do Moodle' }))
    const table = await screen.findByRole('table')
    expect(within(table).getByText('Enunciado Sprint 2.pdf')).toBeInTheDocument()
    expect(within(table).getByText('Laboratório / Projeto III')).toBeInTheDocument()
    expect(within(table).getByText('atualizado')).toBeInTheDocument()
    expect(within(table).getByText('471 KB')).toBeInTheDocument()
  })

  it('mostra os registos e deixa mudar de ficheiro', async () => {
    const { user, mock } = await loaded()
    await user.click(screen.getByRole('tab', { name: 'Registos' }))
    expect(await screen.findByLabelText('Registos')).toHaveTextContent('(mock) sem registos reais')
    await user.selectOptions(screen.getByLabelText('Ficheiro de registo'), 'sync')
    await screen.findByLabelText('Registos')
    expect(mock.requests.some((r) => r.path.startsWith('/logs/sync'))).toBe(true)
  })

  it('mostra uma mensagem quando não há histórico', async () => {
    const mock = new MockTransport()
    mock.history = []
    await loaded(mock)
    expect(await screen.findByText('Ainda não há execuções registadas.')).toBeInTheDocument()
  })

  it('assinala quando a IA falhou numa execução', async () => {
    const mock = new MockTransport()
    mock.history[2].details = { ...mock.history[2].details, ai_fallback: true }
    const { user } = await loaded(mock)
    await user.click(await screen.findByRole('button', { name: /Resumo enviado \(1 email\(s\)\)/ }))
    expect(await screen.findByText('A IA falhou e usou-se o modo Regras.')).toBeInTheDocument()
  })
})
