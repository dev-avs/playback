import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import Settings from './Settings.tsx'
import Changelog from './Changelog.tsx'
import Setup from './Setup.tsx'
import { DEFAULTS, KEYS } from './settings.ts'

// One bundle, four windows: settings loads this page with #settings, the
// changelog with #changelog (plus ?from=<version> after an update), setup with
// #setup.
const isSettings = location.hash === '#settings'
const isChangelog = location.hash.startsWith('#changelog')
const isSetup = location.hash === '#setup'

if (!isSettings && !isChangelog && !isSetup) {
  // Romanize starts each launch from its default; the settings window can flip
  // it afterwards. Seeded before the first render so the widget never flashes.
  let initial = DEFAULTS.romanizeDefault
  try {
    initial = JSON.parse(localStorage.getItem(KEYS.romanizeDefault) ?? 'null') ?? initial
  } catch {
    /* keep the default */
  }
  try {
    localStorage.setItem(KEYS.romanized, JSON.stringify(initial))
  } catch {
    /* storage unavailable — the widget falls back to off */
  }
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isSettings ? <Settings /> : isChangelog ? <Changelog /> : isSetup ? <Setup /> : <App />}
  </StrictMode>,
)
