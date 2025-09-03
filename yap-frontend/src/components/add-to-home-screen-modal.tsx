import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Home, Share, Plus, MoreVertical } from 'lucide-react'

interface AddToHomeScreenModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function AddToHomeScreenModal({ open, onOpenChange }: AddToHomeScreenModalProps) {
  const [platform, setPlatform] = useState<'ios' | 'android' | 'desktop' | 'unknown'>('unknown')

  useEffect(() => {
    const userAgent = navigator.userAgent.toLowerCase()
    const isSafari = /safari/.test(userAgent) && !/chrome/.test(userAgent)
    const isMacOS = /macintosh|mac os x/.test(userAgent)
    
    if (/iphone|ipad|ipod/.test(userAgent)) {
      setPlatform('ios')
    } else if (/android/.test(userAgent)) {
      setPlatform('android')
    } else if (isSafari && isMacOS) {
      // Desktop Safari doesn't support PWA installation
      setPlatform('ios') // Show iOS-like instructions for Safari desktop
    } else {
      setPlatform('desktop')
    }
  }, [])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Home className="h-5 w-5" />
            Add Yap.Town to Home Screen
          </DialogTitle>
          <DialogDescription>
            Install the app for quick access and a better experience
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {platform === 'ios' && (
            <div className="space-y-3 text-sm">
              <p className="text-muted-foreground">Safari is required for this feature.</p>
              <ol className="space-y-3 ml-4 list-decimal">
                <li className="flex items-start gap-3">
                  <MoreVertical className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground" />
                  <span>Tap the three dots in the bottom right</span>
                </li>
                <li className="flex items-start gap-3">
                  <Share className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground" />
                  <span>Tap "Share"</span>
                </li>
                <li className="flex items-start gap-3">
                  <span className="text-sm mt-0.5 shrink-0 text-muted-foreground">•••</span>
                  <span>Tap "More"</span>
                </li>
                <li className="flex items-start gap-3">
                  <Plus className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground" />
                  <span>Tap "Add to Home Screen"</span>
                </li>
              </ol>
            </div>
          )}

          {platform === 'android' && (
            <div className="space-y-3 text-sm">
              <p>To add this app to your home screen:</p>
              <ol className="space-y-3 ml-4">
                <li className="flex items-start gap-3">
                  <MoreVertical className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground" />
                  <span>Tap the menu button (three dots) in your browser</span>
                </li>
                <li className="flex items-start gap-3">
                  <Home className="h-5 w-5 mt-0.5 shrink-0 text-muted-foreground" />
                  <span>Tap "Add to Home Screen" or "Install App"</span>
                </li>
                <li className="flex items-start gap-3">
                  <span className="text-xl mt-[-4px] text-muted-foreground">✓</span>
                  <span>Tap "Add" or "Install"</span>
                </li>
              </ol>
            </div>
          )}

          {platform === 'desktop' && (
            <div className="space-y-3 text-sm">
              <p className="font-medium">For Chrome, Edge, or Brave:</p>
              <div className="space-y-2 ml-4">
                <p className="flex items-start gap-2">
                  <span className="text-muted-foreground">•</span>
                  Look for the install icon in your address bar (right side)
                </p>
                <p className="flex items-start gap-2">
                  <span className="text-muted-foreground">•</span>
                  Or click the three-dot menu → "Install Yap.Town"
                </p>
              </div>
              
              <img 
                src="/desktop-pwa-installation.png" 
                alt="Desktop PWA installation example" 
                className="rounded-lg border mt-3 mb-3"
              />
              
              <p className="font-medium mt-4">For Firefox:</p>
              <div className="ml-4">
                <p className="text-muted-foreground">Firefox doesn't support app installation. Consider using Chrome or Edge for the best experience.</p>
              </div>
            </div>
          )}

          <div className="flex justify-end gap-2 pt-4">
            <Button
              onClick={() => onOpenChange(false)}
              variant="outline"
            >
              Got it!
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
