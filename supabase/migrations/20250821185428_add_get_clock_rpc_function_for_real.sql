-- Creates an RPC to fetch per-stream, per-device event counts for a user
-- Returns JSONB shaped like: {"<stream_id>": {"<device_id>": <event_count>}}

-- Create or replace ensures idempotent deployments
create or replace function public.get_clock(p_user_id uuid)
returns jsonb
language sql
stable
set search_path = public
as $$
  with counts as (
    select
      stream_id,
      device_id,
      count(*)::int as event_count
    from public.events
    where user_id = p_user_id
    group by stream_id, device_id
  ),
  device_map as (
    select
      stream_id,
      jsonb_object_agg(device_id::text, to_jsonb(event_count)) as devices
    from counts
    group by stream_id
  )
  select coalesce(
    jsonb_object_agg(stream_id::text, devices),
    '{}'::jsonb
  )
  from device_map;
$$;

-- Grant execution to typical Supabase roles as needed
grant execute on function public.get_clock(uuid) to authenticated, service_role;
