//! Translate a book's extracted chunks into the course languages and segment
//! them into sentences — producing `data/<lang>/sentence-sources/books/<series>/<book>.jsonl`
//! (the source consumed by `generate_data::books`). Remember to add the book to the
//! series' `metadata.jsonl` in the same folder.
//!
//!     cargo run -p generate-data --release --bin translate_book -- \
//!         --chunks /data/books/pale-lights-extracted/chunks.jsonl \
//!         --series pale-lights --book pale-lights \
//!         [--take 300] [--model gpt-5.6-luna] [--source-lang eng] [--only zho-hans]
//!
//! Run from the repo root (paths and `.cache/` are root-relative). Translations go
//! through tysm (cached in `.cache/`, synced by osmo); segmentation goes through the
//! parsley `/segment` serve with a language hint. Incremental: chunks already present in
//! an output file are skipped, so reruns only fill gaps. The source language (`--source-lang`,
//! default `eng`) skips translation and segments the original text — e.g. a Chinese-original
//! book runs with `--source-lang zho-hans --only zho-hans`.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, bail};
use futures::StreamExt;
use generate_data::books::BookSentence;
use generate_data::cache_remote;
use lexide::{Language, RemoteClient, RemoteConfig};
use serde::{Deserialize, Serialize};
use tysm::chat_completions::ChatClient;

/// (output code, display name for the translation prompt, segmenter language hint).
/// Simplified Chinese has no `lexide::Language` variant, so it segments locally with
/// [`segment_chinese`] — the parsley `/segment` serve filters out content it judges
/// nontextual (by design), but book prose is all textual and we want it lossless.
const LANGS: [(&str, &str, Option<Language>); 11] = [
    ("deu", "German", Some(Language::German)),
    ("eng", "English", Some(Language::English)),
    ("fra", "French", Some(Language::French)),
    ("hin", "Hindi", Some(Language::Hindi)),
    ("ita", "Italian", Some(Language::Italian)),
    ("jpn", "Japanese", Some(Language::Japanese)),
    ("kor", "Korean", Some(Language::Korean)),
    ("por", "Portuguese", Some(Language::Portuguese)),
    ("rus", "Russian", Some(Language::Russian)),
    ("spa", "Spanish", Some(Language::Spanish)),
    ("zho-hans", "Simplified Chinese", None),
];

static MODEL: LazyLock<String> = LazyLock::new(|| {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--model")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "gpt-5.6-luna".to_string())
});

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    generate_data::apply_cache_only(
        ChatClient::from_env(MODEL.as_str())
            .unwrap()
            .with_cache_directory("./.cache")
            .with_service_tier("flex"),
    )
});

#[derive(Debug, Deserialize)]
struct Chunk {
    id: String,
    #[allow(dead_code)]
    chapter: String,
    text: String,
}

/// tysm structured-output response for one translated chunk.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ChunkTranslation {
    /// The full translation of the excerpt, preserving paragraph breaks.
    translation: String,
}

/// Deterministic sentence splitter for Chinese: split after runs of 。！？…
/// (carrying along any closing quotes/brackets) and at line breaks. Lossless —
/// the concatenation of the output is the input minus whitespace.
fn segment_chinese(text: &str) -> Vec<String> {
    const TERMINALS: [char; 4] = ['。', '！', '？', '…'];
    const CLOSERS: [char; 5] = ['”', '』', '」', '）', '"'];
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut flush = |current: &mut String| {
        let sentence = current.trim();
        if !sentence.is_empty() {
            sentences.push(sentence.to_string());
        }
        current.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            flush(&mut current);
            continue;
        }
        current.push(c);
        if TERMINALS.contains(&c) {
            while chars
                .peek()
                .is_some_and(|c| TERMINALS.contains(c) || CLOSERS.contains(c))
            {
                current.push(chars.next().unwrap());
            }
            flush(&mut current);
        }
    }
    flush(&mut current);
    sentences
}

fn arg_value(name: &str) -> Option<String> {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let chunks_path = arg_value("--chunks").context("--chunks <chunks.jsonl> is required")?;
    let book = arg_value("--book").context("--book <slug> is required")?;
    let series = arg_value("--series").context("--series <slug> is required")?;
    let take: usize = arg_value("--take")
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(300);
    // Restrict the run to a single output code (e.g. `--only zho-hans`); others are skipped.
    let only = arg_value("--only");
    // The language the chunks are already written in — skips translation for that
    // output code and translates "from" it for the others.
    let source_lang = arg_value("--source-lang").unwrap_or_else(|| "eng".to_string());
    let (_, source_lang_name, _) = LANGS
        .iter()
        .find(|(code, _, _)| *code == source_lang)
        .with_context(|| format!("unknown --source-lang {source_lang}"))?;

    let all_chunks: Vec<Chunk> = std::fs::read_to_string(&chunks_path)
        .with_context(|| format!("read {chunks_path}"))?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    if all_chunks.is_empty() {
        bail!("no chunks in {chunks_path}");
    }
    // Evenly-spread selection across the whole book, so the sample isn't just the opening.
    let stride = (all_chunks.len() / take.min(all_chunks.len())).max(1);
    let selected: Vec<&Chunk> = all_chunks.iter().step_by(stride).take(take).collect();
    println!(
        "{} chunks in extraction; taking {} (stride {}) with model {}",
        all_chunks.len(),
        selected.len(),
        stride,
        *MODEL,
    );

    cache_remote::warm().await;
    let segmenter = RemoteClient::new(RemoteConfig::default())?;

    for (code, lang_name, seg_hint) in LANGS {
        if let Some(ref o) = only
            && o != code
        {
            continue;
        }
        let out_dir = PathBuf::from(format!(
            "./generate-data/data/{code}/sentence-sources/books/{series}"
        ));
        std::fs::create_dir_all(&out_dir)?;
        let out_path = out_dir.join(format!("{book}.jsonl"));

        // Incremental: skip chunks already segmented into the output file.
        let mut done_chunks = BTreeSet::new();
        if out_path.exists() {
            for line in std::fs::read_to_string(&out_path)?.lines() {
                if let Ok(rec) = serde_json::from_str::<BookSentence>(line) {
                    done_chunks.insert(rec.chunk_id);
                }
            }
        }
        let todo: Vec<&&Chunk> = selected
            .iter()
            .filter(|c| !done_chunks.contains(&c.id))
            .collect();
        println!(
            "[{code}] {} chunks to process ({} already done)",
            todo.len(),
            done_chunks.len()
        );
        if todo.is_empty() {
            continue;
        }

        let out_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out_path)?;
        let mut writer = std::io::BufWriter::new(out_file);

        let system_prompt = format!(
            "Translate this excerpt from a fantasy novel written in {source_lang_name} into \
             natural, fluent {lang_name}. Preserve the paragraph breaks. Keep names as a \
             native translation would (transliterate where that is the convention). Return \
             only the translation.",
        );

        let mut results = futures::stream::iter(todo)
            .map(|chunk| {
                let system_prompt = system_prompt.clone();
                let segmenter = &segmenter;
                let source_lang = source_lang.clone();
                async move {
                    let text = if code == source_lang {
                        chunk.text.clone()
                    } else {
                        let response: ChunkTranslation = CHAT_CLIENT
                            .chat_with_system_prompt(system_prompt, chunk.text.clone())
                            .await
                            .with_context(|| format!("translate chunk {}", chunk.id))?;
                        response.translation
                    };
                    let sentences = match seg_hint {
                        Some(l) => segmenter
                            .segment_sentences_in(&text, l)
                            .await
                            .with_context(|| format!("segment chunk {}", chunk.id))?,
                        None => segment_chinese(&text),
                    };
                    anyhow::Ok((chunk.id.clone(), sentences))
                }
            })
            .buffer_unordered(16);

        let mut n_sent = 0usize;
        let mut n_chunks = 0usize;
        let mut n_failed = 0usize;
        while let Some(result) = results.next().await {
            match result {
                Ok((chunk_id, sentences)) => {
                    for sentence in sentences {
                        let rec = BookSentence {
                            sentence,
                            book: book.clone(),
                            chunk_id: chunk_id.clone(),
                        };
                        writeln!(writer, "{}", serde_json::to_string(&rec)?)?;
                        n_sent += 1;
                    }
                    n_chunks += 1;
                    if n_chunks.is_multiple_of(25) {
                        writer.flush()?;
                        println!(
                            "[{code}] {n_chunks} chunks -> {n_sent} sentences (${:.2})",
                            CHAT_CLIENT.cost().unwrap_or(0.0)
                        );
                    }
                }
                Err(e) => {
                    n_failed += 1;
                    if n_failed <= 3 {
                        eprintln!("[{code}] {e:#}");
                    }
                }
            }
        }
        writer.flush()?;
        println!("[{code}] done: {n_chunks} chunks -> {n_sent} sentences, {n_failed} failed");
    }

    cache_remote::flush().await;
    println!(
        "total LLM cost this run: ${:.2}",
        CHAT_CLIENT.cost().unwrap_or(0.0)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::segment_chinese;

    #[test]
    fn splits_on_terminals_and_carries_closers() {
        let text = "“方源，交出春秋蝉！”他大喝。这里早已布下天罗大网……\n\n在他看来——\n\n无非是早死、晚死的区别罢了。";
        assert_eq!(
            segment_chinese(text),
            vec![
                "“方源，交出春秋蝉！”",
                "他大喝。",
                "这里早已布下天罗大网……",
                "在他看来——",
                "无非是早死、晚死的区别罢了。",
            ]
        );
    }

    #[test]
    fn lossless_modulo_whitespace() {
        let text = "第一段。没有终结标点的段尾\n\n“对话！？”……第二段。";
        let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(strip(&segment_chinese(text).concat()), strip(text));
    }
}
