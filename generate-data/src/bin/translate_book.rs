//! Translate an English book's extracted chunks into the course languages and segment
//! them into sentences — producing `data/<lang>/sentence-sources/books/<series>/<book>.jsonl`
//! (the source consumed by `generate_data::books`). Remember to add the book to the
//! series' `metadata.jsonl` in the same folder.
//!
//!     cargo run -p generate-data --release --bin translate_book -- \
//!         --chunks /data/books/pale-lights-extracted/chunks.jsonl \
//!         --series pale-lights --book pale-lights \
//!         [--take 300] [--model gpt-5.6-luna]
//!
//! Run from the repo root (paths and `.cache/` are root-relative). Translations go
//! through tysm (cached in `.cache/`, synced by osmo); segmentation goes through the
//! parsley `/segment` serve with a language hint. Incremental: chunks already present in
//! an output file are skipped, so reruns only fill gaps. English skips translation and
//! segments the original text.

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

const LANGS: [(&str, Language); 10] = [
    ("deu", Language::German),
    ("eng", Language::English),
    ("fra", Language::French),
    ("hin", Language::Hindi),
    ("ita", Language::Italian),
    ("jpn", Language::Japanese),
    ("kor", Language::Korean),
    ("por", Language::Portuguese),
    ("rus", Language::Russian),
    ("spa", Language::Spanish),
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

    for (code, language) in LANGS {
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
            "Translate this excerpt from an English fantasy novel into natural, fluent {}. \
             Preserve the paragraph breaks. Keep names as a native translation would \
             (transliterate where that is the convention). Return only the translation.",
            language_name(language)
        );

        let mut results = futures::stream::iter(todo)
            .map(|chunk| {
                let system_prompt = system_prompt.clone();
                let segmenter = &segmenter;
                async move {
                    let text = if code == "eng" {
                        chunk.text.clone()
                    } else {
                        let response: ChunkTranslation = CHAT_CLIENT
                            .chat_with_system_prompt(system_prompt, chunk.text.clone())
                            .await
                            .with_context(|| format!("translate chunk {}", chunk.id))?;
                        response.translation
                    };
                    let sentences = segmenter
                        .segment_sentences_in(&text, language)
                        .await
                        .with_context(|| format!("segment chunk {}", chunk.id))?;
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

fn language_name(language: Language) -> &'static str {
    match language {
        Language::German => "German",
        Language::English => "English",
        Language::French => "French",
        Language::Hindi => "Hindi",
        Language::Italian => "Italian",
        Language::Japanese => "Japanese",
        Language::Korean => "Korean",
        Language::Portuguese => "Portuguese",
        Language::Russian => "Russian",
        Language::Spanish => "Spanish",
    }
}
