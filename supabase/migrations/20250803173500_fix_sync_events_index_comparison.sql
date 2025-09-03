-- Fix sync_events to compare within_device_events_index instead of id

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
          select coalesce(jsonb_agg(row_to_json(e) order by e.within_device_events_index), '[]'::jsonb)
          from events e
          where e.device_id = device_record.key
          and e.stream_id = stream_record.key
          and e.within_device_events_index > device_record.value::integer
          and e.user_id = auth.uid()
        )
      );
    end loop;
    
    -- Add this stream's results to the main result
    result := result || jsonb_build_object(stream_record.key, stream_result);
  end loop;
  
  return result;
end;
$$ language plpgsql security definer;

-- Update comment to clarify the expected format
comment on function sync_events(jsonb) is 'Syncs events for multiple streams. Input format: {"stream_id": {"last_synced_ids": {"device_id": last_within_device_events_index}}}. Output format: {"stream_id": {"device_id": [events]}}. The last_synced_ids values should be the within_device_events_index of the last synced event for each device.';