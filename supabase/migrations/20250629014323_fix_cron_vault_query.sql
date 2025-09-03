-- Delete the problematic cron job
SELECT cron.unschedule('process-scheduled-notifications-v2');

-- Create a function to process notifications
CREATE OR REPLACE FUNCTION process_scheduled_notifications()
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    project_url text;
    service_key text;
    request_id bigint;
BEGIN
    -- Get the secrets from vault
    SELECT decrypted_secret INTO project_url
    FROM vault.decrypted_secrets
    WHERE name = 'project_url';
    
    SELECT decrypted_secret INTO service_key
    FROM vault.decrypted_secrets
    WHERE name = 'service_role_key';
    
    -- Check if secrets exist
    IF project_url IS NULL OR service_key IS NULL THEN
        RAISE WARNING 'Vault secrets not found. Please add project_url and service_role_key to vault.';
        RETURN;
    END IF;
    
    -- Make the HTTP request
    SELECT net.http_post(
        url := project_url || '/functions/v1/process-scheduled-notifications',
        headers := jsonb_build_object(
            'Content-Type', 'application/json',
            'Authorization', 'Bearer ' || service_key
        ),
        body := '{}'::jsonb
    ) INTO request_id;
    
    -- Log success (visible in cron job logs)
    RAISE NOTICE 'Scheduled notification processing request ID: %', request_id;
EXCEPTION
    WHEN OTHERS THEN
        RAISE WARNING 'Failed to process scheduled notifications: %', SQLERRM;
END;
$$;

-- Create the cron job to call our function
SELECT cron.schedule(
  'process-scheduled-notifications-v3',
  '*/5 * * * *', -- Every 5 minutes
  'SELECT process_scheduled_notifications();'
);