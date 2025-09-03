import { useState, useEffect } from 'react'
import { supabase } from '@/lib/supabase'

interface PushSubscriptionState {
  id?: string
  token?: string
  optedIn?: boolean
}

interface PushSubscriptionChangeEvent {
  previous: PushSubscriptionState
  current: PushSubscriptionState
}

declare global {
  interface Window {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    OneSignalDeferred?: Array<(OneSignal: any) => void>
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    OneSignal?: any
  }
}

export function useOneSignalNotifications() {
  const [isSupported, setIsSupported] = useState(true)
  const [isSubscribed, setIsSubscribed] = useState(false)
  const [isLoading, setIsLoading] = useState(false)
  const [isInitialized, setIsInitialized] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    initializeOneSignal()
    
    // Set up listener for subscription changes
    const handleSubscriptionChange = (event: PushSubscriptionChangeEvent) => {
      console.log("Push subscription changed", event)
      if (event.current && event.current.optedIn !== undefined) {
        setIsSubscribed(event.current.optedIn === true)
      }
    }
    
    window.OneSignalDeferred = window.OneSignalDeferred || []
    window.OneSignalDeferred.push(function(OneSignal) {
      OneSignal.User.PushSubscription.addEventListener("change", handleSubscriptionChange)
    })
    
    return () => {
      window.OneSignalDeferred = window.OneSignalDeferred || []
      window.OneSignalDeferred.push(function(OneSignal) {
        OneSignal.User.PushSubscription.removeEventListener("change", handleSubscriptionChange)
      })
    }
  }, [])

  const initializeOneSignal = async () => {
    try {
      // Wait for OneSignal to be available
      await new Promise<void>((resolve) => {
        if (window.OneSignal) {
          resolve()
        } else {
          window.OneSignalDeferred = window.OneSignalDeferred || []
          window.OneSignalDeferred.push(() => resolve())
        }
      })
      console.log("OneSignal is available")

      // Wait a bit more to ensure OneSignal is fully initialized
      await new Promise(resolve => setTimeout(resolve, 100))

      // Check if notifications are supported
      console.log("Checking permission status", window.OneSignal.Notifications)
      const supported = await window.OneSignal.Notifications.isPushSupported()
      console.log("Notifications are supported: ", supported)
      setIsSupported(supported)

      if (supported) {
        // Check current permission status
        const permission = window.OneSignal.Notifications.permission
        console.log("Permission: ", permission)
        
        // Also check if user is actually opted in to push notifications
        let optedIn = false
        try {
          optedIn = window.OneSignal.User.PushSubscription.optedIn
          console.log("OptedIn status: ", optedIn)
        } catch (e) {
          console.log("Could not check optedIn status: ", e)
        }
        
        // Consider subscribed if we have permission AND user is opted in
        setIsSubscribed(permission === true && optedIn === true)
        
        // Only try to login if we have notification permissions
        if (permission === true) {
          const { data: { user } } = await supabase.auth.getUser()
          if (user) {
            try {
              await window.OneSignal.login(user.id)
            } catch (loginError) {
              console.warn('OneSignal login failed, will retry on subscribe:', loginError)
            }
          }
        }
      } else {
        console.log("Notifications are not supported")
      }

      setIsInitialized(true)
    } catch (err) {
      console.error('Error initializing OneSignal:', err)
      setError(err instanceof Error ? err.message : 'Failed to initialize')
      setIsInitialized(true)
    }
  }

  const subscribe = async () => {
    try {
      setIsLoading(true)
      setError(null)

      const { data: { user } } = await supabase.auth.getUser()
      if (!user) {
        throw new Error('Must be logged in to subscribe')
      }

      const accepted = await window.OneSignal.Notifications.requestPermission()
      console.log('accepted', accepted)

      // Only set subscribed if permission was actually granted
      if (accepted) {
        setIsSubscribed(true)
      }
      
      // Wait a moment for OneSignal to process the permission
      await new Promise(resolve => setTimeout(resolve, 500))
      
      // Ensure user is logged in with OneSignal
      try {
        await window.OneSignal.login(user.id)
      } catch (loginError) {
        console.error('Failed to login to OneSignal:', loginError)
        // Continue anyway - the subscription still works
      }
      
      // Add tags for targeting
      await window.OneSignal.User.addTags({
        app: 'yap-town',
        user_id: user.id
      })
    } catch (err) {
      console.error('Error subscribing:', err)
      setError(err instanceof Error ? err.message : 'Failed to subscribe')
    } finally {
      setIsLoading(false)
    }
  }

  const unsubscribe = async () => {
    try {
      setIsLoading(true)
      setError(null)

      // OneSignal doesn't have a direct unsubscribe method
      // Users need to manage this through browser settings
      setError('To unsubscribe, please use your browser notification settings')
    } catch (err) {
      console.error('Error unsubscribing:', err)
      setError(err instanceof Error ? err.message : 'Failed to unsubscribe')
    } finally {
      setIsLoading(false)
    }
  }

  const sendTestNotification = async () => {
    try {
      setIsLoading(true)
      setError(null)

      const { data: { user } } = await supabase.auth.getUser()
      if (!user) throw new Error('Not authenticated')

      // Send test notification via edge function
      const response = await supabase.functions.invoke('send-onesignal-notification', {
        body: {
          userId: user.id,
          title: 'Test Notification',
          body: 'This is a test notification from Yap.Town!',
          test: true
        }
      })

      if (response.error) {
        throw response.error
      }
    } catch (err) {
      console.error('Error sending test notification:', err)
      setError(err instanceof Error ? err.message : 'Failed to send test notification')
    } finally {
      setIsLoading(false)
    }
  }

  return {
    isSupported,
    isSubscribed,
    isLoading,
    isInitialized,
    error,
    subscribe,
    unsubscribe,
    sendTestNotification
  }
}
