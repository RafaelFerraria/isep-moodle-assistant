'use strict'
const { contextBridge, ipcRenderer } = require('electron')

// Única superfície exposta à página: pedidos à API local (o token fica no processo principal).
contextBridge.exposeInMainWorld('isepBridge', {
  request: (method, path, body) => ipcRenderer.invoke('isep:request', method, path, body),
  backend: () => ipcRenderer.invoke('isep:backend'),
})
