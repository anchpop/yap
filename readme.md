# Yap.Town

**A Modern, Spaced-Repetition Language Learning App**

Check it out on [yap.town](https://yap.town)!

Join the community on [Discord](https://discord.gg/mpgqfsH).

**Yap's goal is to be the #1 most effective language learning app.**

The idea is basically to combine Anki-style spaced repetition with comprehensible input. You add vocabulary to your deck, like any flashcard app. But Yap has a corpus of sentences, and can show you a sentence containing the word you need to review. You review the whole sentence my translating it or listening to it, and then Yap records what you got right and what you got wrong. It then uses that information to give you more sentences, always prompting you to review exactly what you need to!

![Yap example](docs/yap-example-image.png)

> I have a seemingly endless supply of sentences to translate containing words at my level with immediate feedback. Amazing. I’ve used Duolingo in the past, but it often felt like rote memorization of sentences that I’d never actually use. I’ve used Anki too, but sentence practice isn’t as granular as Yap Town. I think there’s really something special here and I‘d definitely recommend it to anyone interested in learning a new language.

– Jarret

## Supported Languages

1. French
2. Spanish
3. German
4. Italian (beta)
5. Portuguese (beta)
3. Korean (alpha)

## Supported learning modalities

1. Reading
2. Listening

## Why other apps fall short

Most apps are optimized for engagement or are otherwise poorly designed, making them very ineffective. Language learning apps typically have two main flaws.

1. They do not effectively utilize spaced repetition.
2. They teach words and grammatical concepts in an ineffective order.

Spaced repetition is the #1 most important thing a language learning app could possibly provide. It is the foundation of time-efficient focused study. Yet most language-learning tools relegate it to a curiosity in an out-of-the-way section of the app, if they make use of it at all!

In other apps, the order that words are taught in is also very inefficient. The most common words like "to", "from", "of", "I", "who", "that", and so on are the most common, so they should be learned first. But apps spend time teaching you how to say sentences like "the man is eating an apple", even though words like "man" and "apple" are incredibly rare by comparison.

You can do better than most by creating Anki decks with vocabulary words, but the issue is that you won't encounter words in their natural sentence context. Without this context, it becomes much harder to recall words when seeing them in sentences rather than isolated in an Anki deck.

Yap solves this problem by implementing spaced repetition through sentences containing the target word, and asking users to translate the entire sentence. A side-benefit is that upon successful translation, we can mark every word in the sentence as having been successfully repeated. (Even if you mistranslate a word that wasn't the intended focus of the repetition, we can still log that data, ultimately providing much more data to the SRS and much better practice than a typical Anki session would.)

## Spaced repetition features

The app does not do spaced repetition at the level of words. Instead, it works on the level of `(word, lemma, part of speech)` triples. This allows the spaced repetition system to more intelligently schedule sentences.

1. For example, the word "le" in French can be used as an article or as a pronoun. When you mistranslate a sentence that uses "le" as a pronoun, we detect this and only mark that specific usage as needing repetition. You won't get followed up with sentences using "le" as an article, since that's not what you misunderstood.
2. Another example is the word "suis," which can be a conjugation of "être" or "suivre" in French. Misunderstanding one of these should result in more sentences that use the specific conjugation you misunderstood, even though the word is spelled the same. (To accomplish this, we use natural language processing on our dataset of sentences.)

In addition to words, YAP also has "multi-word terms" as part of its spaced repetition system. For example, the French term "se passer" means "to happen" or "to take place." You wouldn't understand this meaning by looking at the individual words alone. Therefore, we have separate SRS entries for such expressions. This ensures that you learn complete phrases and expression, in addition to individual words.

The words that Yap chooses to introduce are initially based on which words are most common. As you use Yap, we build a model using isotonic regression to assess how difficult you find words based on their frequency. If you consistently find the shown words easy, Yap will begin introducing rarer words to identify vocabulary you don't already know. This approach allows Yap to quickly adapt to your existing skill level and reduces time wasted on reviewing words you know from outside of Yap.

## Other features of Yap I'm proud of

Of course, in addition to being the most effective language learning app, I couldn't live with myself if I didn't think that YAP was also just a generally pleasant app to use. To that end, there are some features of YAP that I'm proud of that set it apart from most other apps on the internet.

1. Instant sync across all your devices.
2. You can use Yap while logged out, and it functions almost exactly the same as when you're logged in. (The exception is features that fundamentally require an account, like cross-device sync.) As soon as you do log in, all of your data is migrated to your account, and you can pick up exactly where you left off.
3. Yap works seamlessly offline once installed as a Progressive Web App. All of the language data is downloaded to your device and challenge selection etc. is all done locally.
4. Yap is quite fast, with most operations taking less than the time to render one frame, despite processing large amounts of sentences. Yap's performance benefits from being primarily written in Rust (and compiled to WebAssembly to run in the browser). We also implement various optimizations, including string interning, which allows most operations to work with objects that fit entirely in the stack (removing the need for most heap access or allocations).

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

The data in out/ is generated via the `generate-data` binary. 

```
cargo run --bin generate-data --release
```

Each individual step is cached inside a file in the out/ directory. To rerun a step, you need to delete the cache file. For example, if you want to rerun the dictionary generation for a language, you'll need to delete the dictionary file for that language. LLM calls are cached in the .cache directory. This allows you to rerun a step without spending a ton of money.

The NLP is extremely slow. I rented a GH200 from Lambda Labs when I need to recalculate it.

## Data Cleaning (for custom NLP training data)

The NLP model used by Yap (lexide) is trained from data in this repo. Generating that data requires python.

1. Install the spaCy NLP module:

```bash
cd ./generate-data/nlp && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/fr_dep_news_trf-3.8.0/fr_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/es_dep_news_trf-3.8.0/es_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ko_core_news_lg-3.8.0/ko_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/en_core_web_trf-3.8.0/en_core_web_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/de_dep_news_trf-3.8.0/de_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/zh_core_web_trf-3.8.0/zh_core_web_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/it_core_news_lg-3.8.0/it_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/pt_core_news_lg-3.8.0/pt_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ja_core_news_trf-3.8.0/ja_core_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ru_core_news_lg-3.8.0/ru_core_news_lg-3.8.0-py3-none-any.whl
```

2. Generate the data

```
cargo run --bin clean-nlp-data clean
```

## Supabase / Onesignal

Accounts and cross-device sync uses supabase as a backend. Migrations are in the supabase/ folder. Onesignal is used for notifications.

## Special thanks

### Data

1. Tatoeba
2. neri's frequency lists (a bit redundant because they're sourced from tatoeba, but they're convenient to have)
3. wiktionary/[wikipron](https://github.com/CUNY-CL/wikipron/tree/master) for phonetics
4. [opensubtitles](http://www.opensubtitles.org/) and TMDB for the movie integration!
5. [Michael Oeser](https://unsplash.com/photos/black-and-gray-textile-in-close-up-photography-X7jvviscg8o) and [Corina Rainer](https://unsplash.com/photos/white-cotton-on-white-textile-jZc5eTXnYLU) on unsplash (their images are used in the background)

### Libraries

1. [Pair Adjacent Violators for Rust](https://github.com/sanity/)
2. [The Open Spaced Repetition group](https://github.com/open-spaced-repetition)
