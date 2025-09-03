-- Fix sync_events to return all events for devices not specified in the request

create or replace function sync_events(sync_request jsonb)
returns jsonb as $$
declare
  result jsonb = '{}'::jsonb;
  stream_record record;
  device_record record;
  stream_result jsonb;
  requested_devices jsonb;
begin
  -- Loop through each stream in the request
  for stream_record in select * from jsonb_each(sync_request)
  loop
    stream_result := '{}'::jsonb;
    requested_devices := stream_record.value->'last_synced_ids';
    
    -- First, get events for explicitly requested devices
    for device_record in select * from jsonb_each_text(requested_devices)
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
    
    -- Then, get ALL events for devices not in the request but present in this stream
    for device_record in 
      select distinct device_id 
      from events 
      where stream_id = stream_record.key 
      and user_id = auth.uid()
      and device_id not in (select jsonb_object_keys(requested_devices))
    loop
      stream_result := stream_result || jsonb_build_object(
        device_record.device_id,
        (
          select coalesce(jsonb_agg(row_to_json(e) order by e.within_device_events_index), '[]'::jsonb)
          from events e
          where e.device_id = device_record.device_id
          and e.stream_id = stream_record.key
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

-- Update comment to document this behavior
comment on function sync_events(jsonb) is 'Syncs events for multiple streams. Input format: {"stream_id": {"last_synced_ids": {"device_id": last_within_device_events_index}}}. For each stream: returns new events (index > provided) for specified devices, and ALL events for devices not specified in the request but present in the database for that stream.';