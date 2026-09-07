# Yap.Town

**A Modern, Spaced-Repetition Language Learning App**

Check it out on [yap.town](https://yap.town)!

Join the community on [Discord](https://discord.gg/mpgqfsH).

**Yap's goal is to be the #1 most effective language learning app.**

The idea is basically to combine Anki-style spaced repetition with comprehensible input. You add vocabulary to your deck, like any flashcard app. But Yap has a corpus of sentences, and can show you a sentence containing the word you need to review. You review the whole sentence by translating it or listening to it, and then Yap records what you got right and what you got wrong. It then figures out what you got right and what you didn't, and feeds all of that back into the spaced repetition system. That way it can always prompt you to review exactly what and when you need to!

![Yap example](docs/yap-example-image.png)

> I have a seemingly endless supply of sentences to translate containing words at my level with immediate feedback. Amazing. I’ve used Duolingo in the past, but it often felt like rote memorization of sentences that I’d never actually use. I’ve used Anki too, but sentence practice isn’t as granular as Yap Town. I think there’s really something special here and I‘d definitely recommend it to anyone interested in learning a new language.

– Jarret (Yap user)

## Supported Languages

1. French
2. Spanish
3. German
4. Italian (beta)
5. Portuguese (beta)
6. Russian (alpha)
7. Korean (alpha)
8. Japanese (alpha)
9. Simplified Chinese (alpha)
10. Traditional Chinese (alpha)
11. Hindi (alpha)
12. Thai (alpha)

## Supported learning modalities

1. Reading
2. Listening

## Why other apps fall short

Most apps are optimized for engagement or are otherwise poorly designed, making them very ineffective. Language learning apps typically have two main flaws.

1. They do not effectively utilize spaced repetition.
2. They teach words and grammatical concepts in an ineffective order.

Spaced repetition is the #1 most important thing a language learning app could possibly provide. It is the foundation of time-efficient focused study. Yet most language-learning tools relegate it to a curiosity in an out-of-the-way section of the app, if they make use of it at all!

In other apps, the order that words are taught in is also very inefficient. The most common words like "to", "from", "of", "I", "who", "that", and so on are the most common, so they should be learned first. But apps spend time teaching you how to say sentences like "the man is eating an apple", even though words like "man", "eating", and "apple" are incredibly rare by comparison.

You can do much better than most people by creating Anki decks with vocabulary words. But the issue with that is you lose a major benefit of Duolingo, which is seeing words in their natural sentence context. Without this context, it becomes much harder to recall words when seeing them in sentences rather than isolated in an Anki deck.

Yap solves this problem by implementing spaced repetition *through* sentences containing the target word, and asking users to translate the entire sentence. A side-benefit is that upon successful translation, we can mark every word in the sentence as having been successfully repeated. (Even if you mistranslate a word that wasn't the intended focus of the repetition, we can still log that data, ultimately providing much more data to the SRS and much better practice than a typical Anki session would.)

## Spaced repetition features

This part will be a little more technical. The app does not do spaced repetition at the level of words. Instead, it works on the level of `Vector<(word, lemma, part of speech)>`. This allows the spaced repetition system to more intelligently schedule sentences.

1. For example, the word "le" in French can be used as an article or as a pronoun. When you mistranslate a sentence that uses "le" as a pronoun, we detect this and only mark that specific usage as needing repetition. You won't get followed up with sentences using "le" as an article, since that's not what you misunderstood.
2. Another example is the word "suis," which can be a conjugation of "être" or "suivre" in French. Misunderstanding one of these should result in more sentences that use the specific conjugation you misunderstood, even though the word is spelled the same. (To accomplish this, we use natural language processing on our dataset of sentences.)
3. A third example is very common sequences of words, like "c'est". "c'est" is technically two separate words, but you really want to treat it as its own thing that gets reviewed independently of the two words that make it up.

The words that Yap chooses to introduce are initially based on which words are most common. As you use Yap, it builds a model using isotonic regression to assess how difficult you find words based on their frequency. If you consistently find the shown words easy, Yap will begin introducing rarer words to identify vocabulary you don't already know. This approach allows Yap to quickly adapt to your existing skill level and reduces time wasted on reviewing words you know from outside of Yap.

## Other features of Yap I'm proud of

Of course, in addition to being the most effective language learning app, I couldn't live with myself if I didn't think that Yap was also just a generally pleasant app to use. To that end, there are some features of Yap that I'm proud of that set it apart from most other apps on the internet.

1. Instant sync across all your devices.
2. You can use Yap while logged out, and it functions almost exactly the same as when you're logged in. (The exception is features that fundamentally require an account, like cross-device sync.) As soon as you do log in, all of your data is migrated to your account, and you can pick up exactly where you left off.
3. Yap works seamlessly offline once installed as a Progressive Web App. All of the language data is downloaded to your device and challenge selection etc. is all done locally.
4. Yap is quite fast, with most operations taking less than the time to render one frame, despite processing large amounts of sentences. Yap's performance benefits from being primarily written in Rust (and compiled to WebAssembly to run in the browser). We also implement various optimizations, including string interning, which allows most operations to work with objects that fit entirely in the stack (removing the need for most heap access or allocations).
5. The placement test at the beginning, and the "adaptive placement" as you use the app, where we use the pool adjacent violators algorithm to compute your level based on the frequency rank of the words you do and don't know. It's totally extra, but I actually use a "smoothed" version of the isotonic regression curve, and making that fast ended up being an [insane rabbit hole](https://github.com/anchpop/pav.rs/blob/master/src/smooth_regression.rs).
6. The audio in the app is mostly TTS, but it's verified with a combination of strategies. Some is verified with a simple whisper check, but some is verified with a custom [audio → phoneme model](https://github.com/anchpop/lexide/) (which will soon be used to grade user pronunciations!)

## Build Process

Build the rust library

```bash
cargo bridgerton web --package yap-frontend-rs
```

Then, run the page

```bash
cd yap-frontend
pnpm i
pnpm dev
```

There is also a supporting backend, normally assumed to be at `https://yap-ai-backend.fly.io`. But if you build the rust library with `--features local-backend`, it will look for the server on `localhost:8080`. You can then run the server locally with `cd yap-ai-backend && cargo run`.

## Data Generation

The data in out/ is generated via the `generate-data` binary. 

```
cargo run --bin generate-data --release
```

Each individual step writes artifacts to a file in the out/ directory, for you to inspect. LLM calls are cached in the .cache directory. This allows you to rerun a step without spending a ton of money.

The NLP is extremely slow. It runs on [lexide](https://github.com/anchpop/lexide)'s Modal endpoint (currently an A100-80GB serving the fine-tuned model via vLLM).

## Data Cleaning (for custom NLP training data)

The NLP model used by Yap (lexide) is trained from data in this repo. See [libraries/clean-nlp-data](libraries/clean-nlp-data/README.md) for setup (spaCy model installation) and usage.

## Supabase / Onesignal

Accounts and cross-device sync use supabase as a backend. Migrations are in the supabase/ folder. Onesignal is used for notifications.

## MCP

You can connect Yap to your LLM provider of choice using the following MCP server: https://mcp.yap.town/mcp

<img width="1056" height="398" alt="CleanShot 2026-07-11 at 19 30 34@2x" src="https://github.com/user-attachments/assets/9e9046c7-ff4b-401c-8fa2-af9331568b9a" />

## Special thanks

[The Open Spaced Repetition group](https://github.com/open-spaced-repetition)

### Data

1. Tatoeba
2. neri's frequency lists (a bit redundant because they're sourced from tatoeba, but they're convenient to have)
3. wiktionary/[wikipron](https://github.com/CUNY-CL/wikipron/tree/master) for phonetics
4. [opensubtitles](http://www.opensubtitles.org/) and TMDB for the movie integration!
5. [Michael Oeser](https://unsplash.com/photos/black-and-gray-textile-in-close-up-photography-X7jvviscg8o) and [Corina Rainer](https://unsplash.com/photos/white-cotton-on-white-textile-jZc5eTXnYLU) on unsplash (their images are used in the background)

### Coming soon

Soon... Pronunciation grading with a [custom audio → phoneme model](https://huggingface.co/anchpop/lexide-pronunciation)! I'm very excited about this, because with my model I can actually represent things like Japanese pitch accent and French's unusual stress patterns.
