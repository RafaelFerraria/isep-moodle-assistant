'use strict'
const test = require('node:test')
const assert = require('node:assert/strict')
const { isAllowedAuthUrl, isRedirect, isAllowedNavigation, isValidRequestPattern, REQUEST_PATTERNS, REDIRECT } = require('../src/oauth.cjs')

test('só abre a página de início de sessão da Microsoft', () => {
  assert.equal(isAllowedAuthUrl('https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize?client_id=x'), true)
  for (const bad of [
    'http://login.microsoftonline.com/organizations/oauth2/v2.0/authorize',
    'https://login.microsoftonline.com.evil.example/authorize',
    'https://evil.example/https://login.microsoftonline.com/',
    'https://login.microsoftonline.com@evil.example/authorize',
    'file:///C:/Windows/System32',
    'javascript:alert(1)',
    '',
    null,
    undefined,
    42,
  ]) {
    assert.equal(isAllowedAuthUrl(bad), false, String(bad))
  }
})

test('reconhece o regresso a https://localhost e nada mais', () => {
  assert.equal(REDIRECT, 'https://localhost')
  assert.equal(isRedirect('https://localhost/?code=abc&state=xyz'), true)
  assert.equal(isRedirect('https://localhost?code=abc'), true)
  assert.equal(isRedirect('https://localhost:443/?code=abc'), true)
  for (const other of ['https://localhost.evil.example/?code=abc', 'https://evil.example/localhost', 'http://localhost/?code=abc', 'https://localhost:8443/', 'https://127.0.0.1/', 'lixo', '', null]) {
    assert.equal(isRedirect(other), false, String(other))
  }
})

test('a janela só navega por https', () => {
  assert.equal(isAllowedNavigation('https://login.microsoftonline.com/x'), true)
  assert.equal(isAllowedNavigation('https://sso.escola.pt/login'), true)
  for (const bad of ['http://sso.escola.pt', 'file:///C:/x', 'javascript:1', 'ftp://x', '', null]) assert.equal(isAllowedNavigation(bad), false, String(bad))
})

test('os padrões que o Electron recebe são válidos (o antigo «https://localhost?*» dava «empty path»)', () => {
  assert.ok(REQUEST_PATTERNS.length > 0)
  for (const p of REQUEST_PATTERNS) assert.equal(isValidRequestPattern(p), true, p)
  for (const bad of ['https://localhost?*', 'https://localhost', 'https://localhost:443', 'localhost/*', '', null]) {
    assert.equal(isValidRequestPattern(bad), false, String(bad))
  }
  // o regresso da Microsoft (https://localhost/?code=...) é apanhado por um dos padrões
  const url = new URL('https://localhost/?code=abc&state=x')
  assert.ok(REQUEST_PATTERNS.some((p) => p === `${url.protocol}//${url.host}/*`))
})
