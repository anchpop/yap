create or replace function sync_events(last_synced_ids jsonb)
returns jsonb as $$
declare
  result jsonb = '{}'::jsonb;
  device record;
  known_devices text[];
begin
  -- Get array of known device IDs
  select array_agg(key) into known_devices
  from jsonb_each_text(last_synced_ids);
  
  -- First, get updates for known devices
  for device in select * from jsonb_each_text(last_synced_ids)
  loop
    result := result || jsonb_build_object(
      device.key,
      (
        select coalesce(jsonb_agg(row_to_json(e)), '[]'::jsonb)
        from events e
        where e.device_id = device.key
        and e.id > device.value::bigint
        and e.user_id = auth.uid()
        order by e.id
      )
    );
  end loop;
  
  -- Then, get ALL events for unknown devices
  for device in 
    select distinct device_id 
    from events 
    where user_id = auth.uid()
    and device_id != all(known_devices) -- devices not in our list
  loop
    result := result || jsonb_build_object(
      device.device_id,
      (
        select coalesce(jsonb_agg(row_to_json(e)), '[]'::jsonb)
        from events e
        where e.device_id = device.device_id
        and e.user_id = auth.uid()
        order by e.id
      )
    );
  end loop;
  
  return result;
end;
$$ language plpgsql security definer
set search_path = public, auth, extensions, pg_catalog;
