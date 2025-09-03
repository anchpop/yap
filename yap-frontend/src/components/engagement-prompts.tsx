import { useState, useEffect } from 'react'
import { Button } from "@/components/ui/button"
import { Bell, Home, Sparkles } from 'lucide-react'
import { useOneSignalNotifications } from '@/hooks/use-onesignal-notifications'
import { useIsInstalled } from '@/hooks/use-is-installed'
import { AddToHomeScreenModal } from '@/components/add-to-home-screen-modal'
import { AnimatedCard } from './AnimatedCard'

export function EngagementPrompts() {
  const {
    isSupported,
    isSubscribed,
    isLoading: isNotificationLoading,
    isInitialized,
    subscribe
  } = useOneSignalNotifications()

  const { isInstalled, isLoading: isInstalledLoading } = useIsInstalled()
  const [promptsDismissed, setPromptsDismissed] = useState(false)
  const [showHomeScreenInstructions, setShowHomeScreenInstructions] = useState(false)

  useEffect(() => {
    // Check dismissal count first
    const dismissalCount = parseInt(localStorage.getItem('engagement-prompts-dismissal-count') || '0', 10)

    // If dismissed 3 or more times, never show again
    if (dismissalCount >= 3) {
      setPromptsDismissed(true)
      return
    }

    // Check if user has previously dismissed the engagement prompts
    const dismissedTime = localStorage.getItem('engagement-prompts-dismissed')
    if (dismissedTime) {
      const dismissedTimestamp = parseInt(dismissedTime, 10)
      const now = Date.now()
      const oneDayInMs = 24 * 60 * 60 * 1000

      // If less than 24 hours have passed, keep it dismissed
      if (now - dismissedTimestamp < oneDayInMs) {
        setPromptsDismissed(true)
      } else {
        // More than 24 hours have passed, remove the dismissal
        localStorage.removeItem('engagement-prompts-dismissed')
      }
    }
  }, [])

  const handleAddToHomeScreenModalClose = (open: boolean) => {
    setShowHomeScreenInstructions(open)
  }

  const handleDismiss = () => {
    setPromptsDismissed(true)

    // Increment dismissal count
    const currentCount = parseInt(localStorage.getItem('engagement-prompts-dismissal-count') || '0', 10)
    const newCount = currentCount + 1
    localStorage.setItem('engagement-prompts-dismissal-count', newCount.toString())

    // Only set the timestamp if we haven't hit the permanent dismissal threshold
    if (newCount < 3) {
      localStorage.setItem('engagement-prompts-dismissed', Date.now().toString())
    }
  }

  // Calculate what to show
  const shouldShowAddToHomeScreen = !isInstalledLoading && !isInstalled
  const shouldShowNotifications = isInitialized && isSupported && !isSubscribed
  const shouldShowAnything = (shouldShowAddToHomeScreen || shouldShowNotifications) && !promptsDismissed

  if (!shouldShowAnything) {
    return null
  }

  return (
    <AnimatedCard className="bg-card text-card-foreground rounded-lg p-6 border">
      <div className="flex items-center gap-2 mb-4">
        <Sparkles className="h-5 w-5 text-primary" />
        <h3 className="font-semibold">Stay on track with your French learning</h3>
      </div>

      <p className="text-sm text-muted-foreground mb-4">
        Research shows that consistent daily practice is key to language learning success.
        These features help you maintain your streak:
      </p>

      <div className="grid grid-cols-[auto_1fr] gap-3">
        {shouldShowAddToHomeScreen && (
          <>
            <Button
              onClick={() => setShowHomeScreenInstructions(true)}
              variant="outline"
              size="sm"
              className="justify-start"
            >
              <Home className="mr-2 h-4 w-4" />
              {window.navigator.userAgent.match(/mobile/i) ? 'Add to Home Screen' : 'Install App'}
            </Button>
            <p className="text-xs text-muted-foreground self-center">
              {window.navigator.userAgent.match(/mobile/i)
                ? 'Quick access from your home screen makes it easier to practice daily'
                : 'Install as a desktop app for quick access and offline use'}
            </p>
          </>
        )}

        {shouldShowNotifications && (
          <>
            <Button
              onClick={subscribe}
              disabled={isNotificationLoading}
              variant="outline"
              size="sm"
              className="justify-start"
            >
              <Bell className="mr-2 h-4 w-4" />
              {isNotificationLoading ? 'Enabling...' : 'Enable Reminders'}
            </Button>
            <p className="text-xs text-muted-foreground self-center">
              Get gentle reminders when you have cards ready to review
            </p>
          </>
        )}
      </div>

      <div className="flex justify-end mt-4">
        <Button
          onClick={handleDismiss}
          variant="ghost"
          size="sm"
          className="text-xs"
        >
          Maybe later
        </Button>
      </div>

      <AddToHomeScreenModal
        open={showHomeScreenInstructions}
        onOpenChange={handleAddToHomeScreenModalClose}
      />
    </AnimatedCard>
  )
}
