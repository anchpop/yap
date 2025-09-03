create or replace function sync_events(last_synced_ids jsonb)
returns jsonb as $$
declare
  result      jsonb := '{}'::jsonb;
  device      record;
  known_devices text[];
begin
  -- Treat no keys as an empty array, not NULL
  select coalesce(array_agg(key), ARRAY[]::text[])
    into known_devices
  from jsonb_each_text(last_synced_ids);

  -- First, updates for known devices
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

  -- Then, all events for any devices not in our list
  for device in
    select distinct device_id
      from events
     where user_id = auth.uid()
       and device_id <> all(known_devices)
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
