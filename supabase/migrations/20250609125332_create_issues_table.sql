-- Create issues table for user reports
create table issues (
  id bigserial primary key,
  user_id uuid references auth.users,
  issue_text text not null,
  created_at timestamptz default now(),
  updated_at timestamptz default now()
);

-- Enable RLS
alter table issues enable row level security;

-- Users can only insert their own issues
create policy "Users can insert own issues" on issues
  for insert with check (auth.uid() = user_id);

-- Users can see their own issues (optional - for future viewing)
create policy "Users can see own issues" on issues
  for select using (auth.uid() = user_id);

-- Admin can see all issues (you'll need to set up admin roles separately)
-- create policy "Admins can see all issues" on issues
--   for select using (auth.jwt() ->> 'role' = 'admin');