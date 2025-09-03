# Yap.Town 

Currently supports french, with experimental support for spanish.

## Build process

The build process is somewhat convoluted. It requires Rust, wasm-pack, uv, and pnpm.

1. Install the spaCy French NLP module: `cd ./generate-data/nlp && uv pip install https://github.com/explosion/spacy-models/releases/download/fr_dep_news_trf-3.8.0/fr_dep_news_trf-3.8.0-py3-none-any.whl`
2. Extract sentences from the Anki decks and generate the dictionary: `cargo run --bin generate-data`
3. Build the wasm module: `cd yap-frontend-rs` `wasm-pack build --release`
4. Build the frontend: `cd yap-frontend` `pnpm build`

## The server

Aside from relying on supabase, the frontend also relies on a server at yap-ai-backend.fly.io, code in `yap-ai-backend/`. The server requires the following environment variables be set:

```
OPENAI_API_KEY=
ELEVENLABS_API_KEY=
SUPABASE_JWT_SECRET=
SUPABASE_URL=
SUPABASE_SERVICE_ROLE_KEY=
GOOGLE_CLOUD_API_KEY=
```

# data

thanks to 

neri's sentences
wikipron for phoneticshttps://github.com/CUNY-CL/wikipron/tree/master 
