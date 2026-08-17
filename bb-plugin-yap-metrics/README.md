# Yap Metrics

A local [bb](https://getbb.app) panel for two rolling Yap product metrics:

- weekly active users: unique users with a synced event in the trailing 7 days;
- sign-ups in the past month: Supabase Auth users created in the trailing 30 days.

The plugin never reads or stores the Supabase service-role key. It runs the
repository's existing `user-metrics` binary as a local child process; that
binary reads the root `.env`, talks directly to Supabase, and returns only the
two aggregate counts and their window timestamps as JSON. The bb frontend and
getbb.app never receive the key.

```bash
npm install --include=dev
npm test
npm run build
bb plugin install .
```
