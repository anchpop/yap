-- Enable pg_cron extension if not already enabled
CREATE EXTENSION IF NOT EXISTS pg_cron;

-- Enable pg_net extension for HTTP requests
CREATE EXTENSION IF NOT EXISTS pg_net;

-- Grant usage on cron schema to postgres role
GRANT USAGE ON SCHEMA cron TO postgres;

-- Create a cron job to process scheduled notifications every 5 minutes
SELECT cron.schedule(
  'process-scheduled-notifications', -- Job name
  '*/5 * * * *', -- Every 5 minutes
  $$
  SELECT net.http_post(
    url := current_setting('app.settings.supabase_url') || '/functions/v1/process-scheduled-notifications',
    headers := jsonb_build_object(
      'Authorization', 'Bearer ' || current_setting('app.settings.supabase_service_role_key'),
      'Content-Type', 'application/json'
    ),
    body := '{}'::jsonb
  ) AS request_id;
  $$
);

-- Optional: Add a cleanup job to delete old sent notifications (runs daily at 2 AM)
SELECT cron.schedule(
  'cleanup-old-notifications',
  '0 2 * * *', -- Daily at 2 AM
  $$
  DELETE FROM public.scheduled_notifications 
  WHERE sent = true 
  AND sent_at < NOW() - INTERVAL '7 days';
  $$
);