create or replace function sync_events(last_synced_ids jsonb)
returns jsonb as $$
declare
  result jsonb = '{}'::jsonb;
  device record;
  known_devices text[];
begin
  -- Get array of known device IDs (will be NULL if empty)
  select array_agg(key) into known_devices
  from jsonb_each_text(last_synced_ids);
  
  -- First, get updates for known devices
  for device in select * from jsonb_each_text(last_synced_ids)
  loop
    result := result || jsonb_build_object(
      device.key,
      (
        select coalesce(array_to_json(array_agg(t.*)), '[]')::jsonb
        from (
          select * from events
          where device_id = device.key
          and id > device.value::bigint
          and user_id = auth.uid()
          order by id
        ) t
      )
    );
  end loop;
  
  -- Then, get ALL events for unknown devices
  for device in 
    select distinct device_id 
    from events 
    where user_id = auth.uid()
    and (
      known_devices is null
      or device_id != all(known_devices)
    )
  loop
    result := result || jsonb_build_object(
      device.device_id,
      (
        select coalesce(array_to_json(array_agg(t.*)), '[]')::jsonb
        from (
          select * from events
          where device_id = device.device_id
          and user_id = auth.uid()
          order by id
        ) t
      )
    );
  end loop;
  
  return result;
end;
$$ language plpgsql security definer
set search_path = public, auth, extensions, pg_catalog;
