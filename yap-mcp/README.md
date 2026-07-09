# yap-mcp

An MCP (Model Context Protocol) stdio server that exposes a yap.town account to
chat clients: do reviews, add words, look things up in the dictionary, pull
comprehensible example sentences, and check stats — all backed by the same
event stream the web app syncs, so everything shows up across devices.

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

## Current limitations

- Auth is service-role + email, which is fine for personal/local use only. A
  public version (e.g. a ChatGPT app) needs a remote MCP server with per-user
  JWTs — `yap-ai-backend` already verifies Supabase JWTs and would be the
  natural host.
- Listening cards can be quizzed only as text in a chat client.
- No challenge generation (translation/transcription exercises) yet; the chat
  LLM is expected to quiz the user itself, e.g. using `get_sentences`.
