import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { EmailsPage } from './EmailsPage'

describe('EmailsPage', () => {
  it('lista os emails com remetente, pré-visualização e se já entraram num resumo', async () => {
    renderWithApi(<EmailsPage />)
    const list = await screen.findByRole('list', { name: 'Emails' })
    const items = within(list).getAllByRole('listitem')
    expect(items).toHaveLength(2)
    expect(items[0]).toHaveTextContent('LAPR3: Sprint 2 - Entrega')
    expect(items[0]).toHaveTextContent('A entrega do Sprint 2 é amanhã às 23:59.')
    expect(items[0]).not.toHaveTextContent('No resumo')
    expect(items[1]).toHaveTextContent('No resumo')
    expect(screen.getByText(/Origem: o teu email da escola, lido pela app/)).toBeInTheDocument()
  })

  it('quem ainda lê pelo Thunderbird vê essa origem', async () => {
    const mock = new MockTransport()
    mock.settings.email.method = 'thunderbird'
    renderWithApi(<EmailsPage />, { mock })
    expect(await screen.findByText(/Origem: Thunderbird/)).toBeInTheDocument()
  })

  it('muda o período e pede os dados ao servidor', async () => {
    const user = userEvent.setup()
    const { mock } = renderWithApi(<EmailsPage />)
    await screen.findByRole('list', { name: 'Emails' })
    await user.selectOptions(screen.getByLabelText('Período'), '30')
    await waitFor(() => expect(mock.requests.some((r) => r.path.startsWith('/emails?days=30'))).toBe(true))
  })

  it('quando a origem não está pronta explica porquê e aponta para as Definições', async () => {
    const mock = new MockTransport()
    mock.emailError = 'A conta Microsoft ainda não foi ligada. Liga-a nas Definições.'
    renderWithApi(<EmailsPage />, { mock })
    expect(await screen.findByText(/ainda não foi ligada/)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Abrir as Definições' })).toHaveAttribute('href', '/definicoes')
  })

  it('mostra uma mensagem quando não há emails', async () => {
    const mock = new MockTransport()
    mock.emails = []
    renderWithApi(<EmailsPage />, { mock })
    expect(await screen.findByText('Sem emails nos últimos 7 dia(s).')).toBeInTheDocument()
  })
})
