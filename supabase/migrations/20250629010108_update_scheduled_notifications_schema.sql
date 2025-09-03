-- Add missing columns and rename scheduled_for to scheduled_at
ALTER TABLE public.scheduled_notifications 
  RENAME COLUMN scheduled_for TO scheduled_at;

-- Add sent column if it doesn't exist
ALTER TABLE public.scheduled_notifications 
  ADD COLUMN IF NOT EXISTS sent BOOLEAN DEFAULT false;

-- Add url column for notification deep links
ALTER TABLE public.scheduled_notifications 
  ADD COLUMN IF NOT EXISTS url TEXT;

-- Update the index to use the new column name
DROP INDEX IF EXISTS idx_scheduled_notifications_user_scheduled;
CREATE INDEX idx_scheduled_notifications_user_scheduled ON public.scheduled_notifications(user_id, scheduled_at);

-- Update the pending notifications index to use the sent column
DROP INDEX IF EXISTS idx_scheduled_notifications_pending;
CREATE INDEX idx_scheduled_notifications_pending ON public.scheduled_notifications(scheduled_at, sent) WHERE sent = false;

-- Add index for efficient cleanup of old sent notifications
CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_sent_at ON public.scheduled_notifications(sent_at) WHERE sent = true;