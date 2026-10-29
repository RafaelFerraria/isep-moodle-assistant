'use strict'
const { contextBridge, ipcRenderer } = require('electron')

// Única superfície exposta à página: pedidos à API local (o token fica no processo principal).
contextBridge.exposeInMainWorld('isepBridge', {
  request: (method, path, body) => ipcRenderer.invoke('isep:request', method, path, body),
  backend: () => ipcRenderer.invoke('isep:backend'),
  pickFolder: () => ipcRenderer.invoke('isep:pickFolder'),
  // abre o início de sessão da Microsoft numa janela da app e devolve o endereço de regresso (ou null se for cancelado)
  oauthLogin: (authUrl, redirectUri) => ipcRenderer.invoke('isep:oauth', authUrl, redirectUri),
})
