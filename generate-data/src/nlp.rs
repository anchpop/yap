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
        Language::ChineseSimplified => Some(lexide::Language::ChineseSimplified),
        Language::Thai => Some(lexide::Language::Thai),
        // There is no Traditional Chinese NLP pipeline yet.
        Language::ChineseTraditional => None,
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

/// Load the incremental tokenization store, applying the deterministic correction
/// pass (`token_corrections::fix_tokens`) to every RETURNED entry. The store itself
/// is never touched: it holds the model's raw output, so the correction rules can
/// be revisited later without having lost what the model actually said. Everything
/// downstream of this loader (app data, and training exports via
/// `export-training-data`) therefore sees canonical tokens, while the file keeps
/// the raw form. Public so the export bin shares the exact same load path.
pub fn load_canonicalized(
    output_file: &Path,
    language: Language,
) -> Result<BTreeMap<String, Vec<lexide::Token>>> {
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

    for tokens in already_processed.values_mut() {
        token_corrections::fix_tokens(language, tokens);
    }

    Ok(already_processed)
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

    // Load the already-processed store; the returned entries are canonicalized,
    // the file keeps the model's raw output.
    let mut already_processed = load_canonicalized(output_file, language)?;

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
            Ok(mut tokenized) => {
                // the store records the model's raw output…
                let json = serde_json::to_string(&tokenized)?;
                writeln!(writer, "{json}")?;
                // …while everything downstream sees the canonical form
                token_corrections::fix_tokens(language, &mut tokenized.tokens);
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
) -> Result<
    BTreeMap<
        String,
        language_utils::MultiwordTerms<language_utils::MultiwordTermMatch<Gram<String>>>,
    >,
> {
    use language_utils::{MultiwordTermMatch, MultiwordTerms};
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

    // Process all sentences that survived the literal-level filters (the
    // tokenization map may cover more).
    let mut result: BTreeMap<String, MultiwordTerms<MultiwordTermMatch<Gram<String>>>> =
        BTreeMap::new();

    for (sentence_str, tokens) in sentences_tokenizations.iter() {
        if !sentence_literals.contains_key(sentence_str) {
            continue;
        }
        let tokenization = lexide::Tokenization {
            tokens: tokens.clone(),
        };

        // Find high confidence matches using lemma matcher. The matched word
        // indices are the contiguous token span.
        let lemma_matches = lemma_matcher.find_all(&tokenization);
        let mut high_confidence: Vec<MultiwordTermMatch<Gram<String>>> = lemma_matches
            .iter()
            .map(|m| MultiwordTermMatch {
                gram: m.matched_label.clone(),
                matched_word_indices: (m.start..m.end).map(|i| i as u16).collect(),
            })
            .collect();

        // Find matches using discontinuous lemma matcher
        // Gap ≤ 1 → high confidence, gap > 1 → low confidence
        let disc_matches = discontinuous_matcher.find_all(&tokenization);
        let mut low_confidence: Vec<MultiwordTermMatch<Gram<String>>> = Vec::new();
        for m in &disc_matches {
            if high_confidence.iter().any(|t| t.gram == m.matched_label) {
                continue;
            }
            let max_gap = m
                .positions
                .windows(2)
                .map(|w| w[1] - w[0] - 1)
                .max()
                .unwrap_or(0);
            let term = MultiwordTermMatch {
                gram: m.matched_label.clone(),
                matched_word_indices: m.positions.iter().map(|&p| p as u16).collect(),
            };
            if max_gap <= 1 {
                high_confidence.push(term);
            } else {
                low_confidence.push(term);
            }
        }

        // Find low confidence matches using dependency matcher
        if let Ok(tree) = TreeNode::try_from(tokenization.clone()) {
            let dep_matches = dependency_matcher.find_all(&tree);

            for m in &dep_matches {
                if !high_confidence.iter().any(|t| t.gram == m.matched_label)
                    && !low_confidence.iter().any(|t| t.gram == m.matched_label)
                {
                    low_confidence.push(MultiwordTermMatch {
                        gram: m.matched_label.clone(),
                        matched_word_indices: m
                            .matched_token_indices
                            .iter()
                            .map(|&i| i as u16)
                            .collect(),
                    });
                }
            }
        }

        result.insert(
            sentence_str.clone(),
            MultiwordTerms {
                high_confidence,
                low_confidence,
            },
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, pos: lexide::pos::PartOfSpeech, head: i32) -> lexide::Token {
        lexide::Token {
            text: lexide::Text {
                text: text.to_string(),
            },
            whitespace: String::new(),
            pos,
            lemma: lexide::Lemma {
                lemma: text.to_string(),
            },
            dep: lexide::DependencyRelation::Dep,
            head,
        }
    }

    #[test]
    fn load_canonicalized_corrects_in_memory_only() {
        use lexide::pos::PartOfSpeech as P;
        let dir = std::env::temp_dir().join(format!("nlp-canon-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("target_language_sentences_tokenization.jsonl");

        // A raw model-output entry: 不要 as one token.
        let record = TokenizedSentence {
            sentence: "不要走".to_string(),
            tokens: vec![tok("不要", P::Verb, 0), tok("走", P::Verb, 1)],
        };
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();

        // The returned tokens are canonical; the raw store stays byte-untouched.
        let before = std::fs::read_to_string(&path).unwrap();
        let map = load_canonicalized(&path, Language::ChineseSimplified).unwrap();
        let texts: Vec<&str> = map["不要走"].iter().map(|t| t.text.text.as_str()).collect();
        assert_eq!(texts, vec!["不", "要", "走"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
