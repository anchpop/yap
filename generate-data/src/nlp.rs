use anyhow::{Context, Result};
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::Language;
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
        // Languages not yet supported by lexide
        Language::Chinese | Language::Japanese | Language::Russian => None,
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
        .ok_or_else(|| anyhow::anyhow!("Language {} is not yet supported by lexide", language))?;

    // Initialize lexide
    let lexide = Lexide::from_server("https://anchpop--lexide-gemma-3-27b-vllm-serve.modal.run")
        .context("Failed to initialize lexide")?;

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
    let mut results = futures::stream::iter(sentences_to_process.into_iter())
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
        .buffer_unordered(900);

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

/// Generate NLP analyzed sentences by matching multiword terms against sentences
/// using lemma matcher (high confidence) and dependency matcher (low confidence)
///
/// This is fast since it only involves local pattern matching (no API calls).
///
/// Returns a BTreeMap containing all the input sentences that were successfully processed
pub async fn generate_nlp_sentences(
    sentences_tokenizations: BTreeMap<String, Vec<lexide::Token>>,
    multiword_terms_tokenizations: &BTreeMap<String, Vec<lexide::Token>>,
    language: Language,
) -> Result<BTreeMap<String, language_utils::SentenceInfo>> {
    use language_utils::{Literal, MultiwordTerms, SentenceInfo};
    use lexide::matching::{DependencyMatcher, LemmaMatcher, TreeNode};

    // Build matchers for all multiword terms

    // For lemma matcher (high confidence), create patterns from lemmas
    let lemma_patterns: Vec<(String, Vec<&str>)> = multiword_terms_tokenizations
        .iter()
        .filter(|(_, tokens)| tokens.len() > 1)
        .map(|(term_str, tokens)| {
            (
                term_str.clone(),
                tokens
                    .iter()
                    .map(|token| token.lemma.lemma.as_str())
                    .collect(),
            )
        })
        .collect();
    let lemma_matcher = LemmaMatcher::new(&lemma_patterns);

    // For dependency matcher (low confidence), create tree patterns
    let tree_patterns: Vec<(String, TreeNode)> = multiword_terms_tokenizations
        .iter()
        .filter_map(|(term_str, tokens)| {
            let tokenization = lexide::Tokenization {
                tokens: tokens.clone(),
            };
            match TreeNode::try_from(tokenization.clone()) {
                Ok(tree) => Some((term_str.clone(), tree)),
                Err(_e) => {
                    // eprintln!("Warning: Failed to create TreeNode for '{term_str}': {e}"); // todo make this less noisy
                    None
                }
            }
        })
        .collect();

    let dependency_matcher = DependencyMatcher::new(&tree_patterns);

    // Empty proper nouns map (for now, proper noun handling can be added later if needed)
    let proper_nouns = BTreeMap::new();

    // Process all sentences
    let mut result: BTreeMap<String, SentenceInfo> = BTreeMap::new();

    for (sentence_str, tokens) in sentences_tokenizations.iter() {
        let tokenization = lexide::Tokenization {
            tokens: tokens.clone(),
        };

        // Find high confidence matches using lemma matcher
        let lemma_matches = lemma_matcher.find_all(&tokenization);
        let high_confidence: Vec<String> = lemma_matches
            .iter()
            .map(|m| m.matched_label.clone())
            .collect();

        // Find low confidence matches using dependency matcher
        let low_confidence: Vec<String> = if let Ok(tree) = TreeNode::try_from(tokenization.clone())
        {
            let dep_matches = dependency_matcher.find_all(&tree);

            dep_matches
                .iter()
                .map(|m| m.matched_label.clone())
                // Filter out high confidence matches
                .filter(|term| !high_confidence.contains(term))
                .collect()
        } else {
            Vec::new()
        };

        // Convert lexide tokens to Literals
        let words: Vec<Literal<String>> = tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                crate::lexide_token::lexide_token_to_literal(token, &proper_nouns, language, i == 0)
            })
            .collect();

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
