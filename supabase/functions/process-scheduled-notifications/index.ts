import { serve } from "https://deno.land/std@0.168.0/http/server.ts"
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

serve(async (req) => {
  // Handle CORS preflight requests
  if (req.method === 'OPTIONS') {
    return new Response(null, { headers: corsHeaders })
  }

  try {
    // Get the authorization header from the request
    const authHeader = req.headers.get('Authorization')
    if (!authHeader) {
      throw new Error('Missing Authorization header')
    }

    // Create Supabase client with the service role key from the request
    const supabaseUrl = Deno.env.get('SUPABASE_URL')!
    const supabase = createClient(
      supabaseUrl,
      authHeader.replace('Bearer ', ''),
      {
        auth: {
          autoRefreshToken: false,
          persistSession: false
        }
      }
    )
    // Get current time
    const now = new Date()
    
    // Find all notifications that are due and haven't been sent
    const { data: dueNotifications, error: fetchError } = await supabase
      .from('scheduled_notifications')
      .select('*')
      .lte('scheduled_at', now.toISOString())
      .eq('sent', false)
      .limit(100) // Process up to 100 notifications at a time

    if (fetchError) {
      console.error('Error fetching due notifications:', fetchError)
      throw fetchError
    }

    if (!dueNotifications || dueNotifications.length === 0) {
      return new Response(
        JSON.stringify({ 
          success: true, 
          message: 'No notifications to process',
          processed: 0 
        }),
        { 
          headers: { ...corsHeaders, 'Content-Type': 'application/json' }
        }
      )
    }

    // Get OneSignal configuration
    const ONESIGNAL_REST_API_KEY = Deno.env.get('ONESIGNAL_REST_API_KEY')
    const ONESIGNAL_APP_ID = Deno.env.get('ONESIGNAL_APP_ID')

    if (!ONESIGNAL_REST_API_KEY || !ONESIGNAL_APP_ID) {
      console.error('OneSignal configuration missing')
      throw new Error('OneSignal not configured')
    }

    // Process each notification
    const results = await Promise.allSettled(
      dueNotifications.map(async (notification) => {
        try {
          // Check if user has notifications enabled
          const { data: profile } = await supabase
            .from('profiles')
            .select('notifications_enabled')
            .eq('id', notification.user_id)
            .single()

          if (!profile?.notifications_enabled) {
            // Mark as sent but skip sending
            await supabase
              .from('scheduled_notifications')
              .update({ sent: true })
              .eq('id', notification.id)
            
            return { skipped: true, userId: notification.user_id, reason: 'disabled' }
          }

          // Send notification via OneSignal
          const notificationData = {
            app_id: ONESIGNAL_APP_ID,
            headings: { en: notification.title },
            contents: { en: notification.body },
            include_external_user_ids: [notification.user_id],
            url: notification.url || 'https://yap.town'
          }

          const response = await fetch('https://onesignal.com/api/v1/notifications', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              'Authorization': `Basic ${ONESIGNAL_REST_API_KEY}`
            },
            body: JSON.stringify(notificationData)
          })

          const result = await response.json()

          if (response.ok) {
            // Mark notification as sent
            await supabase
              .from('scheduled_notifications')
              .update({ 
                sent: true,
                sent_at: new Date().toISOString()
              })
              .eq('id', notification.id)

            return { 
              success: true, 
              userId: notification.user_id, 
              notificationId: result.id 
            }
          } else {
            console.error('OneSignal API error:', result)
            return { 
              success: false, 
              userId: notification.user_id, 
              error: result 
            }
          }
        } catch (error) {
          console.error('Error processing notification:', error)
          return { 
            success: false, 
            userId: notification.user_id, 
            error: error.message 
          }
        }
      })
    )

    // Count successes and failures
    const processed = results.filter(r => r.status === 'fulfilled' && r.value.success).length
    const skipped = results.filter(r => r.status === 'fulfilled' && r.value.skipped).length
    const failed = results.filter(r => r.status === 'rejected' || (r.status === 'fulfilled' && !r.value.success && !r.value.skipped)).length

    return new Response(
      JSON.stringify({ 
        success: true,
        message: `Processed ${dueNotifications.length} notifications`,
        processed,
        skipped,
        failed,
        details: results.map(r => r.status === 'fulfilled' ? r.value : { error: r.reason })
      }),
      { 
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      }
    )

  } catch (error) {
    console.error('Error:', error)
    return new Response(
      JSON.stringify({ error: error.message }),
      { 
        status: 500,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      }
    )
  }
})