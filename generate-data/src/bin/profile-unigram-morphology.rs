use omnigram::PChar;
use omnigram::unigram::{UnigramModel, UnigramTrainer, UnigramTrainerConfig};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Character-level unigram morphology extraction.
///
/// Trains a unigram model where:
/// - Each "sentence" is a single word (sequence of unicode characters)
/// - The model learns to segment words into morphologically meaningful subword units
///
/// Usage:
/// `cargo run --release -p generate-data --bin profile-unigram-morphology`
///
/// Optional flags:
/// `--sentences 10000`         (limit number of sentences to extract words from)
/// `--target-pieces 500`       (target number of multi-char pieces to learn)
/// `--max-piece-length 12`     (max chars in a learned piece)
/// `--min-frequency 5`         (min word frequency to include)
/// `--merge-alpha 0.0`         (blend: 0=unigram, 1=pathpiece)
/// `--em-iterations 10`
/// `--shrinking-factor 0.75`
/// `--top 50`                  (show top N vocab entries)
/// `--segment WORD`            (segment a specific word)
/// `--path FILE`               (corpus path)
#[derive(Debug, Clone)]
struct Args {
    path: PathBuf,
    sentence_limit: Option<usize>,
    target_pieces: usize,
    max_piece_length: usize,
    min_frequency: u32,
    merge_alpha: f64,
    em_iterations: usize,
    shrinking_factor: f64,
    initial_candidate_multiplier: usize,
    top: usize,
    segment_words: Vec<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            path: default_corpus_path(),
            sentence_limit: None,
            target_pieces: 500,
            max_piece_length: 12,
            min_frequency: 5,
            merge_alpha: 0.0,
            em_iterations: 10,
            shrinking_factor: 0.75,
            initial_candidate_multiplier: 4,
            top: 50,
            segment_words: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenizedSentence {
    tokens: Vec<Token>,
}

#[derive(Debug, Deserialize)]
struct Token {
    text: TokenText,
    pos: String,
}

#[derive(Debug, Deserialize)]
struct TokenText {
    text: String,
}

fn default_corpus_path() -> PathBuf {
    let workspace_relative = PathBuf::from("out/fra/target_language_sentences_tokenization.jsonl");
    if workspace_relative.exists() {
        return workspace_relative;
    }

    let crate_relative = PathBuf::from("../out/fra/target_language_sentences_tokenization.jsonl");
    if crate_relative.exists() {
        return crate_relative;
    }

    workspace_relative
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);

    while let Some(flag) = iter.next() {
        let mut value = || {
            iter.next().unwrap_or_else(|| {
                panic!("Missing value for flag `{flag}`");
            })
        };

        match flag.as_str() {
            "--path" => args.path = PathBuf::from(value()),
            "--sentences" => {
                let v = value();
                args.sentence_limit = match v.as_str() {
                    "all" => None,
                    _ => Some(
                        v.parse()
                            .unwrap_or_else(|_| panic!("Invalid --sentences: {v}")),
                    ),
                };
            }
            "--target-pieces" => {
                args.target_pieces = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --target-pieces: {e}"));
            }
            "--max-piece-length" => {
                args.max_piece_length = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --max-piece-length: {e}"));
            }
            "--min-frequency" => {
                args.min_frequency = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --min-frequency: {e}"));
            }
            "--merge-alpha" => {
                args.merge_alpha = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --merge-alpha: {e}"));
            }
            "--em-iterations" => {
                args.em_iterations = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --em-iterations: {e}"));
            }
            "--shrinking-factor" => {
                args.shrinking_factor = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --shrinking-factor: {e}"));
            }
            "--initial-candidate-multiplier" => {
                args.initial_candidate_multiplier = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --initial-candidate-multiplier: {e}"));
            }
            "--top" => {
                args.top = value()
                    .parse()
                    .unwrap_or_else(|e| panic!("Invalid --top: {e}"));
            }
            "--segment" => {
                args.segment_words.push(value());
            }
            "--help" | "-h" => {
                println!(
                    "Usage: profile-unigram-morphology [OPTIONS]\n\n\
                     Options:\n\
                     --path FILE                        Corpus JSONL path\n\
                     --sentences N|all                  Limit sentences to read\n\
                     --target-pieces N                  Target multi-char pieces (default: 500)\n\
                     --max-piece-length N               Max chars per piece (default: 12)\n\
                     --min-frequency N                  Min word frequency (default: 5)\n\
                     --merge-alpha F                    Blend factor (default: 0.0)\n\
                     --em-iterations N                  EM iterations (default: 10)\n\
                     --shrinking-factor F               Vocab pruning factor (default: 0.75)\n\
                     --initial-candidate-multiplier N   Candidate budget multiplier (default: 4)\n\
                     --top N                            Show top N vocab entries (default: 50)\n\
                     --segment WORD                     Segment a word (repeatable)"
                );
                std::process::exit(0);
            }
            _ => panic!("Unknown flag `{flag}`"),
        }
    }

    args
}

/// Extract unique lowercased words from the corpus, with frequency counts.
/// Only includes words with alphabetic POS tags (not PUNCT, NUM, etc.)
fn extract_words(path: &Path, sentence_limit: Option<usize>) -> HashMap<String, u32> {
    let file = File::open(path).unwrap_or_else(|err| {
        panic!("Failed to open {}: {err}", path.display());
    });
    let reader = BufReader::new(file);

    let mut word_counts: HashMap<String, u32> = HashMap::new();

    for line in reader.lines().take(sentence_limit.unwrap_or(usize::MAX)) {
        let line = line.unwrap();
        let parsed: TokenizedSentence = serde_json::from_str(&line).unwrap();

        for token in &parsed.tokens {
            // Skip punctuation, numbers, symbols
            match token.pos.as_str() {
                "PUNCT" | "NUM" | "SYM" | "X" | "SPACE" => continue,
                _ => {}
            }

            let word = token.text.text.to_lowercase();

            // Skip very short words (single char) and words with non-alphabetic chars
            if word.len() < 2
                || !word
                    .chars()
                    .all(|c| c.is_alphabetic() || c == '-' || c == '\'')
            {
                continue;
            }

            *word_counts.entry(word).or_insert(0) += 1;
        }
    }

    word_counts
}

/// Render a PChar sequence as text with ^/$ markers on the whole piece.
/// e.g. [p^, r] → "^pr", [e, r$] → "er$", [d^, e$] → "^de$"
fn render_piece(seq: &[PChar]) -> String {
    use omnigram::CharPosition;
    let text: String = seq.iter().map(|pc| pc.ch).collect();
    let starts = seq.first().is_some_and(|pc| pc.pos == CharPosition::Start);
    let ends = seq.last().is_some_and(|pc| pc.pos == CharPosition::End);
    match (starts, ends) {
        (true, true) => format!("^{text}$"),
        (true, false) => format!("^{text}"),
        (false, true) => format!("{text}$"),
        (false, false) => text,
    }
}

fn display_segmentation(model: &UnigramModel<PChar>, word: &str) {
    let pchars = PChar::from_word(word);
    let Some(segments) = model.segment(&pchars) else {
        println!("  {word}: not segmentable (contains a character the model never saw)");
        return;
    };
    let pieces: Vec<String> = segments
        .iter()
        .map(|seq| PChar::seq_to_string(&seq.0))
        .collect();
    println!("  {word} → {}", pieces.join(" | "));
}

fn main() {
    let args = parse_args();

    println!("Extracting words from {}...", args.path.display());
    let word_counts = extract_words(&args.path, args.sentence_limit);
    println!("  unique words: {}", word_counts.len());

    // Build corpus: each word becomes a "sentence" of positioned characters.
    // Repeat each word according to its frequency so the unigram model
    // learns that common morphemes are common.
    let corpus: Vec<Vec<PChar>> = word_counts
        .iter()
        .filter(|&(_, count)| *count >= args.min_frequency)
        .flat_map(|(word, &count)| {
            let pchars = PChar::from_word(word);
            std::iter::repeat_n(pchars, count as usize)
        })
        .collect();

    let unique_pchars: std::collections::HashSet<PChar> =
        corpus.iter().flat_map(|w| w.iter().cloned()).collect();

    println!(
        "  corpus words (after freq filter): {} ({} total instances)",
        word_counts
            .values()
            .filter(|&&c| c >= args.min_frequency)
            .count(),
        corpus.len()
    );
    println!("  unique positioned chars: {}", unique_pchars.len());

    let config = UnigramTrainerConfig {
        target_multiword_tokens: args.target_pieces,
        max_piece_length: args.max_piece_length,
        shrinking_factor: args.shrinking_factor,
        min_frequency: args.min_frequency,
        em_iterations: args.em_iterations,
        initial_candidate_multiplier: args.initial_candidate_multiplier,
        merge_alpha: args.merge_alpha,
    };

    println!(
        "\nTraining character-level unigram model...\n  \
         target_pieces={}\n  max_piece_length={}\n  merge_alpha={}\n  \
         em_iterations={}\n  shrinking_factor={}",
        args.target_pieces,
        args.max_piece_length,
        args.merge_alpha,
        args.em_iterations,
        args.shrinking_factor
    );

    let start = Instant::now();
    let trainer = UnigramTrainer::new(config);
    let model = trainer.train(&corpus, &[]);
    let elapsed = start.elapsed();

    let multi_char_count = model.get_vocab().iter().filter(|s| s.len() >= 2).count();
    println!(
        "\nTrained in {elapsed:.2?}\n  vocab_size: {}\n  multi-char pieces: {}",
        model.vocab_size(),
        multi_char_count
    );

    // Display top vocabulary entries
    println!("\nTop {} vocab entries (by usage count):", args.top);
    let vocab_with_counts = model.get_vocab_with_counts();
    for (seq, count) in vocab_with_counts.iter().take(args.top) {
        let rendered = render_piece(&seq.0);
        let len_indicator = if seq.len() >= 2 {
            format!(" ({})", seq.len())
        } else {
            String::new()
        };
        println!("  {count:>8}  {rendered}{len_indicator}");
    }

    // Show multi-char pieces sorted by count
    let multi_char: Vec<_> = vocab_with_counts
        .iter()
        .filter(|(seq, _)| seq.len() >= 2)
        .collect();
    println!(
        "\nTop {} multi-char pieces (morphological units):",
        args.top.min(multi_char.len())
    );
    for (seq, count) in multi_char.iter().take(args.top) {
        let rendered = render_piece(&seq.0);
        println!("  {count:>8}  {rendered}");
    }

    // Segment requested words
    if !args.segment_words.is_empty() {
        println!("\nSegmentations:");
        for word in &args.segment_words {
            display_segmentation(&model, word);
        }
    }

    // Always segment some example words to show what the model learned
    let example_words = [
        "destabiliser",
        "impossible",
        "reconstruction",
        "malheureux",
        "incompréhensible",
        "déménagement",
        "heureusement",
        "irrégulièrement",
        "encouragement",
        "décourageant",
        "anticonstitutionnellement",
        "inacceptable",
        "redécouvrir",
        "préhistorique",
        "transformation",
    ];
    println!("\nExample segmentations:");
    for word in &example_words {
        display_segmentation(&model, word);
    }
}
