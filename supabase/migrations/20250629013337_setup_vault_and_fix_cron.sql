-- First, ensure vault extension is enabled
CREATE EXTENSION IF NOT EXISTS supabase_vault;

-- Note: You'll need to manually insert the secrets after running this migration
-- Run these commands in the Supabase SQL editor after the migration:
-- 
-- INSERT INTO vault.secrets (name, secret, description)
-- VALUES 
--   ('project_url', 'https://YOUR_PROJECT_ID.supabase.co', 'Supabase project URL'),
--   ('service_role_key', 'YOUR_SERVICE_ROLE_KEY', 'Supabase service role key')
-- ON CONFLICT (name) DO UPDATE 
-- SET secret = EXCLUDED.secret;

-- Delete the old cron job
SELECT cron.unschedule('process-scheduled-notifications');

-- Create the new cron job using vault secrets
SELECT cron.schedule(
  'process-scheduled-notifications-v2',
  '*/5 * * * *', -- Every 5 minutes
  $$
  SELECT
    net.http_post(
      url := (SELECT decrypted_secret FROM vault.decrypted_secrets WHERE name = 'project_url') || '/functions/v1/process-scheduled-notifications',
      headers := jsonb_build_object(
        'Content-Type', 'application/json',
        'Authorization', 'Bearer ' || (SELECT decrypted_secret FROM vault.decrypted_secrets WHERE name = 'service_role_key')
      ),
      body := '{}'::jsonb
    ) AS request_id;
  $$
);

-- Also update the cleanup job to use a more standard approach
SELECT cron.unschedule('cleanup-old-notifications');

SELECT cron.schedule(
  'cleanup-old-notifications-v2',
  '0 2 * * *', -- Daily at 2 AM
  $$
  DELETE FROM public.scheduled_notifications 
  WHERE sent = true 
  AND sent_at < NOW() - INTERVAL '7 days';
  $$
);