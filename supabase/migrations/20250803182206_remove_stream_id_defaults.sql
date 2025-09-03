-- Remove default stream_id values from functions to ensure explicit stream specification

-- Drop the old functions that have defaults
drop function if exists sync_events(jsonb, text);
drop function if exists sync_events_single_stream(jsonb, text);

-- The main sync_events function doesn't need changes since it already requires explicit stream_id
-- in the request format: {"stream_id": {"last_synced_ids": {...}}}

-- If needed in the future, create a single-stream helper without defaults:
-- create or replace function sync_events_single_stream(last_synced_ids jsonb, p_stream_id text)
-- returns jsonb as $$
-- ...
-- (implementation without default)
-- ...
-- $$ language plpgsql security definer;

-- Add comment to clarify that stream_id must always be explicit
comment on function sync_events(jsonb) is 'Syncs events for multiple streams. Input format: {"stream_id": {"last_synced_ids": {"device_id": last_within_device_events_index}}}. Stream ID must be explicitly specified - there are no defaults.';