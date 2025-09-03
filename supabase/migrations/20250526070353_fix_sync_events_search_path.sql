create or replace function sync_events(last_synced_ids jsonb)
returns jsonb as $$
declare
  result jsonb = '{}'::jsonb;
  device record;
begin
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
  return result;
end;
$$ language plpgsql security definer
set search_path = public, auth, extensions, pg_catalog;
