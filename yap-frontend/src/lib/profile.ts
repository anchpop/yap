import { supabase } from './supabase'

export async function getUserNotificationPreference(userId: string): Promise<boolean> {
  try {
    const { data: profile, error } = await supabase
      .from('profiles')
      .select('notifications_enabled')
      .eq('id', userId)
      .single()

    if (error && error.code !== 'PGRST116') {
      console.error('Error fetching profile:', error)
      return true // Default to enabled on error
    }

    return profile?.notifications_enabled ?? true
  } catch (err) {
    console.error('Error fetching notification preference:', err)
    return true // Default to enabled on error
  }
}