---
name: supabase-docs
description: Search and read Supabase documentation using a bash shell. Use when working on a Supabase feature or troubleshooting a problem.
---

# Supabase Docs

Search and read Supabase documentation over SSH.

## How to use

```bash
# Search for a topic
ssh supabase.sh grep -rl 'auth' /supabase/docs/

# Read a specific guide
ssh supabase.sh cat /supabase/docs/guides/auth/passwords.md

# Find all guides in a section
ssh supabase.sh find /supabase/docs/guides/database -name '*.md'

# Search with context
ssh supabase.sh grep -r 'RLS' /supabase/docs/guides/auth --include='*.md' -l
```

All docs live under `/supabase/docs/` as markdown files. You can use any standard Unix tools (grep, find, cat, etc.) to search and read them.
