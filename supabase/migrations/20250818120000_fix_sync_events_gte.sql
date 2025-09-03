-- Ensure sync_events returns events with within_device_events_index >= provided value
-- This matches the client payload, which sends the current count (next needed index),
-- not the last-seen index.

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
            and e.user_id = auth.uid()
            -- Use >= because the client sends the next needed index (count), starting at 0
            and e.within_device_events_index >= device_record.value::integer
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

comment on function sync_events(jsonb) is 'Syncs events for multiple streams. Input: {"stream_id": {"last_synced_ids": {"device_id": next_needed_index}}}. Returns events with within_device_events_index >= provided value for requested devices, and ALL events for devices not specified but present in the stream.';

