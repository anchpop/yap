import { serve } from "https://deno.land/std@0.168.0/http/server.ts"
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

const supabaseUrl = Deno.env.get('SUPABASE_URL')!
const supabaseServiceKey = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!

serve(async (req) => {
  // Handle CORS preflight requests
  if (req.method === 'OPTIONS') {
    return new Response(null, { headers: corsHeaders })
  }

  try {
    const { userId, title, body, url, test } = await req.json()

    if (!userId || !title || !body) {
      return new Response(
        JSON.stringify({ error: 'Missing required fields' }),
        { 
          status: 400,
          headers: { ...corsHeaders, 'Content-Type': 'application/json' }
        }
      )
    }

    // Check if user has notifications enabled (unless it's a test notification)
    if (!test) {
      const supabase = createClient(supabaseUrl, supabaseServiceKey)
      
      const { data: profile, error: profileError } = await supabase
        .from('profiles')
        .select('notifications_enabled')
        .eq('id', userId)
        .single()

      if (profileError && profileError.code !== 'PGRST116') {
        console.error('Error fetching profile:', profileError)
      }

      const notificationsEnabled = profile?.notifications_enabled ?? true
      
      if (!notificationsEnabled) {
        return new Response(
          JSON.stringify({ 
            success: true, 
            skipped: true, 
            reason: 'User has disabled notifications' 
          }),
          { 
            headers: { ...corsHeaders, 'Content-Type': 'application/json' }
          }
        )
      }
    }

    // Get OneSignal REST API key from environment
    const ONESIGNAL_REST_API_KEY = Deno.env.get('ONESIGNAL_REST_API_KEY')
    const ONESIGNAL_APP_ID = Deno.env.get('ONESIGNAL_APP_ID')

    if (!ONESIGNAL_REST_API_KEY || !ONESIGNAL_APP_ID) {
      console.error('OneSignal configuration missing')
      return new Response(
        JSON.stringify({ error: 'OneSignal not configured' }),
        { 
          status: 500,
          headers: { ...corsHeaders, 'Content-Type': 'application/json' }
        }
      )
    }

    // Send notification via OneSignal REST API
    const notificationData = {
      app_id: ONESIGNAL_APP_ID,
      headings: { en: title },
      contents: { en: body },
      include_external_user_ids: [userId],
      ...(url && { url }),
      ...(test && { 
        // For test notifications, send immediately
        send_after: new Date().toISOString()
      })
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

    if (!response.ok) {
      console.error('OneSignal API error:', result)
      return new Response(
        JSON.stringify({ error: 'Failed to send notification', details: result }),
        { 
          status: response.status,
          headers: { ...corsHeaders, 'Content-Type': 'application/json' }
        }
      )
    }

    return new Response(
      JSON.stringify({ success: true, notificationId: result.id }),
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