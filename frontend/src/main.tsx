import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

// Font subsets trimmed to latin + latin-ext (M9 — offline font trim).
// @fontsource's weight-based `400.css` files also embedded the cyrillic/greek/
// vietnamese subsets; latin + latin-ext suffices for SkyComet's TR/EN UI.
// Archivo is the single UI + display family (weight 600 for headings); IBM Plex
// Mono stays for numeric/coordinate readouts.
import '@fontsource/archivo/latin-400.css'
import '@fontsource/archivo/latin-ext-400.css'
import '@fontsource/archivo/latin-500.css'
import '@fontsource/archivo/latin-ext-500.css'
import '@fontsource/archivo/latin-600.css'
import '@fontsource/archivo/latin-ext-600.css'
import '@fontsource/ibm-plex-mono/latin-400.css'
import '@fontsource/ibm-plex-mono/latin-ext-400.css'
import '@fontsource/ibm-plex-mono/latin-500.css'
import '@fontsource/ibm-plex-mono/latin-ext-500.css'

import './styles/tokens.css'
import './styles/base.css'
import App from './App.tsx'
import { ThemeProvider } from './theme/ThemeProvider'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </StrictMode>,
)
