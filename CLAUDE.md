# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Tips

To clean up unused imports in rust code, you can generally just run `cargo fix`. No need to do it yourself! Then run `cargo fmt` afterwards to clean everything up. Make sure everything passes `cargo clippy`, it's very helpful! One important tip: you do not need to cd anywhere to use these commands. You can use them from the root of the project, because the root of the project defines a cargo workspace.

To make sure you can still make a wasm build, you do have to `cd` into `yap-frontend-rs` and then run `wasm-pack build --features local-backend`. 

Whenever possible, I want you to use `cargo fix`, `cargo clippy --fix`, and `cargo fmt`.

## Project Architecture

Yap.Town is a French language learning application with a Rust-based backend and React frontend architecture:

### Core Components

- **yap-frontend-rs**: WASM module built with Rust providing core language learning logic, spaced repetition (FSRS), and offline data storage via OPFS
- **yap-frontend**: React/TypeScript frontend using Vite, with Tailwind CSS and Radix UI components
- **generate-data**: Rust binary that extracts sentences from Anki decks and generates dictionary data using Python NLP
- **language-utils**: Shared Rust library containing language processing types and utilities
- **yap-ai-backend**: Rust backend service for AI features (deployed on Fly.io)
- **modal-llm-server**: Python FastAPI service for LLM inference using Modal. (Not currently used.)
- **supabase/**: Database and authentication configuration

### Data Flow

1. Anki decks are processed by `generate-data` using Python spaCy NLP for French multiword term detection
2. Generated data is embedded into the WASM module as static assets
3. Frontend uses WASM module for offline-first language learning features
4. Supabase handles user authentication and event syncing
5. AI features are handled by separate backend services

## Essential Commands

### Setup and Installation

```bash
# Install French NLP model (required first)
cd ./generate-data/nlp && uv pip install https://github.com/explosion/spacy-models/releases/download/fr_dep_news_trf-3.8.0/fr_dep_news_trf-3.8.0-py3-none-any.whl

# Generate dictionary data from Anki decks
cargo run --bin generate-data

# Build WASM module
cd yap-frontend-rs && wasm-pack build --release

# Install frontend dependencies and build
cd yap-frontend && pnpm install && pnpm build
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

### Key Technologies

- **Rust**: Core logic, WASM compilation, backend services
- **WASM-Pack**: For building Rust to WebAssembly
- **React 19 + TypeScript**: Frontend framework
- **Vite**: Frontend build tool and dev server
- **Tailwind CSS + Radix UI**: Styling and components
- **Supabase**: Database, auth, and real-time features
- **OPFS**: Browser-based persistent file storage for offline data
- **spaCy**: French NLP processing for multiword term detection
- **FSRS**: Spaced repetition algorithm implementation

### Important Notes

- The build process is complex and requires multiple tools: Rust, wasm-pack, uv (Python), and pnpm
- WASM module must be rebuilt after changes to `yap-frontend-rs`
- Dictionary data generation requires the French spaCy transformer model
- Frontend depends on the local WASM package at `../yap-frontend-rs/pkg`
- Use `uv` for Python dependency management in NLP components
- Use `pnpm` for JavaScript/TypeScript dependencies
