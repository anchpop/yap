-- Create events table
create table events (
  id bigserial primary key,
  user_id uuid references auth.users,
  device_id text not null,
  event jsonb not null,
  created_at timestamptz
);

create index idx_events_sync on events(device_id, id);

-- Enable RLS
alter table events enable row level security;
create policy "Users can see own events" on events
  for select using (auth.uid() = user_id);

-- Create sync function
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
$$ language plpgsql security definer;
