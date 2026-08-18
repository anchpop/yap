# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Tips

To clean up unused imports in rust code, you can generally just run `cargo fix`. No need to do it yourself! Then run `cargo fmt` afterwards to clean everything up. Make sure everything passes `cargo clippy`, it's very helpful! One important tip: you do not need to cd anywhere to use these commands. You can use them from the root of the project, because the root of the project defines a cargo workspace.

To make sure you can still make a release WASM build with LTO, you do have to `cd` into `yap-frontend-rs` and then run `CARGO_PROFILE_RELEASE_LTO=true wasm-pack build --release --features local-backend`.

Whenever possible, I want you to use `cargo fix`, `cargo clippy --fix`, and `cargo fmt`.

## Project Architecture

Yap.Town is a language learning application with a Rust-based backend and React frontend architecture:

### Core Components

- **yap-frontend-rs**: WASM module built with Rust providing core language learning logic, spaced repetition (FSRS), and offline data storage via OPFS
- **yap-frontend**: React/TypeScript frontend using Vite, with Tailwind CSS and Radix UI components
- **generate-data**: Rust binary that extracts sentences from Anki decks and generates dictionary data using Python NLP
- **language-utils**: Shared Rust library containing language processing types and utilities
- **libraries/movie-subtitles**: Movie subtitle text handling — raw SRTs as source of truth, lossy cleaning at load time, CP1252 mojibake repair
- **libraries/opensubtitles-downloader**: Downloads course subtitles from OpenSubtitles; its `recover-subtitles` binary re-fetches the raw SRTs the old pipeline threw away and adjudicates near-misses
- **libraries/subtitle-corpus**: Builds a correctly-synced subtitle per film in the movie library (inventory → disc extract/OCR → Whisper/VAD/text-to-text sync → calibration), the substrate for cutting per-sentence clips
- **generate-dictionary-data**: Rust binary that reads `.rkyv` language pack archives and outputs structured JSON for the public dictionary site
- **static-site**: Astro static site generator that builds ~178k dictionary pages from the JSON data, using Tailwind CSS v4
- **yap-ai-backend**: Rust backend service for AI features (deployed on Fly.io)
- **modal-llm-server**: Python FastAPI service for LLM inference using Modal. (Not currently used.)
- **supabase/**: Database and authentication configuration

Cloudflare for hosting.

### Data Flow

1. Sentence corpora (movie subtitles, Tatoeba, books) are tokenized/lemmatized/dependency-parsed by lexide (a sibling repo: fine-tuned Gemma 3 1B doing structured linguistic analysis, one model for all course languages except zho-hant, served via Modal); multiword terms come from Wiktionary category listings and are processed through the same pipeline as sentences
2. Generated data is embedded into the WASM module as static assets
3. Frontend uses WASM module for offline-first language learning features
4. Supabase handles user authentication and event syncing
5. AI features are handled by separate backend services

### Public Dictionary Site

The public dictionary lives at `/d/` and is built as a static site that gets copied into the Vite frontend's `public/` directory. The pipeline:

1. **Rust** (`generate-dictionary-data`): Reads `.rkyv` language pack archives → outputs JSON to `static-site/src/data/`. Produces a lightweight index JSON per course (for listing pages) and individual per-page JSON files (with full sentence data including cross-linked glosses). Data is split this way because loading everything into one JSON would OOM Node during Astro build.
2. **Astro** (`static-site`): Reads the JSON and generates ~178k static HTML pages with Tailwind CSS v4 (`@tailwindcss/vite`). Output goes directly to `yap-frontend/public/d/` via Astro's `outDir` config.
3. **Vite**: A custom plugin (`dictionaryStaticPlugin` in `yap-frontend/vite.config.ts`) intercepts `/d/` routes before the SPA fallback, serving the pre-built static HTML instead.
4. **Cloudflare**: Serves the static dictionary pages alongside the SPA. The CI workflow builds dictionary data → Astro → then the Vite frontend.

Key details:
- Sentences are cross-linked: each gram in a sentence links to its dictionary page and includes a native-language gloss
- The Astro build needs `NODE_OPTIONS="--max-old-space-size=8192"` due to the volume of pages
- All generated data (`static-site/src/data/`, `yap-frontend/public/d/`) is gitignored
- Must run `cargo run --release --bin generate-dictionary-data` from the repo root (not from `static-site/`) since it looks for `.rkyv` files in `out/`

## Essential Commands

### Setup and Installation

```bash
# Install espeak-ng (required by generate-data for phrase-level phonemization
# in audio_verification — Mac: `brew install espeak-ng`, Linux: apt/yum)
espeak-ng --version

# Generate dictionary data from Anki decks
cargo run --bin generate-data

# Build WASM module
cd yap-frontend-rs && CARGO_PROFILE_RELEASE_LTO=true wasm-pack build --release

# Install frontend dependencies and build
cd yap-frontend && pnpm install && pnpm build
```

### Dictionary Site

```bash
# Generate dictionary JSON from language pack archives (run from repo root!)
cargo run --release --bin generate-dictionary-data

# Build static dictionary pages (outputs to yap-frontend/public/d/)
cd static-site && pnpm install && NODE_OPTIONS="--max-old-space-size=8192" npx astro build
```

### Development

```bash
# Frontend development server
cd yap-frontend && pnpm dev

# Frontend linting
cd yap-frontend && pnpm lint

# Frontend type checking
cd yap-frontend && tsc -b

# Build all Rust components
cargo build --release

# Test Rust components
cargo test

# Supabase local development
cd supabase && supabase start
```

### yap-mcp smoke tests

`yap-mcp/smoke/` holds end-to-end smoke scripts for the MCP server (see the
docstring in each for usage). `stdio_read.py` is safe against a real account;
the write-path and remote-OAuth scripts use the throwaway
`yap-mcp-test@popovit.ch` account (reset it with `setup_test_user.py`). Run
them after changing yap-mcp, the deck event flow, or the OAuth layer:

```bash
YAP_USER_EMAIL=you@example.com python3 yap-mcp/smoke/stdio_read.py target/debug/yap-mcp
python3 yap-mcp/smoke/remote_oauth.py target/debug/yap-mcp serve
```

The MCP Apps review widget lives in `yap-mcp/widget/` (Vite; build with
`pnpm install && pnpm build` there). It reuses yap-frontend components
verbatim via aliases — including the app's own `Flashcard` — so a change to
those components flows to the widget automatically. If you change AudioButton,
the ui/ primitives, `Flashcard`, or lib/pure.ts, run the widget's `pnpm check`
too. Any component reachable from the widget must stay free of WASM *value*
imports (`import type` is fine): the `wasm-guard` alias turns a stray value
import into a loud build failure, and `pnpm build` runs in CI, so this is
enforced mechanically — no human vigilance required.

### Key Technologies

- **Rust**: Core logic, WASM compilation, backend services
- **WASM-Pack**: For building Rust to WebAssembly
- **React 19 + TypeScript**: Frontend framework
- **Vite**: Frontend build tool and dev server
- **Tailwind CSS + Radix UI**: Styling and components
- **Supabase**: Database, auth, and real-time features
- **OPFS**: Browser-based persistent file storage for offline data
- **lexide**: NLP base layer for sentence analysis (POS, lemmas, dependencies) — fine-tuned Gemma 3 1B in a sibling repo, served via Modal
- **FSRS**: Spaced repetition algorithm implementation

### Important Notes

- The build process is complex and requires multiple tools: Rust, wasm-pack, uv (Python), and pnpm
- WASM module must be rebuilt after changes to `yap-frontend-rs`
- Sentence tokenization goes through lexide's Modal endpoint (results cached in per-language `*_tokenization.jsonl` files)
- Frontend depends on the local WASM package at `../yap-frontend-rs/pkg`
- Use `uv` for Python dependency management in NLP components
- Use `pnpm` for JavaScript/TypeScript dependencies
- All target-language text in yap-frontend should use the TargetLanguage component 
- The privacy policy at `yap-frontend/src/pages/privacy.tsx` (yap.town/privacy) must stay accurate: whenever a change is privacy-affecting — a new third-party service, new data collected or stored, telemetry changes, a new place user content is sent — update the policy (and its effective date) in the same PR.
- For spacing between siblings in yap-frontend, prefer parent-driven spacing — Tailwind `gap-*` on a flex/grid parent (e.g. the Card component's gap) or a `space-y-*` wrapper — over per-child margins. Keep spacing uniform at the container level.
- Events are append-only and immutable. Once an event has been written to Supabase (the `events` table) it must never be mutated, upserted, or deleted — other devices have already synced it, and the deck is the fold of an immutable event log. To "change" something, append a new event; never rewrite history. This means: no `Prefer: resolution=merge-duplicates`/upsert on the events endpoint, no editing a row's payload, no reusing a `within_device_events_index`. Idempotency and dedup must be achieved *before* an event is written (e.g. an idempotency token in the MCP server, or the local event store's own dedup), not by editing what's already stored.

Final most important note: We do not have to worry about backwards compatibility with respect to our API. We mostly only have to worry about it for our events, in yap-frontend-rs/src/deck_event, as those types are serialized to disk. We do not worry about it with our API. Rust functions, traits, structs... except for those implicated by yap-frontend-rs/src/deck_event, do not worry! All the code is here, it's basically a monorepo, so we can always fix all the breakage and that's always better than leaving scar tissue from old code. If that makes sense to you, please start conversations by saying "I will maintain backwards compatibility with regards to our structs that get serialized, but make all the changes necessary across the codebase to write clean and minimal code without worrying about backwards compatibility in our internal APIs."
