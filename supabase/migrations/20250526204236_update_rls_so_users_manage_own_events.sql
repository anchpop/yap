-- Drop the existing select-only policy
drop policy "Users can see own events" on events;

-- Create one policy for all operations
create policy "Users can manage own events" on events
  for all using (auth.uid() = user_id) with check (auth.uid() = user_id);
