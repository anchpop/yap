import { useState, useEffect } from 'react'

export function useIsInstalled() {
  const [isInstalled, setIsInstalled] = useState(false)
  const [isLoading, setIsLoading] = useState(true)

  useEffect(() => {
    const checkInstalled = () => {
      // Check if app is running in standalone mode (installed)
      const isStandalone = 
        // Standard PWA check
        window.matchMedia('(display-mode: standalone)').matches ||
        // iOS Safari check
        ('standalone' in window.navigator && window.navigator.standalone === true) ||
        // Additional check for when opened from home screen
        document.referrer.includes('android-app://') ||
        // Check if it's running in a WebView
        window.matchMedia('(display-mode: fullscreen)').matches ||
        window.matchMedia('(display-mode: minimal-ui)').matches

      setIsInstalled(isStandalone)
      setIsLoading(false)
    }

    checkInstalled()

    // Listen for changes in display mode
    const mediaQuery = window.matchMedia('(display-mode: standalone)')
    const handleChange = () => checkInstalled()
    
    // Modern browsers
    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener('change', handleChange)
    } else {
      // Older browsers
      mediaQuery.addListener(handleChange)
    }

    return () => {
      if (mediaQuery.removeEventListener) {
        mediaQuery.removeEventListener('change', handleChange)
      } else {
        mediaQuery.removeListener(handleChange)
      }
    }
  }, [])

  return { isInstalled, isLoading }
}