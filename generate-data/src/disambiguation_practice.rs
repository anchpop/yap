use anyhow::Context;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{
    Course, GramFrequencyEntry, HomophonePractice, HomophoneSentencePair, HomophoneWordPair,
};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

/// A pair of raw sentence strings from the LLM (before parsing asterisks)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct RawSentencePair {
    sentence1: String,
    sentence2: String,
}

/// LLM response for generating homophone practice sentences (with thoughts)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct HomophonePracticeThoughts {
    word1: String,
    word2: String,
    /// Array of sentence pairs, each containing two sentences with asterisks around the target word
    sentence_pairs: Vec<RawSentencePair>,
}

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5")
            .unwrap()
            .with_cache_directory("./.cache"),
    )
});

/// Generate homophones file for the top N most frequent words in the language.
/// Returns a map from pronunciations to sets of words that share that pronunciation.
pub fn generate_homophones(
    _course: Course,
    target_language_dir: &Path,
    gram_frequencies: &[GramFrequencyEntry<String>],
    top_n: usize,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let homophones_file = target_language_dir.join(format!("homophones_top_{top_n}.jsonl"));

    if homophones_file.exists() {
        // Load existing homophones from file
        let file = File::open(&homophones_file)?;
        let reader = BufReader::new(file);
        return Ok(reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                serde_json::from_str::<(String, Vec<String>)>(&line).ok()
            })
            .collect::<BTreeMap<_, _>>());
    }

    // Get the top N words from the gram frequencies (single-atom heteronym grams)
    let top_words: HashSet<String> = gram_frequencies
        .iter()
        .filter_map(|entry| entry.gram.heteronym())
        .take(top_n)
        .map(|h| h.word.clone())
        .collect();

    // Load word_to_pronunciation data
    let word_to_pronunciation_file = target_language_dir.join("word_to_pronunciation.jsonl");
    let word_to_pronunciation: BTreeMap<String, String> = if word_to_pronunciation_file.exists() {
        let file = File::open(&word_to_pronunciation_file)
            .context("Failed to open word_to_pronunciation file")?;
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                serde_json::from_str::<(String, String)>(&line).ok()
            })
            .collect()
    } else {
        return Err(anyhow::anyhow!("word_to_pronunciation file not found"));
    };

    // Create pronunciation_to_words map for top N words only
    let mut pronunciation_to_top_words: BTreeMap<String, std::collections::BTreeSet<String>> =
        BTreeMap::new();
    for (word, pronunciation) in &word_to_pronunciation {
        if top_words.contains(word) {
            pronunciation_to_top_words
                .entry(pronunciation.clone())
                .or_default()
                .insert(word.clone());
        }
    }

    // Filter to only keep pronunciations with multiple words (actual homophones)
    let homophones: BTreeMap<String, Vec<String>> = pronunciation_to_top_words
        .into_iter()
        .filter(|(_, words)| words.len() > 1)
        .map(|(pronunciation, words)| (pronunciation, words.into_iter().collect()))
        .collect();

    // Write homophones to file
    let mut file = File::create(&homophones_file).context("Failed to create homophones file")?;
    for (pronunciation, words) in &homophones {
        let json = serde_json::to_string(&(pronunciation, words))?;
        writeln!(file, "{json}")?;
    }

    println!(
        "Found {} homophone groups in top {} words",
        homophones.len(),
        top_n
    );

    Ok(homophones)
}

/// Generate practice sentences for homophone disambiguation using an LLM
pub async fn generate_homophone_practice(
    course: Course,
    homophones: &BTreeMap<String, Vec<String>>,
    target_language_dir: &Path,
) -> anyhow::Result<BTreeMap<HomophoneWordPair<String>, HomophonePractice<String>>> {
    let practice_file = target_language_dir.join("homophone_practice.jsonl");

    let Course {
        native_language,
        target_language,
        ..
    } = course;

    // Create word pairs from homophones (only pairs, skip groups of 3+)
    let word_pairs: Vec<(String, String)> = homophones
        .values()
        .filter(|words| words.len() == 2)
        .map(|words| (words[0].clone(), words[1].clone()))
        .collect();

    let count = word_pairs.len();

    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} homophone practices ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let practice_data = futures::stream::iter(word_pairs.iter())
        .map(|(word1, word2)| {
            let pb = pb.clone();
            async move {
                let cost = CHAT_CLIENT.cost().unwrap_or(0.0);
                pb.set_message(format!("{cost:.2} ({word1} / {word2})"));

                let response: Result<HomophonePracticeThoughts, _> = CHAT_CLIENT
                    .chat_with_system_prompt(
                        format!(
                            r#"You are generating practice sentences to help {native_language} speakers learning {target_language} distinguish between homophones (words that sound the same but have different meanings).

You will be given two {target_language} words that sound identical. Generate 30 pairs of example sentences that clearly demonstrate the difference in meaning between the two words.

Requirements:
1. Each pair should have one sentence using the first word and one using the second word
2. The sentences should be natural, everyday {target_language} appropriate for beginners
3. The sentences should be very simple and easy to understand, using only very basic vocabulary and grammar.
4. The context should make the meaning clear
5. Do not use any special syntax around the target word.
6. Keep sentences relatively simple and clear
7. Use varied contexts to show different uses of each word

Output format:
{{
    "word1": "first word",
    "word2": "second word",
    "sentence_pairs": [
        {{
            "sentence1": "Sentence with word1 in context.",
            "sentence2": "Sentence with word2 in context."
        }},
        ... (30 pairs total)
    ]
}}"#
                        ),
                        format!("word1: `{word1}`\nword2: `{word2}`"),
                    )
                    .await
                    .inspect_err(|e| {
                        println!("error generating practice for {word1}/{word2}: {e:#?}");
                    });

                pb.inc(1);

                (response, word1.clone(), word2.clone())
            }
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    pb.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

    // Process the responses
    let mut practices = BTreeMap::new();
    for (response, word1, word2) in practice_data {
        if let Ok(thoughts) = response {
            let word_pair = HomophoneWordPair::new(word1.clone(), word2.clone())
                .expect("Homophone words should be different");

            let mut sentence_pairs = Vec::new();

            for RawSentencePair {
                sentence1,
                sentence2,
            } in thoughts.sentence_pairs
            {
                sentence_pairs.push(HomophoneSentencePair {
                    sentence1,
                    sentence2,
                });
            }

            if !sentence_pairs.is_empty() {
                practices.insert(word_pair, HomophonePractice { sentence_pairs });
            }
        }
    }

    // Write to file
    let mut file = File::create(&practice_file).context("Failed to create practice file")?;
    for practice in &practices {
        let json = serde_json::to_string(&practice)?;
        writeln!(file, "{json}")?;
    }

    Ok(practices)
}
