# Yap.Town 

**A Modern, Spaced-Repetition Language Learning App**

Check it out on [yap.town](https://yap.town)!

Join the community on [Discord](https://discord.gg/mpgqfsH).

## Supported Languages

1. French
2. Spanish (beta)

## Why Yap?

Other language learning apps I tried are very ineffective for language learning. They have two main flaws. 

1. They do not effectively utilize spaced repetition. 
2. They teach words in an ineffective order.

Spaced repetition is the #1 most important thing a language learning app could possibly provide. It is the foundation of time-efficient focused study. 

The order that words are taught in is also very inefficient. The most common words like "to", "from", "of", "I", "who", "that", and so on are the most common, so they should be learned first. But apps spend time teaching you how to say sentences like "the man is eating an apple", even though words like "man" and "apple" are incredibly rare by comparison.

## Build Process

Build the rust library

```bash
cd yap-frontend-rs
wasm-pack build
```

Then, run the page

```bash
cd yap-frontend
pnpm i 
pnpm dev
```

There is also a supporting backend, normally assumed to be at `https://yap-ai-backend.fly.io`. But If you build the rust library with `wasm-pack build --features "local-backend`, it will look for the server on `localhost:8080`. You can then run the server locally with `cd yap-ai-backend && cargo run`. 

## Data Generation

The data in out/ is generated via the `generate-data` binary. Running it is somewhat convoluted. It requires python as well. 

1. Install the spaCy French and Spanish NLP module: `cd ./generate-data/nlp && uv pip install https://github.com/explosion/spacy-models/releases/download/fr_dep_news_trf-3.8.0/fr_dep_news_trf-3.8.0-py3-none-any.whl && uv pip install https://github.com/explosion/spacy-models/releases/download/es_dep_news_trf-3.8.0/es_dep_news_trf-3.8.0-py3-none-any.whl && uv pip install https://github.com/explosion/spacy-models/releases/download/ko_core_news_lg-3.8.0/ko_core_news_lg-3.8.0-py3-none-any.whl`
2. Generate the data

### Data Sources special thanks

1. neri's frequency lists
2. wikipron for phonetics https://github.com/CUNY-CL/wikipron/tree/master 
3. opensubtitles http://www.opensubtitles.org/ (downloaded via https://opus.nlpl.eu/OpenSubtitles/ko&en/v2024/OpenSubtitles)

## Supabase

Accounts and cross-device sync uses supabase as a backend. Migrations are in the supabase/ folder.
