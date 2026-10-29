import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { MoodlePage } from './MoodlePage'

function running(mock: MockTransport) {
  mock.moodleProgress = {
    ...mock.moodleProgress,
    running: true, finished: null, courses_total: 4, courses_done: 1, current_course: 'DEI - Arquitetura de Computadores - 1º Semestre 2026_2027', percent: 25,
    counts: { pending: 1, downloading: 1, done: 1, unchanged: 0, error: 1 },
    items: [
      { course: 'AC', section: '01', name: 'a.pdf', state: 'done', size: 2048, error: null },
      { course: 'AC', section: '01', name: 'b.pdf', state: 'downloading', size: 0, error: null },
      { course: 'AC', section: '01', name: 'c.pdf', state: 'pending', size: 0, error: null },
      { course: 'AC', section: '02', name: 'd.pdf', state: 'error', size: 0, error: 'HTTP 500' },
    ],
  }
}

describe('MoodlePage', () => {
  it('mostra o progresso, a disciplina atual e os ficheiros por estado', async () => {
    const mock = new MockTransport()
    running(mock)
    renderWithApi(<MoodlePage />, { mock })
    const bar = await screen.findByRole('progressbar', { name: 'Progresso da sincronização' })
    expect(bar).toHaveAttribute('aria-valuenow', '25')
    expect(screen.getByText('Em curso')).toBeInTheDocument()
    expect(screen.getByText(/Disciplina 2 de 4/)).toBeInTheDocument()
    expect(screen.getByText(/Arquitetura de Computadores/)).toBeInTheDocument()
    expect(within(screen.getByRole('list', { name: 'Em progresso' })).getByText('b.pdf')).toBeInTheDocument()
    expect(within(screen.getByRole('list', { name: 'Pendentes' })).getByText('c.pdf')).toBeInTheDocument()
    const errors = screen.getByRole('list', { name: 'Com erro' })
    expect(within(errors).getByText('d.pdf')).toBeInTheDocument()
    expect(within(errors).getByText('HTTP 500')).toBeInTheDocument()
  })

  it('sem sincronização em curso mostra o resultado e os ficheiros já descarregados', async () => {
    renderWithApi(<MoodlePage />)
    expect(await screen.findByText('Concluída')).toBeInTheDocument()
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')
    expect(screen.getByRole('heading', { name: /Já descarregados/ })).toBeInTheDocument()
  })

  it('lista os ficheiros já descarregados com tipo e disciplina', async () => {
    renderWithApi(<MoodlePage />)
    const table = await screen.findByRole('table')
    expect(within(table).getByText('Enunciado Sprint 2.pdf')).toBeInTheDocument()
    expect(within(table).getByText('Laboratório / Projeto III')).toBeInTheDocument()
    expect(within(table).getByText('atualizado')).toBeInTheDocument()
    expect(within(table).getByText('471 KB')).toBeInTheDocument()
  })

  it('explica que o progresso é por ficheiro', async () => {
    renderWithApi(<MoodlePage />)
    expect(await screen.findByText(/cada ficheiro é descarregado inteiro/)).toBeInTheDocument()
  })

  it('antes da primeira sincronização diz que ainda não houve nenhuma', async () => {
    const mock = new MockTransport()
    mock.moodleProgress = { ...mock.moodleProgress, started: '', finished: null, percent: 0, courses_total: 0, courses_done: 0 }
    mock.newFiles = []
    renderWithApi(<MoodlePage />, { mock })
    expect(await screen.findByText('Ainda não houve nenhuma sincronização.')).toBeInTheDocument()
    expect(screen.getByText('Ainda não há ficheiros descarregados.')).toBeInTheDocument()
  })

  it('sincroniza agora e mostra o resultado', async () => {
    const user = userEvent.setup()
    renderWithApi(<MoodlePage />)
    await user.click(await screen.findByRole('button', { name: /Sincronizar agora/ }))
    expect(await screen.findByText(/Sincronização concluída: 1 ficheiro\(s\)/)).toBeInTheDocument()
  })

  it('sem credenciais do Moodle avisa e desativa a sincronização', async () => {
    const mock = new MockTransport()
    mock.moodle.configured = false
    renderWithApi(<MoodlePage />, { mock })
    expect(await screen.findByText(/Ainda não há credenciais do Moodle/)).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: /Sincronizar agora/ })).toBeDisabled())
  })
})
