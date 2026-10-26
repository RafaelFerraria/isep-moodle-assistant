import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App'
import { createApi } from './api'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App api={createApi()} />
  </StrictMode>,
)
