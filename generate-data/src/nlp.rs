use anyhow::{Context, Result};
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Gram, Language};
use lexide::Lexide;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Convert language_utils::Language to lexide::Language
/// Returns None if the language is not supported by lexide
fn to_lexide_language(lang: Language) -> Option<lexide::Language> {
    match lang {
        Language::French => Some(lexide::Language::French),
        Language::English => Some(lexide::Language::English),
        Language::Spanish => Some(lexide::Language::Spanish),
        Language::Korean => Some(lexide::Language::Korean),
        Language::German => Some(lexide::Language::German),
        Language::Italian => Some(lexide::Language::Italian),
        Language::Portuguese => Some(lexide::Language::Portuguese),
        Language::Russian => Some(lexide::Language::Russian),
        Language::Hindi => Some(lexide::Language::Hindi),
        Language::Japanese => Some(lexide::Language::Japanese),
        // Languages not yet supported by lexide
        Language::ChineseSimplified | Language::ChineseTraditional | Language::Thai => todo!(),
    }
}

/// Tokenized sentence for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenizedSentence {
    sentence: String,
    tokens: Vec<lexide::Token>,
}

/// Track sentences that have failed tokenization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FailureRecord {
    sentence: String,
    failure_count: u32,
}

/// Get the path to the failure tracking file for a given output file
fn get_failure_file_path(output_file: &Path) -> PathBuf {
    let mut failure_path = output_file.to_path_buf();
    let filename = failure_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    failure_path.set_file_name(format!("{filename}.failures.jsonl"));
    failure_path
}

/// Load failure records from the failure tracking file
fn load_failures(failure_file: &Path) -> Result<HashMap<String, u32>> {
    let mut failures = HashMap::new();

    if failure_file.exists() {
        let file = std::fs::File::open(failure_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(record) = serde_json::from_str::<FailureRecord>(&line) {
                failures.insert(record.sentence, record.failure_count);
            }
        }
    }

    Ok(failures)
}

/// Update the failure count for a sentence
fn record_failure(
    sentence: String,
    failures: &mut HashMap<String, u32>,
    failure_file: &Path,
) -> Result<()> {
    // Increment failure count
    let count = failures.entry(sentence.clone()).or_insert(0);
    *count += 1;

    // Rewrite the entire failure file with updated counts
    let file = std::fs::File::create(failure_file)?;
    let mut writer = std::io::BufWriter::new(file);

    for (sent, &failure_count) in failures.iter() {
        let record = FailureRecord {
            sentence: sent.clone(),
            failure_count,
        };
        let json = serde_json::to_string(&record)?;
        writeln!(writer, "{json}")?;
    }

    writer.flush()?;
    Ok(())
}

/// Tokenize a list of sentences and write results to an output file
/// This function implements incremental processing - it will only tokenize sentences
/// that are not already in the output file
///
/// Returns a BTreeMap mapping each input sentence to its tokenization
pub async fn process_sentences(
    sentences: Vec<String>,
    output_file: &Path,
    language: Language,
) -> Result<BTreeMap<String, Vec<lexide::Token>>> {
    // Check if language is supported
    let lexide_language = to_lexide_language(language)
        .ok_or_else(|| anyhow::anyhow!("Language {language} is not yet supported by lexide"))?;

    // Load already processed sentences from output file (if it exists)
    let mut already_processed: BTreeMap<String, Vec<lexide::Token>> = BTreeMap::new();
    if output_file.exists() {
        let file = std::fs::File::open(output_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(tokenized) = serde_json::from_str::<TokenizedSentence>(&line) {
                already_processed.insert(tokenized.sentence, tokenized.tokens);
            }
        }
    }

    // In cache-only mode, skip the Modal server entirely and return only
    // sentences already present in the output file.
    if crate::cache_only() {
        let missing = sentences
            .iter()
            .filter(|s| !already_processed.contains_key(*s))
            .count();
        if missing > 0 {
            eprintln!(
                "cache-only: skipping {missing} untokenized sentence(s) for {language} (output: {})",
                output_file.display()
            );
        }
        let result: BTreeMap<String, Vec<lexide::Token>> = sentences
            .into_iter()
            .filter_map(|s| already_processed.get(&s).map(|tokens| (s, tokens.clone())))
            .collect();
        return Ok(result);
    }

    // Initialize lexide
    let lexide = Lexide::from_server("https://anchpop--lexide-gemma-4-31b-vllm-serve.modal.run")
        .context("Failed to initialize lexide")?;

    // Load failure tracking
    let failure_file = get_failure_file_path(output_file);
    let mut failures = load_failures(&failure_file)?;

    // Filter out already processed sentences AND previously failed sentences
    let sentences_to_process: HashSet<String> = sentences
        .iter()
        .filter(|s| !already_processed.contains_key(*s) && !failures.contains_key(*s))
        .cloned()
        .collect();

    if sentences_to_process.is_empty() {
        // Return only the sentences that were requested
        let result: BTreeMap<String, Vec<lexide::Token>> = sentences
            .into_iter()
            .filter_map(|s| already_processed.get(&s).map(|tokens| (s, tokens.clone())))
            .collect();
        return Ok(result);
    }

    // Open output file in append mode
    let output_file_handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output_file)?;
    let mut writer = std::io::BufWriter::new(output_file_handle);

    // Process sentences concurrently

    let pb = ProgressBar::new(sentences_to_process.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sentences ({per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Process all sentences concurrently with buffering and collect results
    let mut newly_processed: BTreeMap<String, Vec<lexide::Token>> = BTreeMap::new();
    let mut results = futures::stream::iter(sentences_to_process)
        .map(|sentence| {
            let lexide = &lexide;
            let pb = pb.clone();
            async move {
                let result = match lexide.analyze(&sentence, lexide_language).await {
                    Ok(tokenization) => Ok(TokenizedSentence {
                        sentence,
                        tokens: tokenization.tokens,
                    }),
                    Err(e) => {
                        eprintln!("Warning: Failed to analyze sentence '{sentence}': {e:?}");
                        Err(sentence)
                    }
                };

                pb.inc(1);
                result
            }
        })
        .buffer_unordered(600);

    // Write results as they come in and collect them in memory
    while let Some(result) = results.next().await {
        match result {
            Ok(tokenized) => {
                let json = serde_json::to_string(&tokenized)?;
                writeln!(writer, "{json}")?;
                newly_processed.insert(tokenized.sentence, tokenized.tokens);
            }
            Err(failed_sentence) => {
                // Record the failure
                record_failure(failed_sentence, &mut failures, &failure_file)?;
            }
        }
    }

    pb.finish_and_clear();

    writer.flush()?;

    // Build result map containing only the requested sentences
    // Merge newly processed sentences with already processed ones
    already_processed.extend(newly_processed);

    // Filter to only return the sentences that were requested
    let result: BTreeMap<String, Vec<lexide::Token>> = sentences
        .into_iter()
        .filter_map(|s| already_processed.get(&s).map(|tokens| (s, tokens.clone())))
        .collect();

    Ok(result)
}

/// Convert tokenizations to Literals without multiword detection.
/// This is useful when you only need the literal words from sentences
/// (e.g., for omnigram training) without the heavier multiword matching.
pub fn convert_tokens_to_literals(
    sentences_tokenizations: &BTreeMap<String, Vec<lexide::Token>>,
    language: Language,
) -> BTreeMap<String, Vec<language_utils::Literal<String>>> {
    use language_utils::Literal;

    let proper_nouns = BTreeMap::new();

    sentences_tokenizations
        .iter()
        .map(|(sentence_str, tokens)| {
            let words: Vec<Literal<String>> = tokens
                .iter()
                .enumerate()
                .map(|(i, token)| {
                    crate::lexide_token::lexide_token_to_literal(
                        token,
                        &proper_nouns,
                        language,
                        i == 0,
                    )
                })
                .collect();
            (sentence_str.clone(), words)
        })
        .collect()
}

/// Generate NLP analyzed sentences by matching multiword terms against sentences
/// using lemma matcher (high confidence), discontinuous lemma matcher (low confidence),
/// and dependency matcher (low confidence).
///
/// Takes pre-computed literals (from `convert_tokens_to_literals`), the raw
/// sentence tokenizations, and pre-computed matcher data:
/// - `lemma_patterns`: maps multiword term labels to their lemma sequences (contiguous)
/// - `discontinuous_lemma_patterns`: maps multiword term labels to their lemma sequences (gapped)
/// - `tree_patterns`: maps multiword term labels to their dependency tree patterns
///
/// This is fast since it only involves local pattern matching (no API calls).
///
/// Returns a BTreeMap containing all the input sentences that were successfully processed
pub async fn generate_nlp_sentences(
    sentence_literals: &BTreeMap<String, Vec<language_utils::Literal<String>>>,
    sentences_tokenizations: &BTreeMap<String, Vec<lexide::Token>>,
    lemma_patterns: &BTreeMap<Gram<String>, Vec<(String, lexide::pos::PartOfSpeech)>>,
    discontinuous_lemma_patterns: &BTreeMap<Gram<String>, Vec<(String, lexide::pos::PartOfSpeech)>>,
    tree_patterns: &BTreeMap<Gram<String>, lexide::matching::TreeNode>,
) -> Result<BTreeMap<String, language_utils::SentenceInfo>> {
    use language_utils::{MultiwordTerms, SentenceInfo};
    use lexide::matching::{DependencyMatcher, DiscontinuousLemmaMatcher, LemmaMatcher, TreeNode};

    type PatternList<'a> = Vec<(Gram<String>, Vec<(&'a str, lexide::pos::PartOfSpeech)>)>;

    // Build lemma matcher (high confidence) — use Gram<String> keys directly
    let lemma_patterns: PatternList<'_> = lemma_patterns
        .iter()
        .map(|(gram, lemma_pos_pairs)| {
            (
                gram.clone(),
                lemma_pos_pairs
                    .iter()
                    .map(|(s, pos)| (s.as_str(), *pos))
                    .collect(),
            )
        })
        .collect();
    let lemma_matcher = LemmaMatcher::new(&lemma_patterns);

    // Build discontinuous lemma matcher (low confidence) — max gap of 5 tokens between anchors
    // Uses lemma-only matching (no POS) because spaCy tags discontinuous constructions
    // like "ne...que" inconsistently (ADV vs SCONJ depending on context).
    let disc_patterns: Vec<(Gram<String>, Vec<&str>)> = discontinuous_lemma_patterns
        .iter()
        .map(|(gram, lemma_pos_pairs)| {
            (
                gram.clone(),
                lemma_pos_pairs.iter().map(|(s, _pos)| s.as_str()).collect(),
            )
        })
        .collect();
    let discontinuous_matcher = DiscontinuousLemmaMatcher::new(&disc_patterns, Some(5));

    // Build dependency matcher (low confidence) — use Gram<String> keys directly
    let tree_patterns: Vec<(Gram<String>, TreeNode)> = tree_patterns
        .iter()
        .map(|(gram, tree)| (gram.clone(), tree.clone()))
        .collect();
    let dependency_matcher = DependencyMatcher::new(&tree_patterns);

    // Process all sentences
    let mut result: BTreeMap<String, SentenceInfo> = BTreeMap::new();

    for (sentence_str, tokens) in sentences_tokenizations.iter() {
        let tokenization = lexide::Tokenization {
            tokens: tokens.clone(),
        };

        // Find high confidence matches using lemma matcher
        let lemma_matches = lemma_matcher.find_all(&tokenization);
        let high_confidence: Vec<Gram<String>> = lemma_matches
            .iter()
            .map(|m| m.matched_label.clone())
            .collect();

        // Find matches using discontinuous lemma matcher
        // Gap ≤ 1 → high confidence, gap > 1 → low confidence
        let disc_matches = discontinuous_matcher.find_all(&tokenization);
        let mut high_confidence = high_confidence;
        let mut low_confidence: Vec<Gram<String>> = Vec::new();
        for m in &disc_matches {
            if high_confidence.contains(&m.matched_label) {
                continue;
            }
            let max_gap = m
                .positions
                .windows(2)
                .map(|w| w[1] - w[0] - 1)
                .max()
                .unwrap_or(0);
            if max_gap <= 1 {
                high_confidence.push(m.matched_label.clone());
            } else {
                low_confidence.push(m.matched_label.clone());
            }
        }

        // Find low confidence matches using dependency matcher
        if let Ok(tree) = TreeNode::try_from(tokenization.clone()) {
            let dep_matches = dependency_matcher.find_all(&tree);

            for m in &dep_matches {
                if !high_confidence.contains(&m.matched_label)
                    && !low_confidence.contains(&m.matched_label)
                {
                    low_confidence.push(m.matched_label.clone());
                }
            }
        }

        // Use pre-computed literals
        let words = sentence_literals
            .get(sentence_str)
            .cloned()
            .unwrap_or_default();

        let sentence_info = SentenceInfo {
            words,
            multiword_terms: MultiwordTerms {
                high_confidence,
                low_confidence,
            },
        };

        // Store in result map
        result.insert(sentence_str.clone(), sentence_info);
    }

    Ok(result)
}
