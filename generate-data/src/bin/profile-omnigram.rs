use generate_data::nlp::convert_tokens_to_literals;
use language_utils::{Atom, Language, literals_to_atoms};
use omnigram::unigram::{UnigramTrainer, UnigramTrainerConfig};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::File;
use std::hint::black_box;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Dedicated one-shot profiling target for omnigram training.
///
/// Typical usage:
/// `samply record -- cargo run --profile profiling -p generate-data --bin profile-omnigram`
///
/// Optional flags:
/// `--sentences 10000`
/// `--max-piece-length 8`
/// `--merge-alpha 0.0`
/// `--initial-candidate-multiplier 4`
/// `--min-frequency 3`
/// `--em-iterations 10`
/// `--shrinking-factor 0.75`
/// `--path out/fra/target_language_sentences_tokenization.jsonl`
#[derive(Debug, Clone)]
struct Args {
    path: PathBuf,
    sentence_limit: Option<usize>,
    max_piece_length: usize,
    merge_alpha: f64,
    initial_candidate_multiplier: usize,
    min_frequency: u32,
    em_iterations: usize,
    shrinking_factor: f64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            path: default_corpus_path(),
            sentence_limit: None,
            max_piece_length: 8,
            merge_alpha: 0.0,
            initial_candidate_multiplier: 4,
            min_frequency: 3,
            em_iterations: 10,
            shrinking_factor: 0.75,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenizedSentence {
    sentence: String,
    tokens: Vec<lexide::Token>,
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
        let value = iter.next().unwrap_or_else(|| {
            panic!(
                "Missing value for flag `{flag}`.\n\
                 Usage: profile-omnigram [--path FILE] [--sentences N] [--max-piece-length N] \
                 [--merge-alpha F] [--initial-candidate-multiplier N] [--min-frequency N] \
                 [--em-iterations N] [--shrinking-factor F]"
            )
        });

        match flag.as_str() {
            "--path" => args.path = PathBuf::from(value),
            "--sentences" => {
                args.sentence_limit = match value.as_str() {
                    "all" => None,
                    _ => Some(
                        value
                            .parse()
                            .unwrap_or_else(|_| panic!("Invalid `--sentences` value: {value}")),
                    ),
                };
            }
            "--max-piece-length" => {
                args.max_piece_length = value
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid `--max-piece-length` value: {value}"));
            }
            "--merge-alpha" => {
                args.merge_alpha = value
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid `--merge-alpha` value: {value}"));
            }
            "--initial-candidate-multiplier" => {
                args.initial_candidate_multiplier = value.parse().unwrap_or_else(|_| {
                    panic!("Invalid `--initial-candidate-multiplier` value: {value}")
                });
            }
            "--min-frequency" => {
                args.min_frequency = value
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid `--min-frequency` value: {value}"));
            }
            "--em-iterations" => {
                args.em_iterations = value
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid `--em-iterations` value: {value}"));
            }
            "--shrinking-factor" => {
                args.shrinking_factor = value
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid `--shrinking-factor` value: {value}"));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: profile-omnigram [--path FILE] [--sentences N|all] \
                     [--max-piece-length N] [--merge-alpha F] \
                     [--initial-candidate-multiplier N] [--min-frequency N] \
                     [--em-iterations N] [--shrinking-factor F]"
                );
                std::process::exit(0);
            }
            _ => panic!("Unknown flag `{flag}`"),
        }
    }

    args
}

fn load_french_corpus_from_tokenization_jsonl(
    path: &Path,
    sentence_limit: Option<usize>,
) -> Vec<Vec<Atom<String>>> {
    let file = File::open(path).unwrap_or_else(|err| {
        panic!(
            "Failed to open tokenization file at `{}`: {err}",
            path.display()
        )
    });
    let reader = BufReader::new(file);

    let tokenizations = reader
        .lines()
        .take(sentence_limit.unwrap_or(usize::MAX))
        .map(|line| {
            let line = line.unwrap();
            let parsed: TokenizedSentence = serde_json::from_str(&line).unwrap();
            (parsed.sentence, parsed.tokens)
        })
        .collect();

    let literals = convert_tokens_to_literals(&tokenizations, Language::French);
    literals
        .values()
        .map(|words| {
            let (atoms, _) = literals_to_atoms(words, Language::French);
            atoms
        })
        .collect()
}

fn main() {
    let args = parse_args();
    let corpus = load_french_corpus_from_tokenization_jsonl(&args.path, args.sentence_limit);
    let unique_atoms: HashSet<_> = corpus
        .iter()
        .flat_map(|sentence| sentence.iter().cloned())
        .collect();
    let target_multiword_tokens = (unique_atoms.len() * 33) / 100;

    println!(
        "Profiling omnigram training\n  path={}\n  sentences={}\n  unique_atoms={}\n  target_multiword_tokens={}\n  max_piece_length={}\n  merge_alpha={}\n  initial_candidate_multiplier={}\n  min_frequency={}\n  em_iterations={}\n  shrinking_factor={}",
        args.path.display(),
        corpus.len(),
        unique_atoms.len(),
        target_multiword_tokens,
        args.max_piece_length,
        args.merge_alpha,
        args.initial_candidate_multiplier,
        args.min_frequency,
        args.em_iterations,
        args.shrinking_factor
    );

    let config = UnigramTrainerConfig {
        target_multiword_tokens,
        max_piece_length: args.max_piece_length,
        shrinking_factor: args.shrinking_factor,
        min_frequency: args.min_frequency,
        em_iterations: args.em_iterations,
        initial_candidate_multiplier: args.initial_candidate_multiplier,
        merge_alpha: args.merge_alpha,
    };

    let trainer = UnigramTrainer::new(config);
    let start = Instant::now();
    let model = trainer.train(&corpus, &[]);
    let elapsed = start.elapsed();

    let multiword_vocab = model
        .get_vocab()
        .iter()
        .filter(|seq| seq.len() >= 2)
        .count();
    black_box(model.vocab_size());

    println!(
        "Finished training in {:.2?}\n  vocab_size={}\n  multiword_vocab_size={}",
        elapsed,
        model.vocab_size(),
        multiword_vocab
    );
}
