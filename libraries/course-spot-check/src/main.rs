use anyhow::{Context as _, Result};
use chrono::{TimeZone, Utc};
use futures::StreamExt;
use language_utils::{Course, Language};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;
use weapon::AppState;
use weapon::data_model::Timestamped;
use yap_frontend_rs::{
    Challenge, Deck, DeckState, TranscribeComprehensibleSentence, TranslateComprehensibleSentence,
};

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4-mini")
        .unwrap()
        .with_cache_directory("./.course-spot-check-cache")
        .with_backup_cache_directory("./.cache")
    //.with_service_tier("flex")
});

const SENTENCES_TO_ANALYZE: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SentenceQualityResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    #[serde(rename = "2. is_grammatically_correct")]
    is_grammatically_correct: bool,
    #[serde(rename = "3. makes_sense_standalone")]
    makes_sense_standalone: bool,
    #[serde(rename = "4. issues")]
    issues: Vec<String>,
    #[serde(rename = "5. corrected_sentence")]
    corrected_sentence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct MultiwordTermResponse {
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    #[serde(rename = "2. missing_multiword_terms")]
    missing_multiword_terms: Vec<String>,
}

#[derive(Debug, Clone)]
struct SentenceAnalysis {
    sentence: String,
    #[expect(unused)]
    language: Language,
    has_quality_issues: bool,
    quality_issues: Vec<String>,
    corrected_sentence: Option<String>,
    #[expect(unused)]
    existing_multiword_terms: Vec<String>,
    missing_multiword_terms: Vec<String>,
}

#[derive(Debug)]
struct CourseAnalysis {
    course: Course,
    total_sentences_analyzed: usize,
    sentences_with_quality_issues: usize,
    sentences_with_missing_multiword_terms: usize,
    sample_issues: Vec<SentenceAnalysis>,
    all_issues: Vec<SentenceAnalysis>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BannedSentence {
    sentence: String,
    issues: Vec<String>,
    corrected_sentence: Option<String>,
}

async fn analyze_sentence_quality(
    sentence: &str,
    language: Language,
) -> Result<SentenceQualityResponse> {
    let system_prompt = format!(
        r#"You are a linguistics expert analyzing {language} sentences for language learning.

Your task is to evaluate whether a sentence is suitable for language learners by checking:
1. Grammar correctness (including proper accents in languages that use them)
2. Whether the sentence makes sense on its own without additional context
3. Whether it would be confusing for learners

Common issues to look for:
- Missing or incorrect accents (e.g., in French: "a" instead of "à")
- Grammar errors
- Incomplete thoughts or fragments. These are okay if it's plausible that someone would say it in some context. For example, "my what?" makes sense as a question and should not be considered problematic. The goal is to detect sentences that really make no sense on their own.
- Subtitle artifacts (like "[Music]" or speaker names)

If the sentence has issues that are purely typographical (like missing accents, minor spelling errors, or punctuation mistakes), provide a corrected version. Only provide corrections for minor typographical issues, not for major structural or semantic problems.

Output format:
{{
    "1. thoughts": "Brief analysis of the sentence",
    "2. is_grammatically_correct": true/false,
    "3. makes_sense_standalone": true/false,
    "4. issues": ["list", "of", "specific", "issues", "if", "any"],
    "5. corrected_sentence": "corrected version if there are minor typographical issues, otherwise null"
}}"#
    );

    let user_prompt = format!("Sentence: {sentence}");

    CHAT_CLIENT
        .chat_with_system_prompt(system_prompt, user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!(e))
        .context("Failed to analyze sentence quality")
}

async fn analyze_multiword_terms(
    sentence: &str,
    language: Language,
    existing_multiword_terms: &[String],
) -> Result<MultiwordTermResponse> {
    let system_prompt = format!(
        r#"You are a linguistics expert analyzing {language} sentences for multiword terms.

A multiword term is a group of words that must be learned as a unit because their combined meaning cannot be understood from the individual words alone. Examples:
- Phrasal verbs: "give up", "look after"
- Idiomatic expressions: "kick the bucket", "break a leg"
- Fixed expressions: "se passer" (French for "to happen")
- Compound terms with special meaning

You will receive:
1. A sentence in {language}
2. A list of already-identified multiword terms in the sentence

Your task is to identify any ADDITIONAL multiword terms that should be learned as units but are NOT already in the provided list. Terms involving verbs should be returned im their "infinitive form". So "se passe" should be returned as "passer".

Output format:
{{
    "1. thoughts": "Brief analysis of what multiword terms might be missing",
    "2. missing_multiword_terms": ["list", "of", "missing", "multiword", "terms"]
}}"#
    );

    let user_prompt = format!(
        "Sentence: {}\nExisting multiword terms: {}",
        sentence,
        if existing_multiword_terms.is_empty() {
            "None".to_string()
        } else {
            existing_multiword_terms.join(", ")
        }
    );

    CHAT_CLIENT
        .chat_with_system_prompt(system_prompt, user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!(e))
        .context("Failed to analyze multiword terms")
}

async fn analyze_course(course: Course) -> Result<CourseAnalysis> {
    println!(
        "\n=== Analyzing course: {:?} -> {:?} ===",
        course.native_language, course.target_language
    );

    // Create a deck for this course
    let deck = create_deck_for_course(course)?;

    let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let sim_start = std::time::Instant::now();
    let mut simulator = deck.simulate_usage(fixed_time);
    let mut all_sentences = Vec::new();
    let mut unique_sentences = HashSet::new();
    let mut total_challenges = 0u64;
    let mut total_days = 0u32;
    let mut next_day_time = std::time::Duration::ZERO;
    let mut challenge_iter_time = std::time::Duration::ZERO;
    let mut finish_day_time = std::time::Duration::ZERO;

    // Collect up to SENTENCES_TO_ANALYZE unique sentences from the simulator
    let pb = indicatif::ProgressBar::new(SENTENCES_TO_ANALYZE as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sentences ({per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    for _day in 0..200 {
        let t = std::time::Instant::now();
        let mut day = simulator.next_day();
        next_day_time += t.elapsed();

        let t = std::time::Instant::now();
        for challenge in day.by_ref() {
            total_challenges += 1;
            let (sentence, language, multiword_terms) = match challenge {
                Challenge::TranslateComprehensibleSentence(TranslateComprehensibleSentence {
                    target_language_literals,
                    unique_target_language_phrases,
                    ..
                }) => {
                    let sentence_text = target_language_literals
                        .iter()
                        .flat_map(|literal| {
                            vec![literal.word.text.clone(), literal.whitespace.clone()]
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    let multiword_terms: Vec<String> = unique_target_language_phrases
                        .iter()
                        .map(|g| g.to_display_string(course.target_language))
                        .collect();

                    (sentence_text, course.target_language, multiword_terms)
                }
                Challenge::TranscribeComprehensibleSentence(TranscribeComprehensibleSentence {
                    parts,
                    ..
                }) => {
                    let sentence_text = parts
                        .iter()
                        .flat_map(|p| match p {
                            language_utils::transcription_challenge::Part::AskedToTranscribe {
                                parts,
                            } => parts
                                .iter()
                                .flat_map(|p| vec![p.word.text.clone(), p.whitespace.clone()])
                                .collect::<Vec<_>>(),
                            language_utils::transcription_challenge::Part::Provided { part } => {
                                vec![part.word.text.clone(), part.whitespace.clone()]
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    let multiword_terms = Vec::new();

                    (sentence_text, course.target_language, multiword_terms)
                }
                _ => continue,
            };

            if unique_sentences.insert(sentence.clone()) {
                all_sentences.push((sentence, language, multiword_terms));
                pb.set_position(all_sentences.len() as u64);
                if all_sentences.len() >= SENTENCES_TO_ANALYZE {
                    break;
                }
            }
        }

        challenge_iter_time += t.elapsed();

        let t = std::time::Instant::now();
        simulator = day.finish_day();
        finish_day_time += t.elapsed();
        total_days += 1;

        if all_sentences.len() >= SENTENCES_TO_ANALYZE {
            break;
        }
    }

    pb.finish_with_message("done");
    println!(
        "Collected {} unique sentences for analysis ({} challenges across {} days in {:?})",
        all_sentences.len(),
        total_challenges,
        total_days,
        sim_start.elapsed()
    );
    println!(
        "  next_day: {next_day_time:?}, challenge iteration: {challenge_iter_time:?}, finish_day: {finish_day_time:?}"
    );

    // Analyze each sentence
    let mut sentences_with_quality_issues = 0;
    let mut sentences_with_missing_multiword_terms = 0;
    let mut sample_issues = Vec::new();

    let total_count = all_sentences.len();
    let analysis_pb = indicatif::ProgressBar::new(total_count as u64);
    analysis_pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} analyzed ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    analysis_pb.set_message("0.00");
    analysis_pb.enable_steady_tick(std::time::Duration::from_millis(100));
    let analysis_stream = futures::stream::iter(&all_sentences)
        .map(|(sentence, language, multiword_terms)| {
            let analysis_pb = analysis_pb.clone();
            async move {
                // Check sentence quality
                let quality_result = analyze_sentence_quality(sentence, *language).await;

                let (has_quality_issues, quality_issues, corrected_sentence) = match quality_result
                {
                    Ok(response) => {
                        let has_issues =
                            !response.is_grammatically_correct || !response.makes_sense_standalone;
                        (has_issues, response.issues, response.corrected_sentence)
                    }
                    Err(e) => {
                        eprintln!("Error analyzing quality for sentence '{sentence}': {e:?}");
                        (false, Vec::new(), None)
                    }
                };

                // Check for missing multiword terms only if sentence is okay
                let missing_multiword_terms = if !has_quality_issues {
                    match analyze_multiword_terms(sentence, *language, multiword_terms).await {
                        Ok(response) => response.missing_multiword_terms,
                        Err(e) => {
                            eprintln!(
                                "Error analyzing multiword terms for sentence '{sentence}': {e:?}"
                            );
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                };

                analysis_pb.inc(1);
                analysis_pb.set_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

                SentenceAnalysis {
                    sentence: sentence.to_string(),
                    language: *language,
                    has_quality_issues,
                    quality_issues,
                    corrected_sentence,
                    existing_multiword_terms: multiword_terms.to_vec(),
                    missing_multiword_terms,
                }
            }
        })
        .buffer_unordered(50) // Process 50 sentences concurrently
        .collect::<Vec<_>>()
        .await;
    analysis_pb.finish_with_message(format!("done, ${:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

    // Aggregate results and collect all issues
    let mut all_issues = Vec::new();
    for analysis in &analysis_stream {
        if analysis.has_quality_issues {
            sentences_with_quality_issues += 1;
            if sample_issues.len() < 10 {
                sample_issues.push(analysis.clone());
            }
        }
        if !analysis.missing_multiword_terms.is_empty() {
            sentences_with_missing_multiword_terms += 1;
            if sample_issues.len() < 20
                && !sample_issues
                    .iter()
                    .any(|s| s.sentence == analysis.sentence)
            {
                sample_issues.push(analysis.clone());
            }
        }
        // Collect all sentences with any issues
        if analysis.has_quality_issues || !analysis.missing_multiword_terms.is_empty() {
            all_issues.push(analysis.clone());
        }
    }

    Ok(CourseAnalysis {
        course,
        total_sentences_analyzed: total_count,
        sentences_with_quality_issues,
        sentences_with_missing_multiword_terms,
        sample_issues,
        all_issues,
    })
}

fn create_deck_for_course(course: Course) -> Result<Deck> {
    let language_data = match (course.native_language, course.target_language) {
        (Language::English, Language::French) => {
            include_bytes!("../../../out/fra_for_eng/language_data.rkyv").to_vec()
        }
        (Language::French, Language::English) => {
            include_bytes!("../../../out/eng_for_fra/language_data.rkyv").to_vec()
        }
        (Language::English, Language::Spanish) => {
            include_bytes!("../../../out/spa_for_eng/language_data.rkyv").to_vec()
        }
        (Language::English, Language::Korean) => {
            include_bytes!("../../../out/kor_for_eng/language_data.rkyv").to_vec()
        }
        (Language::English, Language::German) => {
            include_bytes!("../../../out/deu_for_eng/language_data.rkyv").to_vec()
        }
        (Language::English, Language::Italian) => {
            include_bytes!("../../../out/ita_for_eng/language_data.rkyv").to_vec()
        }
        (Language::English, Language::Portuguese) => {
            include_bytes!("../../../out/por_for_eng/language_data.rkyv").to_vec()
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported course: {:?} -> {:?}",
                course.native_language,
                course.target_language
            ));
        }
    };

    let archived = rkyv::access::<
        language_utils::language_pack::ArchivedLanguagePack,
        rkyv::rancor::Error,
    >(&language_data)?;

    let language_pack: language_utils::language_pack::LanguagePack = rkyv::deserialize::<
        language_utils::language_pack::LanguagePack,
        rkyv::rancor::Error,
    >(archived)?;

    let language_pack = std::sync::Arc::new(language_pack);

    let context = yap_frontend_rs::Context {
        language_pack,
        course,
    };
    let state = DeckState::new();
    let mut deck = <Deck as weapon::AppState>::finalize(state, &context);

    if let Some(event) = deck.add_next_unknown_cards(None, 100, vec![]) {
        let ts = Timestamped {
            timestamp: Utc::now(),
            within_device_events_index: 0,
            event,
        };
        let state = DeckState::from(deck);
        let state = Deck::process_event(state, &context, &ts);
        deck = Deck::finalize(state, &context);
    }

    Ok(deck)
}

fn write_results_to_files(analyses: &[CourseAnalysis]) -> Result<()> {
    println!("\n=== Writing results to files ===");

    for analysis in analyses {
        println!(
            "\nProcessing {:?} -> {:?} with {} total issues",
            analysis.course.native_language,
            analysis.course.target_language,
            analysis.all_issues.len()
        );

        let lang_dir = analysis.course.target_language.iso_639_3();
        // Use absolute path from current working directory
        let data_dir = PathBuf::from(format!("./generate-data/data/{lang_dir}"));

        // Create directory if it doesn't exist
        println!("Creating/ensuring directory exists: {data_dir:?}");
        fs::create_dir_all(&data_dir)?;

        // Read existing banned sentences
        let banned_path = data_dir.join("banned_sentences_ai.txt");
        let mut existing_banned_sentences: HashSet<String> = HashSet::new();
        if banned_path.exists() {
            let content = fs::read_to_string(&banned_path)?;
            for line in content.lines() {
                if let Ok(banned_sentence) = serde_json::from_str::<BannedSentence>(line) {
                    existing_banned_sentences.insert(banned_sentence.sentence);
                }
            }
            println!(
                "Found {} existing banned sentences",
                existing_banned_sentences.len()
            );
        }

        // Collect new banned sentences that aren't already in the file
        let mut new_banned_sentences: Vec<BannedSentence> = Vec::new();
        for issue in &analysis.all_issues {
            if issue.has_quality_issues && !existing_banned_sentences.contains(&issue.sentence) {
                new_banned_sentences.push(BannedSentence {
                    sentence: issue.sentence.clone(),
                    issues: issue.quality_issues.clone(),
                    corrected_sentence: issue.corrected_sentence.clone(),
                });
                // Add to existing set to avoid duplicates within this run
                existing_banned_sentences.insert(issue.sentence.clone());
            }
        }

        if !new_banned_sentences.is_empty() {
            let json_output = new_banned_sentences
                .iter()
                .map(|bs| serde_json::to_string(bs).unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            // Append to file (or create if it doesn't exist)
            if banned_path.exists() {
                fs::write(
                    &banned_path,
                    format!("{}\n{}", fs::read_to_string(&banned_path)?, json_output),
                )?;
            } else {
                fs::write(&banned_path, json_output)?;
            }
            println!(
                "Added {} new banned sentences to {:?}",
                new_banned_sentences.len(),
                banned_path
            );
        } else {
            println!("No new banned sentences to add for {lang_dir}");
        }

        // Read existing multiword terms
        let mwt_path = data_dir.join("extra_multiword_terms_ai.txt");
        let mut existing_multiword_terms: HashSet<String> = HashSet::new();
        if mwt_path.exists() {
            let content = fs::read_to_string(&mwt_path)?;
            for line in content.lines() {
                existing_multiword_terms.insert(line.to_string());
            }
            println!(
                "Found {} existing multiword terms",
                existing_multiword_terms.len()
            );
        }

        // Collect new multiword terms that aren't already in the file
        let mut new_multiword_terms: HashSet<String> = HashSet::new();
        for issue in &analysis.all_issues {
            for mwt in &issue.missing_multiword_terms {
                if !existing_multiword_terms.contains(mwt) {
                    new_multiword_terms.insert(mwt.clone());
                }
            }
        }

        if !new_multiword_terms.is_empty() {
            let mwt_output = new_multiword_terms
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            // Append to file (or create if it doesn't exist)
            if mwt_path.exists() {
                fs::write(
                    &mwt_path,
                    format!("{}\n{}", fs::read_to_string(&mwt_path)?, mwt_output),
                )?;
            } else {
                fs::write(&mwt_path, mwt_output)?;
            }
            println!(
                "Added {} new multiword terms to {:?}",
                new_multiword_terms.len(),
                mwt_path
            );
        } else {
            println!("No new multiword terms to add for {lang_dir}");
        }
    }

    Ok(())
}

fn print_summary(analyses: Vec<CourseAnalysis>) {
    println!("\n\n=== COURSE SPOT CHECK SUMMARY ===\n");

    for analysis in analyses {
        println!(
            "Course: {:?} -> {:?}",
            analysis.course.native_language, analysis.course.target_language
        );
        println!(
            "  Total sentences analyzed: {}",
            analysis.total_sentences_analyzed
        );
        println!(
            "  Sentences with quality issues: {} ({:.1}%)",
            analysis.sentences_with_quality_issues,
            (analysis.sentences_with_quality_issues as f64
                / analysis.total_sentences_analyzed as f64)
                * 100.0
        );
        println!(
            "  Sentences with missing multiword terms: {} ({:.1}%)",
            analysis.sentences_with_missing_multiword_terms,
            (analysis.sentences_with_missing_multiword_terms as f64
                / analysis.total_sentences_analyzed as f64)
                * 100.0
        );

        if !analysis.sample_issues.is_empty() {
            println!("\n  Sample problematic sentences:");
            for (i, issue) in analysis.sample_issues.iter().take(5).enumerate() {
                println!("    {}. \"{}\"", i + 1, issue.sentence);
                if issue.has_quality_issues {
                    println!("       Quality issues: {:?}", issue.quality_issues);
                    if let Some(ref corrected) = issue.corrected_sentence {
                        println!("       Suggested correction: \"{corrected}\"");
                    }
                }
                if !issue.missing_multiword_terms.is_empty() {
                    println!(
                        "       Missing multiword terms: {:?}",
                        issue.missing_multiword_terms
                    );
                }
            }
        }
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let filter = std::env::args().nth(1);
    let mut analyses = Vec::new();

    for course in language_utils::COURSES {
        if let Some(ref filter) = filter {
            if course.target_language.iso_639_3() != filter {
                continue;
            }
        }
        match analyze_course(*course).await {
            Ok(analysis) => analyses.push(analysis),
            Err(e) => eprintln!("Failed to analyze course {course:?}: {e}"),
        }
    }

    // Write results to files first (while we still own analyses)
    write_results_to_files(&analyses)?;

    // Then print summary (consumes analyses)
    print_summary(analyses);

    Ok(())
}
