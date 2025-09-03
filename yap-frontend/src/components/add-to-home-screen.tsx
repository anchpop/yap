import { useState, useEffect } from 'react'
import { Button } from "@/components/ui/button"
import { Home, Share, Plus, MoreVertical } from 'lucide-react'

interface AddToHomeScreenProps {
  onDismiss?: () => void
}

export function AddToHomeScreen({ onDismiss }: AddToHomeScreenProps) {
  const [platform, setPlatform] = useState<'ios' | 'android' | 'desktop' | 'unknown'>('unknown')
  const [showInstructions, setShowInstructions] = useState(false)

  useEffect(() => {
    const userAgent = navigator.userAgent.toLowerCase()
    if (/iphone|ipad|ipod/.test(userAgent)) {
      setPlatform('ios')
    } else if (/android/.test(userAgent)) {
      setPlatform('android')
    } else if (/windows|mac|linux/.test(userAgent) && !(/mobile/.test(userAgent))) {
      setPlatform('desktop')
    }
  }, [])

  const handleShowInstructions = () => {
    setShowInstructions(true)
  }

  const handleDismiss = () => {
    setShowInstructions(false)
    onDismiss?.()
  }

  if (!showInstructions) {
    return (
      <div className="space-y-2">
        <Button
          onClick={handleShowInstructions}
          variant="default"
          size="sm"
          className="w-full sm:w-auto"
        >
          <Home className="mr-2 h-4 w-4" />
          Add to Home Screen
        </Button>
      </div>
    )
  }

  return (
    <div className="space-y-4 p-4 bg-muted rounded-lg">
      <h3 className="font-semibold flex items-center gap-2">
        <Home className="h-5 w-5" />
        Add Yap.Town to Home Screen
      </h3>

      {platform === 'ios' && (
        <div className="space-y-3 text-sm">
          <p>To add this app to your home screen:</p>
          <ol className="space-y-2 ml-4">
            <li className="flex items-start gap-2">
              <Share className="h-4 w-4 mt-0.5 shrink-0" />
              <span>Tap the Share button at the bottom of Safari</span>
            </li>
            <li className="flex items-start gap-2">
              <Plus className="h-4 w-4 mt-0.5 shrink-0" />
              <span>Scroll down and tap "Add to Home Screen"</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-lg mt-[-4px]">✓</span>
              <span>Tap "Add" in the top right corner</span>
            </li>
          </ol>
        </div>
      )}

      {platform === 'android' && (
        <div className="space-y-3 text-sm">
          <p>To add this app to your home screen:</p>
          <ol className="space-y-2 ml-4">
            <li className="flex items-start gap-2">
              <MoreVertical className="h-4 w-4 mt-0.5 shrink-0" />
              <span>Tap the menu button (three dots) in your browser</span>
            </li>
            <li className="flex items-start gap-2">
              <Home className="h-4 w-4 mt-0.5 shrink-0" />
              <span>Tap "Add to Home Screen" or "Install App"</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-lg mt-[-4px]">✓</span>
              <span>Tap "Add" or "Install"</span>
            </li>
          </ol>
        </div>
      )}

      {platform === 'desktop' && (
        <div className="space-y-3 text-sm">
          <p>To install this app on your computer:</p>
          <ol className="space-y-2 ml-4">
            <li>Look for an install button in your browser's address bar</li>
            <li>Or use the browser menu to find "Install Yap.Town"</li>
            <li>The app will open in its own window</li>
          </ol>
        </div>
      )}

      <div className="flex gap-2 pt-2">
        <Button
          onClick={handleDismiss}
          variant="outline"
          size="sm"
        >
          Maybe later
        </Button>
      </div>
    </div>
  )
}