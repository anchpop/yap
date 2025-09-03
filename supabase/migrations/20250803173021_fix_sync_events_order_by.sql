-- Fix sync_events function to handle ORDER BY correctly with aggregation

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
          select coalesce(jsonb_agg(row_to_json(e) order by e.id), '[]'::jsonb)
          from events e
          where e.device_id = device_record.key
          and e.stream_id = stream_record.key
          and e.id > device_record.value::bigint
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