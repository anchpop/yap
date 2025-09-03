-- Migration: Add within_device_events_index to events table

-- Step 1: Add the column as nullable first
alter table events 
add column within_device_events_index integer;

-- Step 2: Populate existing rows with sequential numbers per device
with numbered_events as (
  select 
    id,
    row_number() over (
      partition by user_id, device_id 
      order by id
    ) as device_index
  from events
)
update events 
set within_device_events_index = numbered_events.device_index
from numbered_events
where events.id = numbered_events.id;

-- Step 3: Make the column required (NOT NULL)
alter table events 
alter column within_device_events_index set not null;

-- Step 4: Add a unique constraint to ensure no duplicates per user/device
alter table events 
add constraint events_unique_device_index 
unique (user_id, device_id, within_device_events_index);
