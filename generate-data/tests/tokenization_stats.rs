//! Stats test for omnigram tokenization on real French data.
//!
//! Run with: cargo test -p generate-data --release --test tokenization_stats -- --nocapture

use language_utils::{Atom, Language, Literal, literals_to_atoms};
use omnigram::SuperToken;
use omnigram::unigram::{UnigramTrainer, UnigramTrainerConfig};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn load_french_corpus() -> Vec<Vec<Atom<String>>> {
    let path = "../out/fra/target_language_sentences_nlp.jsonl";
    let file = File::open(path).expect("Failed to open NLP file - run generate-data first");
    let reader = BufReader::new(file);

    let language = Language::French;
    let mut corpus = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let (_, info): (String, serde_json::Value) = serde_json::from_str(&line).unwrap();

        let words_val = &info["words"];
        if words_val.as_array().map_or(true, |a| a.is_empty()) {
            continue;
        }

        let words: Vec<Literal<String>> =
            serde_json::from_value(words_val.clone()).unwrap();
        let (atoms, _) = literals_to_atoms(&words, language);
        corpus.push(atoms);
    }

    corpus
}

fn eval_model(model: &omnigram::unigram::UnigramModel, corpus: &[Vec<Atom<String>>], alpha: f64) {
    let mut total_tokens = 0usize;
    let mut total_atoms_expanded = 0usize;
    let mut multi_atom_usages = 0usize;
    let mut token_counts = Vec::with_capacity(corpus.len());

    for atoms in corpus {
        let supertokens = model.segment(atoms);
        let n_tokens = supertokens.len();
        token_counts.push(n_tokens);
        total_tokens += n_tokens;

        for st in &supertokens {
            let atom_count = match st {
                SuperToken::Base(_) => 1,
                SuperToken::Merged(m) => 2 + m.middle.len(),
            };
            total_atoms_expanded += atom_count;
            if atom_count > 1 {
                multi_atom_usages += 1;
            }
        }
    }

    let n = corpus.len() as f64;
    let avg_tokens = total_tokens as f64 / n;
    let avg_atoms = total_atoms_expanded as f64 / n;
    let compression = total_atoms_expanded as f64 / total_tokens as f64;
    let multi_pct = 100.0 * multi_atom_usages as f64 / total_tokens as f64;

    let mut vocab_single = 0usize;
    let mut vocab_multi = 0usize;
    for (seq, _) in model.get_vocab_with_counts() {
        if seq.0.len() > 1 {
            vocab_multi += 1;
        } else {
            vocab_single += 1;
        }
    }

    token_counts.sort();
    let median = token_counts[token_counts.len() / 2];

    println!("  alpha={alpha:.1}: vocab {vocab_single}+{vocab_multi}={} | avg tok/sent {avg_tokens:.1} | avg atom/sent {avg_atoms:.1} | compression {compression:.2}x | multi {multi_pct:.1}% | median tok/sent {median}",
        vocab_single + vocab_multi);
}

#[test]
fn tokenization_stats() {
    let corpus = load_french_corpus();
    println!("Loaded {} sentences", corpus.len());

    let unique_atoms: HashSet<_> = corpus
        .iter()
        .flat_map(|s| s.iter().cloned())
        .collect();
    let single_atom_count = unique_atoms.len();
    println!("Unique atoms: {single_atom_count}");

    let target_multiword_tokens = (single_atom_count * 33) / 100;
    println!("Training with target_multiword_tokens={target_multiword_tokens}, min_frequency=3\n");

    for alpha in [0.0, 0.25, 0.5, 0.75] {
        let config = UnigramTrainerConfig {
            target_multiword_tokens,
            max_piece_length: 8,
            shrinking_factor: 0.75,
            min_frequency: 3,
            em_iterations: 10,
            merge_alpha: alpha,
        };

        let trainer = UnigramTrainer::new(config);
        let model = trainer.train(&corpus, &[]);
        eval_model(&model, &corpus, alpha);
    }
}
