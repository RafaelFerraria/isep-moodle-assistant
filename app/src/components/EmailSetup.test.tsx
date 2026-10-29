import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MockTransport } from '../api'
import { renderWithApi } from '../test/utils'
import { EmailSetup } from './EmailSetup'

function view(mock: MockTransport) {
  const onChanged = vi.fn()
  const utils = renderWithApi(<EmailSetup email={mock.settings.email} onChanged={onChanged} />, { mock })
  return { ...utils, onChanged, user: userEvent.setup() }
}

function unlinked() {
  const mock = new MockTransport()
  mock.settings.email = { method: 'imap', address: '', connected: false, password_set: false, oauth_connected: false, imap_host: '', imap_port: 993 }
  return mock
}

afterEach(() => {
  delete window.isepBridge
})

describe('EmailSetup: entrar com a conta da escola (Microsoft)', () => {
  it('abre o início de sessão numa janela da app e liga a conta sem pedir palavra-passe', async () => {
    const mock = unlinked()
    const oauthLogin = vi.fn(async () => 'https://localhost/?code=CODIGO&state=ESTADO')
    window.isepBridge = { request: async () => ({ status: 500, data: null }), oauthLogin }
    const { user } = view(mock)
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), '1234567@isep.ipp.pt')

    await user.click(await screen.findByRole('button', { name: 'Entrar com a conta da escola' }))

    await waitFor(() => expect(oauthLogin).toHaveBeenCalledTimes(1))
    const [authUrl, redirect] = oauthLogin.mock.calls[0] as unknown as [string, string]
    expect(authUrl).toMatch(/^https:\/\/login\.microsoftonline\.com\//)
    expect(authUrl).toContain(encodeURIComponent('1234567@isep.ipp.pt'))
    expect(redirect).toBe('https://localhost')
    expect(await screen.findByText(/Ligado\. Encontrei 2 email\(s\)/)).toBeInTheDocument()
    expect(mock.settings.email).toMatchObject({ oauth_connected: true, connected: true, method: 'imap', address: '1234567@isep.ipp.pt' })
    // o endereço de regresso (leva o código) foi para o servidor local, não ficou no ecrã
    const finish = mock.requests.find((r) => r.path === '/email/oauth/finish')?.body as { redirect_url: string }
    expect(finish.redirect_url).toContain('code=CODIGO')
    expect(document.body.textContent).not.toContain('CODIGO')
  })

  it('se o utilizador fechar a janela diz que cancelou e não liga nada', async () => {
    const mock = unlinked()
    window.isepBridge = { request: async () => ({ status: 500, data: null }), oauthLogin: async () => null }
    const { user } = view(mock)
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), 'aluno@isep.ipp.pt')
    await user.click(await screen.findByRole('button', { name: 'Entrar com a conta da escola' }))
    expect(await screen.findByText('Cancelaste o início de sessão.')).toBeInTheDocument()
    expect(mock.requests.some((r) => r.path === '/email/oauth/finish')).toBe(false)
    expect(mock.settings.email.oauth_connected).toBe(false)
  })

  it('mostra o erro da Microsoft quando o regresso não é aceite', async () => {
    const mock = unlinked()
    window.isepBridge = { request: async () => ({ status: 500, data: null }), oauthLogin: async () => 'https://localhost/?error=access_denied' }
    const { user } = view(mock)
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), 'aluno@isep.ipp.pt')
    await user.click(await screen.findByRole('button', { name: 'Entrar com a conta da escola' }))
    expect(await screen.findByText('Cancelaste o início de sessão.')).toBeInTheDocument()
  })

  it('fora da app de secretária deixa abrir a página à parte e colar o endereço de regresso', async () => {
    const mock = unlinked()
    const { user } = view(mock)
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), 'aluno@isep.ipp.pt')
    await user.click(await screen.findByRole('button', { name: 'Entrar com a conta da escola' }))
    const link = await screen.findByRole('link', { name: 'a página de entrada da Microsoft' })
    expect(link.getAttribute('href')).toMatch(/^https:\/\/login\.microsoftonline\.com\//)
    const concluir = screen.getByRole('button', { name: 'Concluir' })
    expect(concluir).toBeDisabled()
    await user.type(screen.getByLabelText('Endereço de regresso'), 'https://evil.example/?code=x')
    expect(concluir).toBeDisabled()
    await user.clear(screen.getByLabelText('Endereço de regresso'))
    await user.type(screen.getByLabelText('Endereço de regresso'), 'https://localhost/?code=abc&state=s')
    await user.click(concluir)
    expect(await screen.findByText(/Ligado\./)).toBeInTheDocument()
    expect(mock.settings.email.oauth_connected).toBe(true)
  })

  it('só oferece a entrada da Microsoft para emails Microsoft; os outros ficam com a palavra-passe', async () => {
    const mock = unlinked()
    const { user } = view(mock)
    expect(screen.queryByRole('button', { name: 'Entrar com a conta da escola' })).not.toBeInTheDocument()
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), 'aluno@escola-pequena.pt')
    expect(screen.queryByRole('button', { name: 'Entrar com a conta da escola' })).not.toBeInTheDocument()
    expect(screen.getByText('Entrar com palavra-passe')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Guardar e testar a ligação' })).toBeInTheDocument()
  })

  it('com a conta ligada mostra-o, deixa entrar de novo e terminar sessão', async () => {
    const mock = unlinked()
    mock.settings.email = { method: 'imap', address: 'aluno@isep.ipp.pt', connected: true, password_set: false, oauth_connected: true, imap_host: '', imap_port: 993 }
    const { user } = view(mock)
    expect(await screen.findByText('Conta da escola ligada.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Entrar de novo' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Terminar sessão' }))
    await waitFor(() => expect(mock.settings.email.oauth_connected).toBe(false))
  })

  it('explica que a palavra-passe pode estar desligada e aponta para o botão da Microsoft', async () => {
    const { user } = view(unlinked())
    await user.type(screen.getByLabelText('O teu email da escola', { selector: 'input' }), 'aluno@isep.ipp.pt')
    expect(await screen.findByText(/Basic authentication is disabled/)).toBeInTheDocument()
    expect(screen.getByText('Ou com palavra-passe')).toBeInTheDocument()
  })
})
