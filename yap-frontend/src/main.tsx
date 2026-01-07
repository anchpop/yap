import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'

// Hide the loading screen once React mounts
if (typeof window !== 'undefined' && (window as any).hideLoadingScreen) {
  (window as any).hideLoadingScreen();
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
