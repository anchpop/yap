//! Stats test for omnigram tokenization on real French data.
//!
//! Run with: cargo test -p generate-data --release --test tokenization_stats -- --nocapture

use generate_data::nlp::convert_tokens_to_literals;
use language_utils::{Atom, Language, Literal, literals_to_atoms};
use omnigram::unigram::{Seq, UnigramModel, UnigramTrainer, UnigramTrainerConfig};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Intern a corpus of `Atom<String>` sentences, returning the interned corpus and a reader.
fn intern_corpus(
    corpus: &[Vec<Atom<String>>],
) -> (Vec<Vec<Atom<lasso::Spur>>>, lasso::RodeoReader) {
    let mut rodeo = lasso::Rodeo::new();
    let interned = corpus
        .iter()
        .map(|sentence| {
            sentence
                .iter()
                .map(|a| a.get_or_intern(&mut rodeo))
                .collect()
        })
        .collect();
    (interned, rodeo.into_reader())
}

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
        if words_val.as_array().is_none_or(|a| a.is_empty()) {
            continue;
        }

        let words: Vec<Literal<String>> = serde_json::from_value(words_val.clone()).unwrap();
        let (atoms, _) = literals_to_atoms(&words, language);
        corpus.push(atoms);
    }

    corpus
}

#[derive(Debug, Deserialize)]
struct TokenizedSentence {
    sentence: String,
    tokens: Vec<lexide::Token>,
}

fn load_french_corpus_from_tokenization_jsonl() -> Vec<Vec<Atom<String>>> {
    let path = "../out/fra/target_language_sentences_tokenization.jsonl";
    let file = File::open(path).expect("Failed to open tokenization file");
    let reader = BufReader::new(file);

    let tokenizations = reader
        .lines()
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

fn eval_model(
    model: &UnigramModel<Atom<lasso::Spur>>,
    corpus: &[Vec<Atom<lasso::Spur>>],
    label: &str,
) {
    let mut total_tokens = 0usize;
    let mut total_atoms_expanded = 0usize;
    let mut multi_atom_usages = 0usize;
    let mut token_counts = Vec::with_capacity(corpus.len());

    for atoms in corpus {
        let segments = model
            .segment(atoms)
            .expect("every training-corpus atom has a single-token vocabulary entry");
        let n_tokens = segments.len();
        token_counts.push(n_tokens);
        total_tokens += n_tokens;

        for seq in &segments {
            let atom_count = seq.len();
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

    println!(
        "  {label}: vocab {vocab_single}+{vocab_multi}={} | avg tok/sent {avg_tokens:.1} | avg atom/sent {avg_atoms:.1} | compression {compression:.2}x | multi {multi_pct:.1}% | median tok/sent {median}",
        vocab_single + vocab_multi
    );
}

fn print_top_multigrams(
    model: &UnigramModel<Atom<lasso::Spur>>,
    reader: &lasso::RodeoReader,
    language: Language,
    limit: usize,
) {
    println!("  top multigrams:");
    for (seq, count) in model
        .get_vocab_with_counts()
        .into_iter()
        .filter(|(seq, count)| seq.len() > 1 && *count > 0)
        .take(limit)
    {
        let gram = language_utils::Gram(seq.0.iter().map(|a| a.resolve(reader)).collect());
        println!(
            "    {} ({count})",
            gram.to_display_string(language).replace('\n', "\\n")
        );
    }
}

fn collect_used_multigrams(
    model: &UnigramModel<Atom<lasso::Spur>>,
) -> Vec<(Seq<Atom<lasso::Spur>>, u32)> {
    model
        .get_vocab_with_counts()
        .into_iter()
        .filter(|(seq, count)| seq.len() > 1 && *count > 0)
        .map(|(seq, count)| (seq.clone(), count))
        .collect()
}

fn print_multigram_list(
    label: &str,
    items: &[(Seq<Atom<lasso::Spur>>, u32)],
    len: usize,
    reader: &lasso::RodeoReader,
    language: Language,
) {
    println!("  {label}:");
    for (idx, (seq, count)) in items.iter().enumerate().take(len) {
        let gram = language_utils::Gram(seq.0.iter().map(|a| a.resolve(reader)).collect());
        println!(
            "    #{idx}: {} ({count})",
            gram.to_display_string(language).replace('\n', "\\n")
        );
    }
}

fn raw_count_fixed_bigram_model(
    corpus: &[Vec<Atom<lasso::Spur>>],
    target_multiword_tokens: usize,
    merge_alpha: f64,
) -> UnigramModel<Atom<lasso::Spur>> {
    let mut unigram_counts: HashMap<Seq<Atom<lasso::Spur>>, u32> = HashMap::new();
    let mut bigram_counts: HashMap<Seq<Atom<lasso::Spur>>, u32> = HashMap::new();

    for sentence in corpus {
        for atom in sentence {
            *unigram_counts.entry(Seq(vec![*atom])).or_insert(0) += 1;
        }

        for pair in sentence.windows(2) {
            let seq = Seq(pair.to_vec());
            if !seq.iter().all(|atom| {
                matches!(atom, Atom::Tok(word) if matches!(word.word_type, language_utils::WordType::Heteronym(_)))
            }) {
                continue;
            }
            *bigram_counts.entry(seq).or_insert(0) += 1;
        }
    }

    let mut top_bigrams: Vec<(Seq<Atom<lasso::Spur>>, u32)> = bigram_counts.into_iter().collect();
    top_bigrams.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.disambiguation_key().cmp(&b.0.disambiguation_key()))
    });
    top_bigrams.truncate(target_multiword_tokens);

    let total_count: u64 = unigram_counts.values().map(|&c| c as u64).sum::<u64>()
        + top_bigrams.iter().map(|(_, c)| *c as u64).sum::<u64>();

    let mut vocab_with_scores = Vec::with_capacity(unigram_counts.len() + top_bigrams.len());
    for (seq, count) in unigram_counts {
        let log_prob = (count as f64 / total_count as f64).ln();
        vocab_with_scores.push((seq, log_prob, count));
    }
    for (seq, count) in top_bigrams {
        let log_prob = (count as f64 / total_count as f64).ln();
        vocab_with_scores.push((seq, log_prob, count));
    }

    UnigramModel::new(vocab_with_scores, merge_alpha)
}

fn run_production_path_french_stats(
    label: &str,
    max_piece_length: usize,
    merge_alpha: f64,
    initial_candidate_multiplier: usize,
) {
    let string_corpus = load_french_corpus_from_tokenization_jsonl();
    println!(
        "Loaded {} sentences from tokenization jsonl",
        string_corpus.len()
    );

    let (corpus, reader) = intern_corpus(&string_corpus);

    let unique_atoms: HashSet<_> = corpus.iter().flat_map(|s| s.iter().cloned()).collect();
    let single_atom_count = unique_atoms.len();
    println!("Unique atoms: {single_atom_count}");

    let target_multiword_tokens = (single_atom_count * 33) / 100;
    println!(
        "Training production-style corpus with target_multiword_tokens={target_multiword_tokens}, min_frequency=3, max_piece_length={max_piece_length}, merge_alpha={merge_alpha}, initial_candidate_multiplier={initial_candidate_multiplier}\n"
    );

    let config = UnigramTrainerConfig {
        target_multiword_tokens,
        max_piece_length,
        shrinking_factor: 0.75,
        min_frequency: 3,
        em_iterations: 10,
        initial_candidate_multiplier,
        merge_alpha,
    };

    let trainer = UnigramTrainer::new(config);
    let model = trainer.train(&corpus, &[]);
    eval_model(&model, &corpus, label);
    print_top_multigrams(&model, &reader, Language::French, 30);
}

#[test]
#[ignore]
fn tokenization_stats() {
    let string_corpus = load_french_corpus();
    println!("Loaded {} sentences", string_corpus.len());

    let (corpus, _reader) = intern_corpus(&string_corpus);

    let unique_atoms: HashSet<_> = corpus.iter().flat_map(|s| s.iter().cloned()).collect();
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
            initial_candidate_multiplier: 4,
            merge_alpha: alpha,
        };

        let trainer = UnigramTrainer::new(config);
        let model = trainer.train(&corpus, &[]);
        eval_model(&model, &corpus, &format!("alpha={alpha:.1}"));
    }
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_alpha_zero() {
    run_production_path_french_stats("production-path alpha=0.0", 8, 0.0, 4);
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_alpha_zero_max_len_3() {
    run_production_path_french_stats("production-path alpha=0.0 max_len=3", 3, 0.0, 4);
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_alpha_zero_max_len_2() {
    run_production_path_french_stats("production-path alpha=0.0 max_len=2", 2, 0.0, 4);
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_alpha_one() {
    run_production_path_french_stats("production-path alpha=1.0", 8, 1.0, 4);
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_alpha_zero_initial_multiplier_10() {
    run_production_path_french_stats(
        "production-path alpha=0.0 initial_multiplier=10",
        8,
        0.0,
        10,
    );
}

#[test]
#[ignore]
fn tokenization_stats_compare_initial_multiplier_4_vs_10_vs_15() {
    let string_corpus = load_french_corpus_from_tokenization_jsonl();
    println!(
        "Loaded {} sentences from tokenization jsonl",
        string_corpus.len()
    );

    let (corpus, reader) = intern_corpus(&string_corpus);

    let unique_atoms: HashSet<_> = corpus.iter().flat_map(|s| s.iter().cloned()).collect();
    let single_atom_count = unique_atoms.len();
    let target_multiword_tokens = (single_atom_count * 33) / 100;

    let make_model = |initial_candidate_multiplier| {
        let config = UnigramTrainerConfig {
            target_multiword_tokens,
            max_piece_length: 8,
            shrinking_factor: 0.75,
            min_frequency: 3,
            em_iterations: 10,
            initial_candidate_multiplier,
            merge_alpha: 0.0,
        };
        UnigramTrainer::new(config).train(&corpus, &[])
    };

    let model4 = make_model(4);
    let model10 = make_model(10);
    let model15 = make_model(15);

    eval_model(&model4, &corpus, "initial_multiplier=4");
    eval_model(&model10, &corpus, "initial_multiplier=10");
    eval_model(&model15, &corpus, "initial_multiplier=15");

    let used4 = collect_used_multigrams(&model4);
    let used10 = collect_used_multigrams(&model10);
    let used15 = collect_used_multigrams(&model15);

    let set4: HashSet<_> = used4.iter().map(|(seq, _)| seq.clone()).collect();
    let set10: HashSet<_> = used10.iter().map(|(seq, _)| seq.clone()).collect();
    let set15: HashSet<_> = used15.iter().map(|(seq, _)| seq.clone()).collect();

    let unique4: Vec<_> = used4
        .iter()
        .filter(|(seq, _)| !set10.contains(seq) && !set15.contains(seq))
        .cloned()
        .collect();
    let unique10: Vec<_> = used10
        .iter()
        .filter(|(seq, _)| !set4.contains(seq) && !set15.contains(seq))
        .cloned()
        .collect();
    let unique15: Vec<_> = used15
        .iter()
        .filter(|(seq, _)| !set4.contains(seq) && !set10.contains(seq))
        .cloned()
        .collect();

    println!(
        "  overlap all three: shared_4_10={} shared_10_15={} shared_4_15={}",
        set4.intersection(&set10).count(),
        set10.intersection(&set15).count(),
        set4.intersection(&set15).count()
    );
    println!(
        "  exclusive counts: only4={} only10={} only15={}",
        unique4.len(),
        unique10.len(),
        unique15.len()
    );

    print_multigram_list(
        "most common only in 4",
        &unique4,
        50,
        &reader,
        Language::French,
    );
    print_multigram_list(
        "most common only in 10",
        &unique10,
        50,
        &reader,
        Language::French,
    );
    print_multigram_list(
        "most common only in 15",
        &unique15,
        50,
        &reader,
        Language::French,
    );
}

#[test]
#[ignore]
fn tokenization_stats_from_tokenization_jsonl_top_raw_bigrams_alpha_one() {
    let string_corpus = load_french_corpus_from_tokenization_jsonl();
    println!(
        "Loaded {} sentences from tokenization jsonl",
        string_corpus.len()
    );

    let (corpus, reader) = intern_corpus(&string_corpus);

    let unique_atoms: HashSet<_> = corpus.iter().flat_map(|s| s.iter().cloned()).collect();
    let single_atom_count = unique_atoms.len();
    println!("Unique atoms: {single_atom_count}");

    let target_multiword_tokens = (single_atom_count * 33) / 100;
    println!(
        "Building fixed raw-count bigram vocab with target_multiword_tokens={target_multiword_tokens}, alpha=1.0\n"
    );

    let model = raw_count_fixed_bigram_model(&corpus, target_multiword_tokens, 1.0);
    eval_model(
        &model,
        &corpus,
        "production-path fixed top bigrams alpha=1.0",
    );
    print_top_multigrams(&model, &reader, Language::French, 30);
}
