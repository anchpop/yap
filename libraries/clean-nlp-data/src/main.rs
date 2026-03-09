mod classify;
mod utils;

use anyhow::{Context, anyhow};
use classify::{
    SentenceClassification, clean_sentence_with_llm, get_classifier, get_corrector,
    parse_dependencies_with_llm,
};
use futures::StreamExt;
use generate_data::target_sentences;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Language, NlpAnalyzedSentence};
use rand::prelude::IndexedRandom;
use sentence_sampler::sample_to_target;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;
use utils::{ValidationResult, validate_and_fix_whitespace};

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_service_tier("flex")
});

static CHAT_CLIENT_MINI: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5-mini")
        .unwrap()
        .with_cache_directory("./.cache")
});

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
        _ => Err(anyhow!(
            "Unknown language code '{}'. Supported codes: fra, deu, spa, eng, kor, por, ita",
            code
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

async fn load_multiword_terms(language: Language) -> anyhow::Result<Vec<NlpAnalyzedSentence>> {
    let base_dir = base_output_directory(language);
    std::fs::create_dir_all(&base_dir).context("Failed to create output directory")?;
    let base_dir = base_dir
        .canonicalize()
        .context("Failed to canonicalize output directory")?;

    let course = Course {
        native_language: default_native_language(language),
        target_language: language,
    };

    // Ensure multiword terms file exists
    let multiword_terms_file =
        generate_data::wiktionary_terms::ensure_multiword_terms_file(&course, &base_dir)
            .await
            .context("Failed to ensure multiword terms file")?;

    // Create a file with the terms as sentences (one per line, JSON string format)
    let terms_as_sentences_path = base_dir.join("multiword_terms_as_sentences.jsonl");
    if !terms_as_sentences_path.exists() {
        let terms_file =
            File::open(&multiword_terms_file).context("Failed to open multiword terms file")?;
        let reader = BufReader::new(terms_file);

        let output_file = File::create(&terms_as_sentences_path)
            .context("Failed to create terms as sentences file")?;
        let mut writer = BufWriter::new(output_file);

        for line in reader.lines() {
            let term = line.context("Failed to read line from multiword terms")?;
            let term = term.trim();
            if !term.is_empty() {
                writeln!(writer, "{}", serde_json::to_string(&term)?)
                    .context("Failed to write term as sentence")?;
            }
        }
        writer.flush().context("Failed to flush writer")?;
    }

    // Process the terms with NLP
    let terms_nlp_path = base_dir.join("multiword_terms_nlp.jsonl");
    if !terms_nlp_path.exists() {
        println!("Running Python NLP on multiword terms for {language:?}...");
        // Create an empty multiword terms file for the NLP (since we're analyzing the terms themselves)
        let empty_terms_file = base_dir.join("empty_multiword_terms.jsonl");
        if !empty_terms_file.exists() {
            File::create(&empty_terms_file)
                .context("Failed to create empty multiword terms file")?;
        }

        run_python_nlp(
            language,
            &terms_as_sentences_path,
            &empty_terms_file,
            &terms_nlp_path,
        )?;
    }

    // Load the analyzed terms
    let file = File::open(&terms_nlp_path)
        .context(format!("Failed to open terms NLP file: {terms_nlp_path:?}"))?;
    let reader = BufReader::new(file);

    let terms: Vec<NlpAnalyzedSentence> = reader
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let line = line.context(format!("Failed to read line {idx}"))?;
            serde_json::from_str::<NlpAnalyzedSentence>(&line)
                .context(format!("Failed to deserialize line {idx}: {line}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(terms)
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
    let sentences = target_sentences::get_target_sentences(course)
        .context("Failed to load target sentences")?;

    if sentences.is_empty() {
        return Err(anyhow!(
            "No target sentences found for {:?}",
            course.target_language
        ));
    }

    let file = File::create(target_sentences_path).context(format!(
        "Failed to create target sentences file: {target_sentences_path:?}"
    ))?;
    let mut writer = BufWriter::new(file);

    for (sentence, _, _source) in sentences {
        writeln!(writer, "{}", serde_json::to_string(&sentence)?)
            .context("Failed to write target sentence")?;
    }

    writer
        .flush()
        .context("Failed to flush target sentences writer")?;

    Ok(())
}

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
        manual_sentences.insert("Bois-le !".to_string());
        manual_sentences.insert("Bois-le.".to_string());
        manual_sentences.insert("Bois un coup à ma santé.".to_string());
        manual_sentences.insert("Est-ce que Robin des Bois est vivant ?".to_string());
    }

    let samples = {
        let sample_size: usize = 6_000
            + match language {
                Language::French | Language::German | Language::Spanish => 2_000,
                _ => 0,
            };
        const TERM_SAMPLE_SIZE: usize = 5_000;

        println!("Loading NLP data for {language:?}...");
        let mut sentences = load_nlp_sentences(language).await?;
        println!("Loaded {} sentences", sentences.len());

        // Separate manual sentences from other sentences
        let (manual_nlp_sentences, other_sentences): (Vec<_>, Vec<_>) = sentences
            .into_iter()
            .partition(|s| manual_sentences.contains(&s.sentence));

        println!(
            "Found {} manual sentences, {} other sentences",
            manual_nlp_sentences.len(),
            other_sentences.len()
        );

        // Use other sentences for sampling, then add ALL manual sentences back
        sentences = other_sentences;

        let terms = load_multiword_terms(language).await?;
        println!("Loaded {} multiword terms", terms.len());

        let sampled_sentences =
            sample_to_target(sentences, sample_size, |s: &NlpAnalyzedSentence| {
                s.sentence.clone()
            });

        let sampled_terms = sample_to_target(terms, TERM_SAMPLE_SIZE, |t: &NlpAnalyzedSentence| {
            t.sentence.clone()
        });

        println!("Sampled {} sentences", sampled_sentences.len());
        println!("Sampled {} multiword terms", sampled_terms.len());

        // Add ALL manual sentences back (they should always be included)
        sampled_sentences
            .into_iter()
            .chain(sampled_terms.into_iter())
            .chain(manual_nlp_sentences.into_iter())
            .collect::<Vec<_>>()
    };
    let sample_count = samples.len();

    println!("Total samples for cleaning: {sample_count} (including all manual sentences)");

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
                    "WARNING: Skipping sentence due to LLM response error {e}: (Sentence: '{}')",
                    original_sentence.sentence
                );
                skipped_count += 1;
                continue;
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
