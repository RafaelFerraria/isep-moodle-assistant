'use strict'
// Regras do início de sessão da Microsoft numa janela da app (sem dependência do Electron, para se poder testar).

const AUTH_PREFIX = 'https://login.microsoftonline.com/'
const REDIRECT = 'https://localhost'

/** O primeiro endereço tem de ser o da Microsoft: a página não pode pedir à app que abra outro sítio. */
function isAllowedAuthUrl(url) {
  if (typeof url !== 'string' || !url.startsWith(AUTH_PREFIX)) return false
  try {
    const u = new URL(url)
    return u.protocol === 'https:' && u.hostname === 'login.microsoftonline.com'
  } catch {
    return false
  }
}

/** É o regresso à app (https://localhost, com ou sem caminho e parâmetros)? */
function isRedirect(url) {
  if (typeof url !== 'string') return false
  try {
    const u = new URL(url)
    return u.protocol === 'https:' && u.hostname === 'localhost' && (u.port === '' || u.port === '443')
  } catch {
    return false
  }
}

/** Dentro da janela só se navega por https (os sítios de início de sessão da escola podem estar noutros domínios). */
function isAllowedNavigation(url) {
  return typeof url === 'string' && url.startsWith('https://')
}

/**
 * Padrões de endereço para interceptar o regresso no Electron. O Electron só aceita padrões com caminho depois do endereço
 * (`https://localhost?*` dá «empty path»), por isso os regressos sem barra são apanhados por `will-redirect` / `will-navigate`.
 */
const REQUEST_PATTERNS = ['https://localhost/*', 'https://localhost:443/*']

/** O Electron aceita o padrão? (esquema, anfitrião e caminho não vazio, como as «match patterns» do Chrome) */
function isValidRequestPattern(pattern) {
  return typeof pattern === 'string' && /^(https?|\*):\/\/(\*|(\*\.)?[^/*?:]+)(:\d+)?\/.*$/.test(pattern)
}

module.exports = { AUTH_PREFIX, REDIRECT, REQUEST_PATTERNS, isValidRequestPattern, isAllowedAuthUrl, isRedirect, isAllowedNavigation }
