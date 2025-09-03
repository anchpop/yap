import { Button } from "@/components/ui/button"
import { useOneSignalNotifications } from '@/hooks/use-onesignal-notifications'
import { Bell } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"
import { Switch } from "@/components/ui/switch"
import { Label } from "@/components/ui/label"
import { useState, useEffect } from 'react'
import { supabase } from '@/lib/supabase'

export function NotificationSettings() {
  const {
    isSupported,
    isSubscribed,
    isLoading,
    isInitialized,
    error,
    subscribe,
    sendTestNotification
  } = useOneSignalNotifications()

  const [notificationsEnabled, setNotificationsEnabled] = useState(true)
  const [loadingProfile, setLoadingProfile] = useState(true)
  const [savingProfile, setSavingProfile] = useState(false)

  // Load profile settings
  useEffect(() => {
    async function loadProfile() {
      try {
        const { data: { user } } = await supabase.auth.getUser()
        if (!user) return

        const { data: profile, error } = await supabase
          .from('profiles')
          .select('notifications_enabled')
          .eq('id', user.id)
          .single()

        if (error && error.code !== 'PGRST116') { // PGRST116 is "not found"
          console.error('Error loading profile:', error)
        } else if (profile) {
          setNotificationsEnabled(profile.notifications_enabled ?? true)
        }
      } catch (err) {
        console.error('Error loading profile:', err)
      } finally {
        setLoadingProfile(false)
      }
    }

    loadProfile()
  }, [])

  // Handle toggle change
  const handleToggleChange = async (checked: boolean) => {
    setSavingProfile(true)
    setNotificationsEnabled(checked)

    try {
      const { data: { user } } = await supabase.auth.getUser()
      if (!user) return

      const { error } = await supabase
        .from('profiles')
        .upsert({
          id: user.id,
          notifications_enabled: checked
        })

      if (error) {
        console.error('Error updating profile:', error)
        // Revert on error
        setNotificationsEnabled(!checked)
      }
    } catch (err) {
      console.error('Error updating profile:', err)
      // Revert on error
      setNotificationsEnabled(!checked)
    } finally {
      setSavingProfile(false)
    }
  }

  const renderContent = () => {
    if (!isInitialized || loadingProfile) {
      return (
        <div className="text-center">
          <p className="text-muted-foreground">Loading...</p>
        </div>
      )
    }

    return (
      <div className="space-y-6">
        {/* Global notification toggle */}
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="notifications-enabled">Notifications</Label>
            <p className="text-sm text-muted-foreground">
              Receive push notifications when cards are due
            </p>
          </div>
          <Switch
            id="notifications-enabled"
            checked={notificationsEnabled}
            onCheckedChange={handleToggleChange}
            disabled={savingProfile}
          />
        </div>

        {/* Push notification setup - only show if globally enabled */}
        {notificationsEnabled && (
          <div className="pt-4 border-t">
            {!isSupported ? (
              <div className="text-center">
                <p className="text-muted-foreground">
                  Push notifications are not supported in your browser.
                </p>
              </div>
            ) : !isSubscribed ? (
              <div className="text-center space-y-4">
                <Bell className="h-12 w-12 mx-auto text-muted-foreground" />
                <p className="text-muted-foreground">
                  Enable browser notifications to receive push alerts.
                </p>
                <Button onClick={subscribe} disabled={isLoading}>
                  {isLoading ? 'Enabling...' : 'Enable Browser Notifications'}
                </Button>
              </div>
            ) : (
              <div className="space-y-4">
                <div className="flex items-center gap-2 text-green-600 dark:text-green-400">
                  <Bell className="h-5 w-5" />
                  <span className="text-sm font-medium">Browser notifications enabled</span>
                </div>

                <Button
                  onClick={sendTestNotification}
                  disabled={isLoading}
                  variant="outline"
                  className="w-full"
                >
                  {isLoading ? 'Sending...' : 'Send Test Notification'}
                </Button>

                <p className="text-sm text-muted-foreground">
                  To disable browser notifications, use your browser's notification settings.
                </p>
              </div>
            )}
          </div>
        )}

        {error && (
          <p className="text-sm text-destructive">{error}</p>
        )}
      </div>
    )
  }

  return (
    <Dialog>
      <DialogTrigger asChild>
        <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
          <Bell className="mr-2 h-4 w-4" />
          Notifications
        </DropdownMenuItem>
      </DialogTrigger>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Push Notifications</DialogTitle>
          <DialogDescription>
            Get reminded when it's time to study your flashcards.
          </DialogDescription>
        </DialogHeader>
        <div className="py-4">
          {renderContent()}
        </div>
      </DialogContent>
    </Dialog>
  )
}
