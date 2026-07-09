# yap-mcp

An MCP (Model Context Protocol) server that exposes a yap.town account to chat
clients: do reviews, add words, look things up in the dictionary, pull
comprehensible example sentences, and check stats — all backed by the same
event stream the web app syncs, so everything shows up across devices.

One binary, two modes:

- `yap-mcp` (or `yap-mcp stdio`) — stdio server for local clients (Claude
  Code/Desktop), bound to one account via env vars and the service role key.
- `yap-mcp serve` — remote server (streamable HTTP + OAuth 2.1) usable as a
  claude.ai custom connector; multi-user, each user signs in with their own
  yap credentials.

## Tools

Words are identified by `(language, gram)`, where a gram is the token sequence
— word + lemma + part of speech — that uniquely discriminates a dictionary
entry (it's exactly what deck events store). Tools that take grams or cards
validate them by interning against the language pack **and** checking
membership in the master frequency list — the rodeos intern more than real
entries, so the dictionary is the real check. Anything that doesn't name a
real course entry is rejected; the server never guesses.

- `search_dictionary` — find words/phrases; returns each match's `language` +
  `gram` (plus display text, frequency rank, definition).
- `add_cards` — add words to the deck as flashcards, by `(language, gram)`.
- `get_due_cards` — list due cards; each entry carries its `language` + `card`
  object to pass back to `log_review`.
- `log_review` — record a review result (`again`/`hard`/`good`/`easy`/`remembered`).
  Appends a real `ReviewCard` event, so FSRS scheduling updates for real.
- `get_sentences` — example sentences containing a gram, with translations and
  source attribution (Anki/Tatoeba/manual/songs/movies): a
  `comprehensible_sentences` list otherwise composed only of words the user
  knows, and an `other_sentences` list sampled from everything containing the
  gram regardless of difficulty.
- `get_stats` — streak, XP, review counts, deck size, tier, recent days.

## How it works

Events are fetched from Supabase at startup (and re-fetched lazily every ~20s)
and replayed natively through `yap-frontend-rs` — the same deck logic the web
app runs in WASM. Writes append events under the device id `yap-mcp` and POST
them to `/rest/v1/events` in the exact shape the web app uses; other devices
pick them up on their next sync.

## Setup

Requires the language pack `.rkyv` files in `out/` (committed via LFS) and the
repo root `.env` containing `SUPABASE_SERVICE_ROLE_KEY`.

Environment:

- `YAP_USER_EMAIL` (required) — the account to operate on
- `SUPABASE_SERVICE_ROLE_KEY` (required) — read from the repo `.env` automatically
- `SUPABASE_URL` (optional) — defaults to production
- `YAP_OUT_DIR` (optional) — defaults to `<repo>/out`

Build and register with a client, e.g. Claude Code:

```bash
cargo build --release -p yap-mcp
claude mcp add yap --env YAP_USER_EMAIL=you@example.com -- /path/to/yap/target/release/yap-mcp
```

Startup takes a few seconds (event fetch + ~130 MB language pack load); the
pack stays resident for the life of the process.

## Remote server (claude.ai custom connector)

`yap-mcp serve` serves the same tools over streamable HTTP at `/mcp`, wrapped
in an OAuth 2.1 flow that claude.ai's "add custom connector" understands:
RFC 9728 protected-resource metadata → RFC 8414 authorization-server metadata
→ RFC 7591 dynamic client registration → PKCE (S256) authorization-code flow
→ token + refresh.

The OAuth layer is a thin shim over Supabase auth:

- The `/oauth/authorize` page is a yap login form (email + password). A
  successful login becomes a single-use authorization code.
- Tokens are MCP-scoped: we mint our own JWTs (`aud: yap-mcp`), with the
  user's Supabase session embedded **encrypted** (ChaCha20-Poly1305, key
  derived from `SUPABASE_JWT_SECRET`). The OAuth client never holds a raw
  Supabase credential — a leaked token is only usable against `/mcp` here,
  and raw Supabase JWTs are rejected. Refresh decrypts the embedded Supabase
  refresh token and proxies to Supabase. Still no token store.
- Client registrations are encoded as signed `client_id`s (the redirect URIs
  live inside the signature), so server restarts don't break connections. The
  only in-memory OAuth state is in-flight authorization codes — which is why
  the deployment pins a single always-on machine.
- All Supabase reads/writes happen as the user themself (anon apikey + user
  JWT), inside row-level security. The remote server never touches the
  service role key.

Per-user deck state is initialized lazily on first tool call (event fetch +
replay) and cached; language packs load lazily per course and are shared
across users.

Env: `SUPABASE_JWT_SECRET` (required), `YAP_MCP_BASE_URL` (public URL, used in
metadata and Host validation), `PORT`, `SUPABASE_URL`, `SUPABASE_ANON_KEY`,
`YAP_OUT_DIR`.

Deploy on Fly (first time: `flyctl launch --no-deploy --copy-config --config
yap-mcp/fly.toml`, then set the secret):

```bash
flyctl secrets set --config yap-mcp/fly.toml SUPABASE_JWT_SECRET=...
flyctl deploy --config yap-mcp/fly.toml
```

Then in claude.ai: Settings → Connectors → Add custom connector → URL
`https://yap-mcp.fly.dev/mcp`. Claude discovers the OAuth endpoints, opens the
yap login page, and connects.

## Current limitations

- stdio mode's auth is service-role + email (fine for personal/local use);
  serve mode is the multi-user path.
- Login is email + password only (matches the yap frontend's auth surface).
- Listening cards can be quizzed only as text in a chat client.
- No challenge generation (translation/transcription exercises) yet; the chat
  LLM is expected to quiz the user itself, e.g. using `get_sentences`.
- OAuth codes and MCP sessions are in-memory: run exactly one instance.
