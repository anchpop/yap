mod classify;
mod polysemous_words;
mod utils;

use anyhow::{Context, anyhow};
use classify::{
    SentenceClassification, clean_sentence_with_llm, double_check_with_llm, get_classifier,
    get_corrector, language_specific_tips, parse_dependencies_with_llm,
};
use futures::StreamExt;
use generate_data::target_sentences;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Language, NlpAnalyzedSentence};
use rand::prelude::IndexedRandom;
use sentence_sampler::sample_to_target;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;
use utils::{ValidationResult, validate_and_fix_whitespace};

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_service_tier("flex")
});

static CHAT_CLIENT_MINI: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4-mini")
        .unwrap()
        .with_cache_directory("./.cache")
});

static CHAT_CLIENT_LOW_REASONING: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_reasoning_effort("low")
        .with_service_tier("flex")
});

static CHAT_CLIENT_NANO: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4-nano")
        .unwrap()
        .with_cache_directory("./.cache")
});

/// Languages that use LLM-based NLP instead of spaCy
/// (either no spaCy model exists, or spaCy output is too inconsistent)
fn needs_llm_nlp(language: Language) -> bool {
    matches!(
        language,
        Language::Hindi
            | Language::Tamil
            | Language::Telugu
            | Language::Bengali
            | Language::Japanese
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];

    match command.as_str() {
        "print" => {
            if args.len() < 4 {
                eprintln!("Error: 'print' command requires a language code and count");
                eprintln!("Usage: clean-nlp-data print <language_code> <count>");
                eprintln!("Example: clean-nlp-data print fra 40");
                return Err(anyhow!("Missing arguments for 'print' command"));
            }

            let language_code = &args[2];
            let count: usize = args[3]
                .parse()
                .context("Failed to parse count as a number")?;

            let language = parse_language_code(language_code)?;

            println!("Loading NLP data for {language:?}...");
            let mut nlp_sentences = load_nlp_sentences(language).await?;
            println!("Loaded {} sentences", nlp_sentences.len());

            // Apply corrections and filter out suspicious sentences
            let corrector = get_corrector(language);
            let classifier = get_classifier(language);

            let mut corrections_count = 0;
            let mut suspicious_count = 0;

            for sentence in &mut nlp_sentences {
                let correction_result = corrector.correct(sentence);
                if correction_result.corrected {
                    corrections_count += 1;
                }
            }

            let unknown_sentences: Vec<_> = nlp_sentences
                .into_iter()
                .filter(|sentence| {
                    let classification = classifier.classify(sentence);
                    match classification {
                        SentenceClassification::Unknown => true,
                        SentenceClassification::Suspicious { .. } => {
                            suspicious_count += 1;
                            false
                        }
                    }
                })
                .collect();

            println!("Applied {corrections_count} corrections");
            println!("Filtered out {suspicious_count} suspicious sentences");
            println!("\nShowing {count} random sentences:\n");

            print_random_sentences(&unknown_sentences, count);
        }
        "clean" => {
            clean_all_languages().await?;
        }
        _ => {
            eprintln!("Error: Unknown command '{command}'");
            print_usage();
            return Err(anyhow!("Unknown command"));
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Usage: clean-nlp-data <command> [args...]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  print <language_code> <count>  Print random sentences from the dataset");
    eprintln!("  clean                          Clean NLP data with LLM for all languages");
    eprintln!();
    eprintln!("Language codes (ISO 639-3):");
    eprintln!("  fra - French");
    eprintln!("  deu - German");
    eprintln!("  spa - Spanish");
    eprintln!("  eng - English");
    eprintln!("  kor - Korean");
    eprintln!("  por - Portuguese");
    eprintln!("  ita - Italian");
    eprintln!("  jpn - Japanese");
    eprintln!("  rus - Russian");
    eprintln!("  zho - Chinese");
    eprintln!("  hin - Hindi");
    eprintln!("  tam - Tamil");
    eprintln!("  tel - Telugu");
    eprintln!("  ben - Bengali");
    eprintln!("  nld - Dutch");
    eprintln!("  dan - Danish");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  clean-nlp-data print fra 40    # Print 40 random French sentences");
    eprintln!("  clean-nlp-data print deu 20    # Print 20 random German sentences");
    eprintln!("  clean-nlp-data clean           # Clean NLP data with LLM");
}

fn parse_language_code(code: &str) -> anyhow::Result<Language> {
    match code.to_lowercase().as_str() {
        "fra" => Ok(Language::French),
        "deu" => Ok(Language::German),
        "spa" => Ok(Language::Spanish),
        "eng" => Ok(Language::English),
        "kor" => Ok(Language::Korean),
        "por" => Ok(Language::Portuguese),
        "ita" => Ok(Language::Italian),
        "rus" => Ok(Language::Russian),
        "zho" => Ok(Language::Chinese),
        "jpn" => Ok(Language::Japanese),
        "hin" => Ok(Language::Hindi),
        "tam" => Ok(Language::Tamil),
        "tel" => Ok(Language::Telugu),
        "ben" => Ok(Language::Bengali),
        "nld" => Ok(Language::Dutch),
        "dan" => Ok(Language::Danish),
        _ => Err(anyhow!(
            "Unknown language code '{code}'. Supported codes: fra, deu, spa, eng, kor, por, ita, rus, zho, jpn, hin, tam, tel, ben, nld, dan"
        )),
    }
}

/// Load manual sentences for a language (these should never be filtered)
fn load_manual_sentences(language: Language) -> anyhow::Result<std::collections::HashSet<String>> {
    let manual_file = PathBuf::from(format!(
        "./generate-data/data/{}/sentence-sources/extra/manual.txt",
        language.iso_639_3()
    ));

    let mut manual_sentences = std::collections::HashSet::new();

    if manual_file.exists() {
        let content = std::fs::read_to_string(&manual_file)
            .context("Failed to read manual sentences file")?;
        for line in content.lines() {
            let line = line.trim().to_string();
            if !line.is_empty() {
                manual_sentences.insert(line);
            }
        }
        println!("Loaded {} manual sentences", manual_sentences.len());
    }

    Ok(manual_sentences)
}

/// Load NLP-analyzed sentences from cache. Used by the `print` command which
/// needs ALL sentences analyzed (not just a sample).
async fn load_nlp_sentences(language: Language) -> anyhow::Result<Vec<NlpAnalyzedSentence>> {
    let nlp_file_path = ensure_nlp_file(language).await?;

    let file = File::open(&nlp_file_path)
        .context(format!("Failed to open NLP file: {nlp_file_path:?}"))?;
    let reader = BufReader::new(file);

    let sentences: Vec<NlpAnalyzedSentence> = reader
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let line = line.context(format!("Failed to read line {idx}"))?;
            serde_json::from_str::<NlpAnalyzedSentence>(&line)
                .context(format!("Failed to deserialize line {idx}: {line}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(sentences)
}

/// Load multiword term strings from the wiktionary terms file (plain text, one per line).
fn load_multiword_term_strings(multiword_terms_file: &Path) -> anyhow::Result<Vec<String>> {
    let file = File::open(multiword_terms_file).context("Failed to open multiword terms file")?;
    let reader = BufReader::new(file);

    let terms: Vec<String> = reader
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect();

    Ok(terms)
}

/// Run spaCy NLP on a set of sentences, using a persistent cache so we only
/// process sentences we haven't seen before.
///
/// Returns NLP-analyzed sentences in the same order as the input.
fn run_nlp_cached(
    language: Language,
    sentences: &[String],
    multiword_terms_file: &Path,
    cache_file: &Path,
) -> anyhow::Result<Vec<NlpAnalyzedSentence>> {
    // Load existing cache
    let mut cache: HashMap<String, NlpAnalyzedSentence> = if cache_file.exists() {
        let file =
            File::open(cache_file).context(format!("Failed to open NLP cache: {cache_file:?}"))?;
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                let sentence: NlpAnalyzedSentence = serde_json::from_str(&line).ok()?;
                Some((sentence.sentence.clone(), sentence))
            })
            .collect()
    } else {
        HashMap::new()
    };

    // Find sentences not in cache
    let uncached: Vec<&String> = sentences
        .iter()
        .filter(|s| !cache.contains_key(s.as_str()))
        .collect();

    let uncached_unique: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        uncached
            .into_iter()
            .filter(|s| seen.insert(s.as_str().to_string()))
            .cloned()
            .collect()
    };

    if uncached_unique.is_empty() {
        println!(
            "All {} sentences found in NLP cache, skipping spaCy",
            sentences.len()
        );
    } else {
        println!(
            "NLP cache hit: {}/{} sentences cached, running spaCy on {} new sentences",
            sentences.len() - uncached_unique.len(),
            sentences.len(),
            uncached_unique.len()
        );

        // Write uncached sentences to a temp file
        let cache_dir = cache_file
            .parent()
            .context("Cache file has no parent directory")?;
        let temp_input = cache_dir.join("_temp_nlp_input.jsonl");
        let temp_output = cache_dir.join("_temp_nlp_output.jsonl");

        {
            let file = File::create(&temp_input).context("Failed to create temp NLP input file")?;
            let mut writer = BufWriter::new(file);
            for sentence in &uncached_unique {
                writeln!(writer, "{}", serde_json::to_string(sentence)?)
                    .context("Failed to write temp sentence")?;
            }
            writer.flush()?;
        }

        // Run spaCy
        run_python_nlp(language, &temp_input, multiword_terms_file, &temp_output)?;

        // Read results and add to cache
        let file = File::open(&temp_output).context("Failed to open temp NLP output file")?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line.context("Failed to read NLP output line")?;
            let sentence: NlpAnalyzedSentence = serde_json::from_str(&line)
                .context(format!("Failed to parse NLP output: {line}"))?;
            cache.insert(sentence.sentence.clone(), sentence);
        }

        // Write updated cache
        let file = File::create(cache_file).context("Failed to write NLP cache")?;
        let mut writer = BufWriter::new(file);
        for sentence in cache.values() {
            writeln!(writer, "{}", serde_json::to_string(sentence)?)
                .context("Failed to write cache entry")?;
        }
        writer.flush()?;

        // Clean up temp files
        let _ = std::fs::remove_file(&temp_input);
        let _ = std::fs::remove_file(&temp_output);
    }

    // Return results in input order
    let results: Vec<NlpAnalyzedSentence> = sentences
        .iter()
        .map(|s| {
            cache
                .get(s.as_str())
                .cloned()
                .with_context(|| format!("Sentence missing from cache after NLP run: '{s}'"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(results)
}

/// LLM response structure for initial NLP analysis (replaces spaCy for languages without models)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct LlmNlpToken {
    #[serde(rename = "1. text")]
    text: String,
    #[serde(rename = "2. whitespace")]
    whitespace: String,
    #[serde(rename = "3. pos")]
    pos: language_utils::PartOfSpeechTag,
    #[serde(rename = "4. lemma")]
    lemma: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct LlmNlpResponse {
    tokens: Vec<LlmNlpToken>,
}

/// Run LLM-based NLP analysis for languages without spaCy models.
/// Produces NlpAnalyzedSentence output compatible with the rest of the pipeline.
async fn run_llm_nlp_cached(
    language: Language,
    sentences: &[String],
    cache_file: &Path,
) -> anyhow::Result<Vec<NlpAnalyzedSentence>> {
    // Load existing cache
    let mut cache: HashMap<String, NlpAnalyzedSentence> = if cache_file.exists() {
        let file = File::open(cache_file)
            .context(format!("Failed to open LLM NLP cache: {cache_file:?}"))?;
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                let sentence: NlpAnalyzedSentence = serde_json::from_str(&line).ok()?;
                Some((sentence.sentence.clone(), sentence))
            })
            .collect()
    } else {
        HashMap::new()
    };

    // Find uncached sentences (deduplicated)
    let uncached: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        sentences
            .iter()
            .filter(|s| !cache.contains_key(s.as_str()) && seen.insert((*s).clone()))
            .cloned()
            .collect()
    };

    if uncached.is_empty() {
        println!(
            "All {} sentences found in LLM NLP cache, skipping",
            sentences.len()
        );
    } else {
        println!(
            "LLM NLP cache hit: {}/{} cached, running LLM NLP on {} new sentences",
            sentences.len() - uncached.len(),
            sentences.len(),
            uncached.len()
        );

        let tips = language_specific_tips(language);
        let system_prompt = format!(
            r#"You are an expert {language} linguist. Tokenize and analyze the given {language} sentence.

For each token, provide:
- "1. text": the exact text as it appears in the sentence
- "2. whitespace": any whitespace characters that follow this token (space, empty string, etc.)
- "3. pos": the Universal Dependencies POS tag (ADJ, ADP, ADV, AUX, CCONJ, DET, INTJ, NOUN, NUM, PART, PRON, PROPN, PUNCT, SCONJ, SYM, VERB, X)
- "4. lemma": the dictionary/base form of the word

CRITICAL: when you concatenate all tokens' text + whitespace in order, you MUST exactly reproduce the original sentence. Every character must be accounted for.

The lemma should be the form a learner would look up in a dictionary.{tips}"#
        );

        let pb = ProgressBar::new(uncached.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} LLM NLP ({per_sec}, ${msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let results = futures::stream::iter(uncached.into_iter())
            .map(|sentence| {
                let system_prompt = system_prompt.clone();
                let pb = pb.clone();
                async move {
                    let user_prompt = format!("Sentence: \"{sentence}\"");
                    let result: Result<LlmNlpResponse, _> = CHAT_CLIENT_NANO
                        .chat_with_system_prompt(&system_prompt, user_prompt)
                        .await;

                    pb.set_message(format!("{:.2}", CHAT_CLIENT_NANO.cost().unwrap_or(0.0)));
                    pb.inc(1);

                    (sentence, result)
                }
            })
            .buffer_unordered(50)
            .collect::<Vec<_>>()
            .await;

        pb.finish_with_message(format!("{:.2}", CHAT_CLIENT_NANO.cost().unwrap_or(0.0)));

        // Convert LLM responses to NlpAnalyzedSentence and add to cache
        let mut success_count = 0;
        let mut fail_count = 0;
        for (sentence, result) in results {
            match result {
                Ok(response) => {
                    let doc: Vec<language_utils::DocToken> = response
                        .tokens
                        .into_iter()
                        .map(|t| language_utils::DocToken {
                            text: t.text,
                            whitespace: t.whitespace,
                            pos: t.pos,
                            lemma: t.lemma,
                            morph: std::collections::BTreeMap::new(),
                        })
                        .collect();

                    let analyzed = NlpAnalyzedSentence {
                        sentence: sentence.clone(),
                        multiword_terms: language_utils::MultiwordTerms {
                            high_confidence: vec![],
                            low_confidence: vec![],
                        },
                        doc,
                    };
                    cache.insert(sentence, analyzed);
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!("WARNING: LLM NLP failed for '{sentence}': {e}");
                    fail_count += 1;
                }
            }
        }
        println!("LLM NLP: {success_count} succeeded, {fail_count} failed");

        // Write updated cache
        let file = File::create(cache_file).context("Failed to write LLM NLP cache")?;
        let mut writer = BufWriter::new(file);
        for sentence in cache.values() {
            writeln!(writer, "{}", serde_json::to_string(sentence)?)
                .context("Failed to write cache entry")?;
        }
        writer.flush()?;
    }

    // Return results in input order, skipping sentences that failed
    let results: Vec<NlpAnalyzedSentence> = sentences
        .iter()
        .filter_map(|s| cache.get(s.as_str()).cloned())
        .collect();

    Ok(results)
}

fn print_random_sentences(sentences: &[NlpAnalyzedSentence], count: usize) {
    let mut rng = rand::rng();
    let sample_size = count.min(sentences.len());

    let sampled: Vec<_> = sentences.choose_multiple(&mut rng, sample_size).collect();

    for (i, sentence) in sampled.iter().enumerate() {
        if i > 0 {
            println!("\n======================================================================\n");
        }

        println!("Input: {}", sentence.sentence);
        println!("{}", "-".repeat(50));
        println!("Output:");

        for (idx, token) in sentence.doc.iter().enumerate() {
            println!("{}\t{}\t{:?}\t{}", idx, token.text, token.pos, token.lemma);
        }
    }
}

fn default_native_language(language: Language) -> Language {
    match language {
        Language::English => Language::French,
        _ => Language::English,
    }
}

fn base_output_directory(language: Language) -> PathBuf {
    PathBuf::from(format!("./out/clean-nlp-data/{}", language.iso_639_3()))
}

fn ensure_target_sentences_file(
    course: Course,
    target_sentences_path: &Path,
) -> anyhow::Result<()> {
    println!(
        "Generating target language sentences for {:?}...",
        course.target_language
    );
    let target_sentences = target_sentences::get_target_sentences(course)
        .context("Failed to load target sentences")?;

    if target_sentences.app_sentences.is_empty() {
        return Err(anyhow!(
            "No target sentences found for {:?}",
            course.target_language
        ));
    }

    let file = File::create(target_sentences_path).context(format!(
        "Failed to create target sentences file: {target_sentences_path:?}"
    ))?;
    let mut writer = BufWriter::new(file);

    for (sentence, _, _source) in target_sentences.app_sentences {
        writeln!(writer, "{}", serde_json::to_string(&sentence)?)
            .context("Failed to write target sentence")?;
    }

    writer
        .flush()
        .context("Failed to flush target sentences writer")?;

    Ok(())
}

/// Ensure all NLP data exists for a language (used by the `print` command).
/// This runs spaCy on ALL sentences — for the `clean` command, use
/// `run_nlp_cached` instead which only processes the sampled subset.
async fn ensure_nlp_file(language: Language) -> anyhow::Result<PathBuf> {
    let base_dir = base_output_directory(language);
    std::fs::create_dir_all(&base_dir).context("Failed to create NLP output directory")?;
    let base_dir = base_dir
        .canonicalize()
        .context("Failed to canonicalize NLP output directory")?;

    let course = Course {
        native_language: default_native_language(language),
        target_language: language,
    };

    let target_sentences_path = base_dir.join("target_language_sentences.jsonl");
    if !target_sentences_path.exists() {
        ensure_target_sentences_file(course, &target_sentences_path)?;
    }

    let nlp_file_path = base_dir.join("target_language_sentences_nlp.jsonl");
    if !nlp_file_path.exists() {
        if needs_llm_nlp(course.target_language) {
            // For languages without spaCy models, use LLM-based NLP
            println!(
                "Running LLM NLP pipeline for {:?} (no spaCy model available)...",
                course.target_language
            );
            let file = File::open(&target_sentences_path)
                .context("Failed to open target sentences file")?;
            let reader = BufReader::new(file);
            let sentences: Vec<String> = reader
                .lines()
                .filter_map(|line| {
                    let line = line.ok()?;
                    serde_json::from_str::<String>(&line).ok()
                })
                .collect();

            let results =
                run_llm_nlp_cached(course.target_language, &sentences, &nlp_file_path).await?;

            // Write results as JSONL
            let file = File::create(&nlp_file_path).context("Failed to create NLP output file")?;
            let mut writer = BufWriter::new(file);
            for sentence in &results {
                writeln!(writer, "{}", serde_json::to_string(sentence)?)?;
            }
            writer.flush()?;
        } else {
            println!(
                "Running Python NLP pipeline for {:?}...",
                course.target_language
            );
            // Create an empty multiword terms file for now
            let multiword_terms_file = base_dir.join("multiword_terms.jsonl");
            if !multiword_terms_file.exists() {
                File::create(&multiword_terms_file)
                    .context("Failed to create empty multiword terms file")?;
            }
            run_python_nlp(
                course.target_language,
                &target_sentences_path,
                &multiword_terms_file,
                &nlp_file_path,
            )?;
        }
    }

    Ok(nlp_file_path)
}

fn run_python_nlp(
    language: Language,
    target_sentences_path: &Path,
    multiword_terms_file: &Path,
    nlp_output_path: &Path,
) -> anyhow::Result<()> {
    let script_path = Path::new("./generate-data/nlp/")
        .canonicalize()
        .context("Failed to canonicalize script path")?;

    let status = Command::new("uv")
        .arg("run")
        .arg("main.py")
        .arg(language.iso_639_3())
        .arg(target_sentences_path)
        .arg(multiword_terms_file)
        .arg(nlp_output_path)
        .current_dir(script_path)
        .status()
        .context("Failed to run Python NLP script")?;

    if !status.success() {
        return Err(anyhow!(
            "Python NLP script exited with status {:?}",
            status.code()
        ));
    }

    println!(
        "Successfully generated NLP data at {}",
        nlp_output_path.display()
    );

    Ok(())
}

async fn clean_all_languages() -> anyhow::Result<()> {
    let languages = vec![
        Language::French,
        Language::German,
        Language::Spanish,
        Language::English,
        Language::Korean,
        Language::Portuguese,
        Language::Italian,
        Language::Russian,
        Language::Chinese,
        Language::Japanese,
        Language::Hindi,
        Language::Tamil,
        Language::Telugu,
        Language::Bengali,
        Language::Dutch,
        Language::Danish,
    ];

    for language in languages {
        println!("\n=== Cleaning {language:?} ===");
        clean_language_with_llm(language).await?;
    }

    Ok(())
}

async fn clean_language_with_llm(language: Language) -> anyhow::Result<()> {
    // Load manual sentences that should never be filtered
    let mut manual_sentences = load_manual_sentences(language)?;

    if language == Language::French {
        manual_sentences.insert("Bois-le !".to_string());
        manual_sentences.insert("Bois-le.".to_string());
        manual_sentences.insert("Bois un coup à ma santé.".to_string());
        manual_sentences.insert("Est-ce que Robin des Bois est vivant ?".to_string());
    }

    let sample_size: usize = if language == Language::Japanese {
        200
    } else if needs_llm_nlp(language) {
        1_000
    } else {
        8_000
    };
    const TERM_SAMPLE_SIZE: usize = 5_000;

    // Step 1: Load all raw sentence strings (no spaCy yet)
    let course = Course {
        native_language: default_native_language(language),
        target_language: language,
    };

    println!("Loading target sentences for {language:?}...");
    let all_raw_sentences: Vec<String> = target_sentences::get_target_sentences(course)
        .context("Failed to load target sentences")?
        .app_sentences
        .into_iter()
        .map(|(s, _, _)| s)
        .collect();
    println!("Loaded {} raw sentences", all_raw_sentences.len());

    // Step 2: Separate manual from non-manual, sample BEFORE running spaCy
    let (manual_texts, other_texts): (Vec<_>, Vec<_>) = all_raw_sentences
        .into_iter()
        .partition(|s| manual_sentences.contains(s));

    println!(
        "Found {} manual sentences, {} other sentences",
        manual_texts.len(),
        other_texts.len()
    );

    // Step 3: Load NLP cache to find previously-processed sentences
    // (these will also have cached LLM responses, so including them is ~free)
    let base_dir = base_output_directory(language);
    std::fs::create_dir_all(&base_dir).context("Failed to create output directory")?;
    let base_dir = base_dir
        .canonicalize()
        .context("Failed to canonicalize output directory")?;

    let nlp_cache_file = base_dir.join("nlp_cache.jsonl");
    let previously_processed: std::collections::HashSet<String> = if nlp_cache_file.exists() {
        let file = File::open(&nlp_cache_file).context("Failed to open NLP cache for reading")?;
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                let sentence: NlpAnalyzedSentence = serde_json::from_str(&line).ok()?;
                Some(sentence.sentence)
            })
            .collect()
    } else {
        std::collections::HashSet::new()
    };
    println!(
        "Found {} previously spaCy-processed sentences in cache",
        previously_processed.len()
    );

    // Include previously-processed sentences (their LLM responses are likely cached too)
    let other_set: std::collections::HashSet<&String> = other_texts.iter().collect();
    let cached_sentences: Vec<String> = previously_processed
        .iter()
        .filter(|s| other_set.contains(s) && !manual_sentences.contains(s.as_str()))
        .cloned()
        .collect();
    println!(
        "{} cached sentences are still in the current sentence pool",
        cached_sentences.len()
    );

    let sampled_texts = sample_to_target(other_texts, sample_size, |s: &String| s.clone());
    println!("Sampled {} sentences", sampled_texts.len());

    // Union of sampled + previously cached (deduplicated)
    let mut seen: std::collections::HashSet<String> = sampled_texts.iter().cloned().collect();
    let mut combined_texts = sampled_texts;
    for s in cached_sentences {
        if seen.insert(s.clone()) {
            combined_texts.push(s);
        }
    }
    println!(
        "Combined {} sentences (sampled + previously cached)",
        combined_texts.len()
    );
    let sampled_texts = combined_texts;

    let multiword_terms_file =
        generate_data::wiktionary_terms::ensure_multiword_terms_file(&course, &base_dir)
            .await
            .context("Failed to ensure multiword terms file")?;

    let term_strings = load_multiword_term_strings(&multiword_terms_file)?;
    println!("Loaded {} multiword terms", term_strings.len());

    let sampled_term_strings =
        sample_to_target(term_strings, TERM_SAMPLE_SIZE, |s: &String| s.clone());
    println!("Sampled {} multiword terms", sampled_term_strings.len());

    // Step 4: Combine all sentences that need NLP processing
    let all_needed: Vec<String> = sampled_texts
        .into_iter()
        .chain(sampled_term_strings)
        .chain(manual_texts)
        .collect();

    println!(
        "Total sentences for NLP processing: {} (including all manual sentences)",
        all_needed.len()
    );

    // Step 5: Run NLP analysis with caching
    // For languages without spaCy models, use LLM-based NLP (gpt-5.4-nano)
    // For others, use spaCy via Python
    let samples = if needs_llm_nlp(language) {
        run_llm_nlp_cached(language, &all_needed, &nlp_cache_file).await?
    } else {
        run_nlp_cached(
            language,
            &all_needed,
            &multiword_terms_file,
            &nlp_cache_file,
        )?
    };

    let sample_count = samples.len();
    println!("Total samples for cleaning: {sample_count}");

    // Classify each sentence to get suspicious reasons
    let classifier = get_classifier(language);
    let corrector = get_corrector(language);
    let classified_sentences: Vec<_> = samples
        .into_iter()
        .map(|mut sentence| {
            let classification = classifier.classify(&sentence);
            let suspicious_reason = match classification {
                SentenceClassification::Suspicious { reasons } => reasons,
                SentenceClassification::Unknown => vec![],
            };
            corrector.correct(&mut sentence);
            (sentence, suspicious_reason)
        })
        .collect();

    // Clean each sentence with LLM
    let pb = ProgressBar::new(sample_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sentences cleaned ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let cleaned_results = futures::stream::iter(classified_sentences.into_iter())
        .map(|(sentence, suspicious_reasons)| {
            let pb = pb.clone();
            async move {
                let corrector = get_corrector(language);
                let result =
                    clean_sentence_with_llm(language, &sentence, suspicious_reasons, &CHAT_CLIENT)
                        .await
                        .map(|mut tokens| {
                            corrector.post_corrections(&mut tokens);
                            tokens
                        });

                pb.set_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));
                pb.inc(1);

                (sentence, result)
            }
        })
        .buffer_unordered(50)
        .collect::<Vec<_>>()
        .await;

    pb.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

    // Write results to file
    let output_dir = PathBuf::from("./out");
    std::fs::create_dir_all(&output_dir).context("Failed to create output directory")?;

    let output_file = output_dir.join(format!("cleaned_{}.jsonl", language.iso_639_3()));
    let file = File::create(&output_file)
        .context(format!("Failed to create output file: {output_file:?}"))?;
    let mut writer = BufWriter::new(file);

    let mut skipped_count = 0;
    let mut auto_fixed_count = 0;

    // Validate and collect successfully cleaned sentences
    let mut validated_results = Vec::new();

    for (original_sentence, mut result) in cleaned_results {
        // Validate that the LLM response matches the original text
        match result {
            Ok(ref mut corrected_tokens) => {
                match validate_and_fix_whitespace(
                    &original_sentence.sentence,
                    corrected_tokens,
                    language,
                ) {
                    ValidationResult::Valid => {
                        // No issues, continue
                    }
                    ValidationResult::AutoFixed => {
                        auto_fixed_count += 1;
                        // Continue with the auto-fixed version
                    }
                    ValidationResult::Invalid {
                        original,
                        reconstructed,
                    } => {
                        println!(
                            "WARNING: Skipping sentence due to text mismatch:\n  Original:      '{original}'\n  Reconstructed: '{reconstructed}'"
                        );
                        skipped_count += 1;
                        continue;
                    }
                }
                validated_results.push((original_sentence, result.unwrap()));
            }
            Err(e) => {
                println!(
                    "WARNING: Skipping sentence due to LLM response error {e:?}: (Sentence: '{}')",
                    original_sentence.sentence
                );
                skipped_count += 1;
                continue;
            }
        }
    }

    // Double-check pass: re-check sentences the classifier flags
    let classifier = get_classifier(language);
    let needs_recheck: Vec<_> = validated_results
        .iter()
        .filter_map(|(sentence, tokens)| {
            classifier
                .needs_double_check(&sentence.sentence, tokens)
                .map(|reasons| (sentence.sentence.clone(), reasons))
        })
        .collect();

    let recheck_count = needs_recheck.len();
    if recheck_count > 0 {
        println!("\n=== Double-check pass: {recheck_count} sentences flagged ===");

        let recheck_set: std::collections::HashSet<String> =
            needs_recheck.iter().map(|(s, _)| s.clone()).collect();
        let recheck_reasons: std::collections::HashMap<String, Vec<String>> =
            needs_recheck.into_iter().collect();

        let pb_dc = ProgressBar::new(recheck_count as u64);
        pb_dc.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} double-checked ({per_sec}, ${msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb_dc.enable_steady_tick(std::time::Duration::from_millis(100));

        let recheck_items: Vec<_> = validated_results
            .iter()
            .filter(|(s, _)| recheck_set.contains(&s.sentence))
            .map(|(s, t)| {
                (
                    s.sentence.clone(),
                    t.clone(),
                    recheck_reasons
                        .get(&s.sentence)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();

        let recheck_results = futures::stream::iter(recheck_items.into_iter())
            .map(|(sentence_text, tokens, reasons)| {
                let pb_dc = pb_dc.clone();
                async move {
                    let result = double_check_with_llm(
                        language,
                        &sentence_text,
                        &tokens,
                        reasons,
                        &CHAT_CLIENT_LOW_REASONING,
                    )
                    .await;
                    pb_dc.set_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));
                    pb_dc.inc(1);
                    (sentence_text, result)
                }
            })
            .buffer_unordered(50)
            .collect::<Vec<_>>()
            .await;

        pb_dc.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

        // Apply double-check results back
        let corrector = get_corrector(language);
        let mut recheck_map: std::collections::HashMap<String, Vec<_>> = recheck_results
            .into_iter()
            .filter_map(|(s, r)| match r {
                Ok(mut tokens) => {
                    corrector.post_corrections(&mut tokens);
                    Some((s, tokens))
                }
                Err(e) => {
                    println!("WARNING: Double-check failed for '{s}': {e}");
                    None
                }
            })
            .collect();

        for (sentence, tokens) in validated_results.iter_mut() {
            if let Some(new_tokens) = recheck_map.remove(&sentence.sentence) {
                *tokens = new_tokens;
            }
        }
    }

    println!("\n=== Pass 2: Adding dependency information ===");

    // Second pass: Add dependency information
    let validated_count = validated_results.len();

    let pb2 = ProgressBar::new(validated_count as u64);
    pb2.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} dependencies parsed ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb2.enable_steady_tick(std::time::Duration::from_millis(100));

    let results_with_deps = futures::stream::iter(validated_results.into_iter())
        .map(|(original_sentence, corrected_tokens)| {
            let pb2 = pb2.clone();
            async move {
                let dep_result = parse_dependencies_with_llm(
                    language,
                    &original_sentence.sentence,
                    &corrected_tokens,
                    &CHAT_CLIENT_MINI,
                )
                .await;

                pb2.set_message(format!("{:.2}", CHAT_CLIENT_MINI.cost().unwrap_or(0.0)));
                pb2.inc(1);

                (original_sentence, corrected_tokens, dep_result)
            }
        })
        .buffer_unordered(50)
        .collect::<Vec<_>>()
        .await;

    pb2.finish_with_message(format!("{:.2}", CHAT_CLIENT_MINI.cost().unwrap_or(0.0)));

    // Write results to file
    for (original_sentence, corrected_tokens, dep_result) in results_with_deps {
        let dep_response = match dep_result {
            Ok(dep_response) => dep_response,
            Err(e) => {
                println!(
                    "WARNING: Dependency parsing failed for sentence: {}: {}",
                    original_sentence.sentence, e
                );
                continue;
            }
        };
        if corrected_tokens.len() != dep_response.dependencies.len() {
            println!(
                "WARNING: Token/dependency count mismatch for sentence: {}",
                original_sentence.sentence
            );
            continue;
        }

        let tokens = corrected_tokens
            .into_iter()
            .zip(dep_response.dependencies.into_iter())
            .collect::<Vec<_>>();
        if tokens.iter().any(|(token, dep)| token.text != dep.word) {
            println!(
                "WARNING: Token/dependency text mismatch for sentence: {}",
                original_sentence.sentence
            );
            continue;
        }
        if tokens
            .iter()
            .enumerate()
            .any(|(i, (_token, dep))| i + 1 != dep.index)
        {
            println!(
                "WARNING: Token/dependency index mismatch for sentence: {}",
                original_sentence.sentence
            );
            continue;
        }

        let tokens: Vec<_> = tokens
            .into_iter()
            .map(|(token, dep)| {
                serde_json::json!({
                    "text": token.text,
                    "whitespace": token.whitespace,
                    "pos": token.pos,
                    "lemma": token.lemma,
                    "dep": dep.dependency,
                    "head": dep.head,
                })
            })
            .collect();

        let output = serde_json::json!({
            "sentence": original_sentence.sentence,
            "tokens": tokens,
        });
        writeln!(writer, "{}", serde_json::to_string(&output)?)
            .context("Failed to write to output file")?;
    }

    writer.flush().context("Failed to flush writer")?;

    println!("Results written to: {}", output_file.display());
    if auto_fixed_count > 0 {
        println!("Auto-fixed {auto_fixed_count} sentences with single-space mismatches");
    }
    if skipped_count > 0 {
        println!("Skipped {skipped_count} sentences due to text mismatches");
    }

    Ok(())
}
