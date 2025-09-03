-- Migration: Add stream_id column to events table for multi-stream support

-- Step 1: Add stream_id column (nullable initially)
alter table events 
add column stream_id text;

-- Step 2: Initialize existing rows with 'reviews' stream_id
update events 
set stream_id = 'reviews'
where stream_id is null;

-- Step 3: Make stream_id required
alter table events 
alter column stream_id set not null;

-- Step 4: Drop the existing unique constraint
alter table events 
drop constraint if exists events_unique_device_index;

-- Step 5: Create new unique constraint including stream_id
alter table events 
add constraint events_unique_stream_device_index 
unique (user_id, stream_id, device_id, within_device_events_index);

-- Step 6: Create index for efficient stream-based queries
create index idx_events_stream_sync on events(stream_id, device_id, id);

-- Step 7: Update the sync_events function to accept stream_id parameter
create or replace function sync_events(last_synced_ids jsonb, p_stream_id text default 'reviews')
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
        and e.stream_id = p_stream_id
        and e.id > device.value::bigint
        and e.user_id = auth.uid()
        order by e.id
      )
    );
  end loop;
  return result;
end;
$$ language plpgsql security definer;

-- Step 8: Create an overloaded version for backward compatibility
create or replace function sync_events(last_synced_ids jsonb)
returns jsonb as $$
begin
  return sync_events(last_synced_ids, 'reviews');
end;
$$ language plpgsql security definer;

-- Add comment to document the column
comment on column events.stream_id is 'Identifies which data stream this event belongs to (e.g., reviews, courses)';