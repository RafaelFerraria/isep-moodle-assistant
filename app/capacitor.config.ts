import type { CapacitorConfig } from '@capacitor/cli'

// Para já a app só corre no desktop (Electron, ver ../desktop). Quando chegar a vez do telemóvel:
//   npm run build && npx cap add android && npx cap sync
const config: CapacitorConfig = {
  appId: 'pt.isep.assistant',
  appName: 'ISEP Assistant',
  webDir: 'dist',
}

export default config
