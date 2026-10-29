'use strict'
// Antes de empacotar: garante que desktop/vendor/thunderbird tem o Thunderbird oficial (descarregado, com o hash e a
// assinatura da Mozilla conferidos) para ir dentro da app. Reutiliza o código do servidor, que já faz essa verificação.
const { spawnSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')

const vendor = path.join(__dirname, '..', 'vendor', 'thunderbird')
const exe = path.join(vendor, 'app', 'thunderbird.exe')
if (fs.existsSync(exe) && fs.existsSync(path.join(vendor, 'version.txt'))) {
  console.log(`Thunderbird já em ${vendor} (${fs.readFileSync(path.join(vendor, 'version.txt'), 'utf8').trim()}); nada a descarregar.`)
  process.exit(0)
}
const server = path.join(__dirname, '..', '..', 'server', 'target', 'release', 'isep-assistant-api.exe')
if (!fs.existsSync(server)) {
  console.error('Falta o servidor compilado (npm run build:server).')
  process.exit(1)
}
fs.mkdirSync(vendor, { recursive: true })
const r = spawnSync(server, ['fetch-thunderbird', '--to', vendor], { stdio: 'inherit' })
process.exit(r.status ?? 1)
