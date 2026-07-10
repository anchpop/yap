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
- `unlock_cards` — release the next batch from lockup (the app's "review N
   more cards" offer), via the deck's own `get_release_offer`; reviewing a
   locked card also unlocks it. `get_due_cards` reports `cards_in_lockup`.
- `get_stats` — streak, XP, review counts, deck size, tier, recent days.
- `present_card` — renders one card as an interactive MCP Apps widget
   (sentence with audio, reveal, grade buttons); the widget logs the user's
   own grade via `log_review` and reports the outcome into model context.
   The model may pass the exact text of any corpus sentence from
   `get_sentences`; the server verifies it and supplies the real translation
   and attribution. Degrades to a polite error when the widget isn't built.
- `get_audio` — widget-only (`ui.visibility: ["app"]`): resolves audio the
   same way the app does (human voice-actor recording from the language pack
   first — shared `human_audio` registry — then TTS via the AI backend) and
   returns base64 + attribution.
- `search` / `fetch` — the standard browse/cite pair (required by ChatGPT for
  deep research and connector validation). `search` returns dictionary entry
  pages as `{id, title, url}`; `fetch` takes an `id`
  (`"<course-slug>:<frequency-index>"`) and returns the entry as readable
  text with a citable `url` into the public dictionary at `yap.town/d/`, plus
  the exact gram in `metadata`. URLs are best-effort: colliding display texts
  get a numeric suffix at site build time we can't reproduce, so rare
  homographs may 404 (sampled hit rate: 39/39).

All tools carry MCP annotations (`title`, `readOnlyHint`/`destructiveHint`,
etc.), which both the Anthropic and OpenAI directories require.

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

- `/oauth/authorize` parks the client's request and redirects to
  `yap.town/connect`, where the user signs in with their normal yap auth
  (same domain, so future passkeys work) and approves. The frontend posts
  proof back to `/oauth/approve`, which mints the connector a **fresh**
  Supabase session (admin generate_link + verify — reusing the browser's
  session would entangle two devices in one refresh-token rotation family)
  and hands back the redirect with a single-use authorization code. The
  frontend hardcodes which MCP origins it will send proof to.
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

Env: `SUPABASE_JWT_SECRET` and `SUPABASE_SERVICE_ROLE_KEY` (required),
`YAP_MCP_BASE_URL` (public URL, used in metadata and Host validation),
`YAP_FRONTEND_URL` (where /connect lives), `YAP_MCP_ALLOWED_HOSTS` (extra Host
values, e.g. the fly.dev name behind the proxy), `PORT`, `SUPABASE_URL`,
`SUPABASE_ANON_KEY`, `YAP_OUT_DIR`.

The public domain `mcp.yap.town` is a Cloudflare Worker pass-through to
`yap-mcp.fly.dev` (`proxy-worker/`; deploy with `wrangler deploy` from that
directory). The worker approach exists because attaching a Workers custom
domain auto-provisions DNS + TLS without needing Fly-side certificates.

Deploy on Fly (first time: `flyctl launch --no-deploy --copy-config --config
yap-mcp/fly.toml`, then set the secret):

```bash
flyctl secrets set --config yap-mcp/fly.toml SUPABASE_JWT_SECRET=...
flyctl deploy --config yap-mcp/fly.toml
```

Then in claude.ai: Settings → Connectors → Add custom connector → URL
`https://mcp.yap.town/mcp`. Claude discovers the OAuth endpoints, opens the
yap login page, and connects.

## Current limitations

- stdio mode's auth is service-role + email (fine for personal/local use);
  serve mode is the multi-user path.
- Approval uses whatever auth the yap.town frontend supports (today email +
  password; passkeys would work there when added).
- Listening cards can be quizzed only as text in a chat client.
- No challenge generation (translation/transcription exercises) yet; the chat
  LLM is expected to quiz the user itself, e.g. using `get_sentences`.
- OAuth codes and MCP sessions are in-memory: run exactly one instance.
- Public OAuth endpoints are rate-limited per client IP (fixed 60s windows);
  cached per-user deck state is evicted after 6h idle.

## Review widget (MCP Apps)

`widget/` is a small Vite/React app that reuses yap-frontend components
verbatim (AudioButton, shadcn ui, theme) via aliases; shims keep the 4 MiB
WASM out (`src/shims/`). Build with `pnpm install && pnpm build` in
`yap-mcp/widget/` — the server loads `widget/dist/index.html` at runtime
(`YAP_WIDGET_HTML` overrides the path) and serves it as
`ui://yap/review.html` (`text/html;profile=mcp-app`) with a fully closed
CSP: the widget talks only through the host bridge (`get_audio`,
`log_review`, `updateModelContext`). CI builds it before the Docker image;
a server without the file just runs text-only.

## Directory review account

Reviewers for the Anthropic/OpenAI directories get the throwaway account
`yap-mcp-test@popovit.ch` (see `smoke/`). To reset and re-populate it with a
believable deck and review history:

```bash
set -a; source .env; set +a
YAP_TEST_USER_PASSWORD=<reviewer password> python3 yap-mcp/smoke/setup_test_user.py
YAP_USER_EMAIL=yap-mcp-test@popovit.ch python3 yap-mcp/smoke/seed_test_user.py target/debug/yap-mcp
```

Public docs for end users live at `yap.town/mcp`; privacy policy at
`yap.town/privacy`.

## When Supabase moves to asymmetric signing keys

The current setup verifies user access tokens and signs/encrypts our own MCP
tokens with the shared `SUPABASE_JWT_SECRET`. Migration is mechanical, and the
`generate_link` session-minting is unaffected:

1. Verify Supabase user tokens via the project JWKS (RS256/ES256) instead of
   the HS256 secret (`verify_supabase_token`).
2. Introduce a dedicated `MCP_TOKEN_SECRET` for signing our access/refresh
   tokens (`issue_tokens`, `verify_access_token`, `verify_refresh_token`) —
   they were only ever verified by us.
3. Derive the session-encryption key from that same new secret
   (`RemoteApp::new`). Existing connections re-authorize once.
