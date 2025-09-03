-- Migration: Update sync_events to handle multiple streams in one call

-- Drop the previous versions of sync_events
drop function if exists sync_events(jsonb, text);
drop function if exists sync_events(jsonb);

-- Create new sync_events function that handles stream-based structure
create or replace function sync_events(sync_request jsonb)
returns jsonb as $$
declare
  result jsonb = '{}'::jsonb;
  stream_record record;
  device_record record;
  stream_result jsonb;
begin
  -- Loop through each stream in the request
  for stream_record in select * from jsonb_each(sync_request)
  loop
    stream_result := '{}'::jsonb;
    
    -- For each stream, loop through each device's last_synced_id
    for device_record in select * from jsonb_each_text(stream_record.value->'last_synced_ids')
    loop
      stream_result := stream_result || jsonb_build_object(
        device_record.key,
        (
          select coalesce(jsonb_agg(row_to_json(e)), '[]'::jsonb)
          from events e
          where e.device_id = device_record.key
          and e.stream_id = stream_record.key
          and e.id > device_record.value::bigint
          and e.user_id = auth.uid()
          order by e.id
        )
      );
    end loop;
    
    -- Add this stream's results to the main result
    result := result || jsonb_build_object(stream_record.key, stream_result);
  end loop;
  
  return result;
end;
$$ language plpgsql security definer;

-- Add comment to document the new function signature
comment on function sync_events(jsonb) is 'Syncs events for multiple streams. Input format: {"stream_id": {"last_synced_ids": {"device_id": last_id}}}. Output format: {"stream_id": {"device_id": [events]}}';

-- Create a compatibility wrapper for single-stream calls (for gradual migration)
create or replace function sync_events_single_stream(last_synced_ids jsonb, p_stream_id text default 'reviews')
returns jsonb as $$
declare
  request jsonb;
  response jsonb;
begin
  -- Convert single-stream format to multi-stream format
  request := jsonb_build_object(p_stream_id, jsonb_build_object('last_synced_ids', last_synced_ids));
  
  -- Call the new multi-stream function
  response := sync_events(request);
  
  -- Extract just this stream's results
  return response->p_stream_id;
end;
$$ language plpgsql security definer;
