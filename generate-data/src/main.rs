use anyhow::Context;
use futures::StreamExt;
use indexmap::IndexSet;
use itertools::Itertools;
use language_utils::{
    Atom, COURSES, EncodedSentence, Gram, GramFrequencyEntry, GramVocabEntry, HomophonePractice,
    SentenceGram, SentenceGrams,
};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::hash::Hash;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

use generate_data::cache_remote;
mod translate;
use translate::{TranslationBackend, Translator};

use generate_data::morphology_analysis;

struct PhraseDetectionData {
    tokens: Option<Vec<lexide::Token>>, // we don't have this for grams
}
type PhraseDetectionDataMap = BTreeMap<Gram<String>, PhraseDetectionData>;

/// Deduplicates a pattern map where multiple grams may produce the same matcher pattern.
/// Convert language_utils PartOfSpeech to lexide PartOfSpeech.
fn convert_pos(pos: language_utils::PartOfSpeech) -> lexide::pos::PartOfSpeech {
    use language_utils::PartOfSpeech as LP;
    use lexide::pos::PartOfSpeech as XP;
    match pos {
        LP::Adj => XP::Adj,
        LP::Adp => XP::Adp,
        LP::Adv => XP::Adv,
        LP::Aux => XP::Aux,
        LP::Cconj => XP::Cconj,
        LP::Det => XP::Det,
        LP::Intj => XP::Intj,
        LP::Noun => XP::Noun,
        LP::Num => XP::Num,
        LP::Part => XP::Part,
        LP::Pron => XP::Pron,
        LP::Sconj => XP::Sconj,
        LP::Sym => XP::Sym,
        LP::Verb => XP::Verb,
    }
}

/// When exactly one gram in a duplicate group is from wiktionary, keeps that one.
/// When multiple are from wiktionary, keeps the shortest (then alphabetically first).
/// When none are from wiktionary, warns and skips the pattern entirely.
fn deduplicate_patterns<P: Eq + Hash + Clone>(
    patterns: BTreeMap<Gram<String>, P>,
    wiktionary_grams: &HashSet<Gram<String>>,
    gram_frequencies: &rustc_hash::FxHashMap<Gram<String>, u32>,
    alt_forms: &BTreeMap<String, String>,
    lang: language_utils::Language,
) -> BTreeMap<Gram<String>, P> {
    // Invert: group grams by their pattern value
    let mut by_pattern: rustc_hash::FxHashMap<P, Vec<Gram<String>>> =
        rustc_hash::FxHashMap::default();
    for (gram, pattern) in &patterns {
        by_pattern
            .entry(pattern.clone())
            .or_default()
            .push(gram.clone());
    }

    let mut result = BTreeMap::new();
    for (pattern, grams) in by_pattern {
        if grams.len() == 1 {
            result.insert(grams.into_iter().next().unwrap(), pattern);
            continue;
        }
        let wiktionary_entries: Vec<_> = grams
            .iter()
            .filter(|g| wiktionary_grams.contains(g))
            .cloned()
            .collect();
        // Prefer wiktionary entries, but fall back to all candidates
        let candidates = if wiktionary_entries.is_empty() {
            grams
        } else {
            wiktionary_entries
        };
        // Score each candidate: (is_alt_form, has_punct, lemma_mismatches, -frequency, char_count, text)
        let score = |g: &Gram<String>| {
            let s = g.to_display_string(lang);
            let is_alt_form = alt_forms.contains_key(&s);
            let has_punct = s
                .chars()
                .any(|c| c.is_ascii_punctuation() && c != '\'' && c != '-');
            let lemma_mismatches: usize = g
                .iter()
                .filter(|atom| match atom {
                    language_utils::Atom::Tok(word) => match &word.word_type {
                        language_utils::WordType::Heteronym(h) => word.text != h.lemma,
                        _ => false,
                    },
                    _ => false,
                })
                .count();
            let freq = gram_frequencies.get(g).copied().unwrap_or(0);
            (
                is_alt_form,
                has_punct,
                lemma_mismatches,
                std::cmp::Reverse(freq),
                s.chars().count(),
                s,
            )
        };
        let best = candidates.into_iter().min_by_key(score).unwrap();
        result.insert(best, pattern);
    }
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenvy::dotenv().ok();

    // Some dependency builds rustls without a default crypto provider, which
    // panics at first TLS use (runtime-only, like the jsonwebtoken incident).
    // Err here just means a provider is already installed, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Parse CLI args. Supported forms:
    //   generate-data [--cache-only] [<lang>...]
    // Naming one or more target languages restricts the run to the courses
    // teaching them; naming none runs every course. A language teaching more
    // than one native audience (por has por_for_eng and por_for_fra) runs all
    // of its courses.
    // --cache-only puts tysm ChatClients, the Translator, and lexide
    // tokenization into cache-only mode (no network calls; cache misses error
    // out or are skipped for lexide).
    let mut lang_filter: BTreeSet<String> = BTreeSet::new();
    let mut cache_only = false;
    let mut sync_cache_only = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--cache-only" => cache_only = true,
            "--sync-cache" => sync_cache_only = true,
            s if s.starts_with("--") => {
                anyhow::bail!("unknown flag: {s}");
            }
            _ => {
                lang_filter.insert(arg);
            }
        }
    }

    // Reject unknown codes up front. A typo used to be silent -- the filter
    // simply matched nothing and the run "succeeded" having done no work, which
    // is much easier to miss now that a run can name several languages and
    // legitimately skip most courses.
    if !lang_filter.is_empty() {
        let known: BTreeSet<&str> = COURSES.iter().map(|c| c.target_language.code()).collect();
        let unknown: Vec<&str> = lang_filter
            .iter()
            .map(String::as_str)
            .filter(|code| !known.contains(code))
            .collect();
        if !unknown.is_empty() {
            anyhow::bail!(
                "unknown language code(s): {}\nknown codes: {}",
                unknown.join(", "),
                known.into_iter().collect::<Vec<_>>().join(", "),
            );
        }
        println!(
            "restricting run to: {}",
            lang_filter.iter().cloned().collect::<Vec<_>>().join(", "),
        );
    }
    generate_data::set_cache_only(cache_only);
    if cache_only {
        println!("cache-only mode: no API calls will be made");
    }

    // `--sync-cache`: just mirror the local .cache dir to/from the bucket and exit.
    // Useful for warming a fresh machine, or pushing an existing cache, without running
    // the pipeline. No-op (and exits) if YAP_CACHE_BUCKET isn't set.
    if sync_cache_only {
        if !cache_remote::enabled() {
            anyhow::bail!("--sync-cache requires YAP_CACHE_BUCKET (and R2 credentials) to be set");
        }
        cache_remote::warm().await;
        cache_remote::flush().await;
        return Ok(());
    }

    // Warm the local cache from the bucket before anything reads it (best-effort).
    cache_remote::warm().await;

    // Check and raise the file descriptor limit (macOS often defaults to 256)
    unsafe {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl);
        println!("fd limit: soft={}, hard={}", rl.rlim_cur, rl.rlim_max);
        if rl.rlim_cur < 65536 {
            let new_rl = libc::rlimit {
                rlim_cur: 65536.min(rl.rlim_max),
                rlim_max: rl.rlim_max,
            };
            libc::setrlimit(libc::RLIMIT_NOFILE, &new_rl);
            libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl);
            println!(
                "fd limit raised to: soft={}, hard={}",
                rl.rlim_cur, rl.rlim_max
            );
        }
    }

    for course in COURSES {
        if !lang_filter.is_empty() && !lang_filter.contains(course.target_language.code()) {
            continue;
        }

        println!();
        println!();
        println!(
            "Processing course: {} -> {}",
            course.native_language, course.target_language
        );
        println!("================================================");

        let target_language_dir = PathBuf::from(format!("./out/{}", course.target_language.code()));
        std::fs::create_dir_all(&target_language_dir)
            .context("Failed to create target language directory")?;
        let target_language_dir = target_language_dir
            .canonicalize()
            .context("Failed to canonicalize target language output directory")?;

        let native_specific_dir = PathBuf::from(format!(
            "./out/{}_for_{}",
            course.target_language.code(),
            course.native_language.code()
        ));
        std::fs::create_dir_all(&native_specific_dir)
            .context("Failed to create native-specific directory")?;
        let native_specific_dir = native_specific_dir
            .canonicalize()
            .context("Failed to canonicalize native-specific output directory")?;

        let source_data_path = format!("./generate-data/data/{}", course.target_language.code());
        let source_data_path = Path::new(source_data_path.as_str());

        let banned_words_file = source_data_path.join("banned_words.jsonl");
        let banned_words = if banned_words_file.exists() {
            let content = std::fs::read_to_string(banned_words_file)
                .context("Failed to read banned words file")?;
            content
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .map(|line| {
                    serde_json::from_str::<language_utils::Heteronym<String>>(line).unwrap()
                })
                .collect::<std::collections::HashSet<_>>()
        } else {
            std::collections::HashSet::new()
        };

        // write sentences
        let target_language_sentences_file =
            target_language_dir.join("target_language_sentences.jsonl");
        let translations_file =
            native_specific_dir.join("target_language_to_native_translations.jsonl");
        let sentence_sources_file = target_language_dir.join("sentence_sources.jsonl");
        // Get target sentences split into app and restricted sets
        let target_sentences_result =
            generate_data::target_sentences::get_target_sentences(*course)
                .context("Failed to get target sentences")?;
        let restricted_sentences = target_sentences_result.restricted_sentences;

        {
            let mut total_sentences = 0;

            let sentences_with_translations_and_sources = target_sentences_result.app_sentences;

            // Create the translator once and share it across all async tasks
            let translator = Translator::new(
                course.target_language, // translate from target to native
                course.native_language,
                cache_remote::store(),
                // Luna over the OpenAI Batch API is ~50x cheaper than Google's
                // translation-llm; anything already in the Google cache is
                // still reused (see translate.rs). Swap in
                // `TranslationBackend::Google` to go back.
                TranslationBackend::OpenAi {
                    model: "gpt-5.6-luna".to_string(),
                },
            )
            .await
            .context("Failed to create translator")?;

            // Warm the cache in batched requests first. The Translation LLM quota
            // is requests-per-minute, so translating the misses in bulk here means
            // the per-sentence pass below is (almost) all cache hits. Prime shows
            // its own progress bar, so set up the per-sentence bar afterwards to
            // avoid two live bars fighting over the terminal.
            let targets: Vec<String> = sentences_with_translations_and_sources
                .iter()
                .map(|(target, _, _)| target.clone())
                .collect();
            translator.prime(&targets).await;

            let total = sentences_with_translations_and_sources.len() as u64;
            let translate_label = format!(
                "translations {} → {}",
                course.target_language.iso_639_1(),
                course.native_language.iso_639_1(),
            );
            let pb = indicatif::ProgressBar::new(total);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template(&format!("{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {translate_label} ({{per_sec}}, {{msg}}, {{eta}})"))
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb.set_message(format!(
                "~${:.4} · {} calls",
                translator.cost_estimate_usd(),
                translator.api_calls()
            ));
            pb.enable_steady_tick(std::time::Duration::from_millis(100));

            let all_sentences =
                futures::stream::iter(sentences_with_translations_and_sources.into_iter().map(
                    |(target_language_sentence, native_sentence, source)| async {
                        let mut translation_set = IndexSet::new();
                        match translator.translate(&target_language_sentence).await {
                            Ok(t) => {
                                if !t.trim().is_empty() {
                                    translation_set.insert(t);
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "Error translating sentence '{target_language_sentence}': {e}"
                                );
                            }
                        };
                        if let Some(native_sentence) = native_sentence {
                            translation_set.insert(native_sentence);
                        }
                        // Machine translation can reintroduce the XProtect tripwire
                        // even when the corpus filter dropped the original pair
                        // (e.g. "Bienvenidos al Paraíso." → "Welcome to Paradise."),
                        // so filter the translations too.
                        translation_set.retain(|t| {
                            !generate_data::target_sentences::contains_xprotect_tripwire(t)
                        });
                        pb.set_message(format!(
                            "~${:.4} · {} calls",
                            translator.cost_estimate_usd(),
                            translator.api_calls()
                        ));
                        pb.inc(1);
                        (target_language_sentence, (translation_set, source))
                    },
                ))
                .buffered(100)
                .collect::<BTreeMap<_, _>>()
                .await;

            pb.finish_with_message(format!(
                "~${:.4} · {} calls",
                translator.cost_estimate_usd(),
                translator.api_calls()
            ));

            // Drop the translator to trigger the Drop implementation
            drop(translator);

            let target_language_file = match File::create(target_language_sentences_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Error creating target_language sentences file: {e}"
                    ));
                }
            };
            let mut target_language_writer = BufWriter::new(target_language_file);

            let translations_file_handle = match File::create(translations_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!("Error creating translations file: {e}"));
                }
            };
            let mut translations_writer = BufWriter::new(translations_file_handle);

            let sentence_sources_file_handle = match File::create(sentence_sources_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!("Error creating sentence sources file: {e}"));
                }
            };
            let mut sentence_sources_writer = BufWriter::new(sentence_sources_file_handle);

            for (target_language_sentence, (native_translations, source)) in all_sentences {
                if native_translations.is_empty() {
                    eprintln!(
                        "Warning: no translations for '{target_language_sentence}', skipping"
                    );
                    continue;
                }

                // Write individual target language sentence
                let target_language_json = serde_json::to_string(&target_language_sentence)
                    .context("Failed to serialize target language sentence")?;
                if let Err(e) = writeln!(target_language_writer, "{target_language_json}") {
                    eprintln!("Error writing to target_language sentences file: {e}");
                }

                let translation_json = serde_json::to_string(&(
                    &target_language_sentence,
                    native_translations.into_iter().collect::<Vec<_>>(),
                ))
                .context("Failed to serialize translation")?;
                if let Err(e) = writeln!(translations_writer, "{translation_json}") {
                    eprintln!("Error writing to translations file: {e}");
                }

                let source_json = serde_json::to_string(&(&target_language_sentence, &source))
                    .context("Failed to serialize sentence source")?;
                if let Err(e) = writeln!(sentence_sources_writer, "{source_json}") {
                    eprintln!("Error writing to sentence sources file: {e}");
                }

                total_sentences += 1;
            }

            // Flush the writers
            if let Err(e) = target_language_writer.flush() {
                eprintln!("Error flushing target_language sentences file: {e}");
            }
            if let Err(e) = translations_writer.flush() {
                eprintln!("Error flushing translations file: {e}");
            }
            if let Err(e) = sentence_sources_writer.flush() {
                eprintln!("Error flushing sentence sources file: {e}");
            }

            if total_sentences < 10 {
                panic!("Too few sentences written: {total_sentences}");
            }
        }

        // Ensure multiword terms file exists
        let multiword_terms_file = generate_data::wiktionary_terms::ensure_multiword_terms_file(
            course,
            &target_language_dir,
        )
        .await
        .context("Failed to ensure multiword terms file exists")?;

        // Process multiword terms with Rust NLP (lexide)
        let multiword_terms_tokenization_file =
            target_language_dir.join("target_language_multiword_terms_tokenization.jsonl");

        // Read multiword terms from file
        let multiword_terms = {
            let file =
                File::open(&multiword_terms_file).context("Failed to open multiword terms file")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map_while(Result::ok)
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<String>>()
        };

        // Download alt-form info from wiktionary
        let wiktionary_alt_forms = generate_data::wiktionary_terms::download_alt_forms(
            &multiword_terms,
            &target_language_dir,
        )
        .await
        .context("Failed to download alt forms")?;

        // Process multiword terms and get tokenizations
        let multiword_terms_tokenizations = generate_data::nlp::process_sentences(
            multiword_terms,
            &multiword_terms_tokenization_file,
            course.target_language,
        )
        .await
        .context("Failed to process multiword terms tokenization")?;

        // Filter out "multiword terms" that tokenized to a single token —
        // these are inflected forms or bad tokenizations, not real multi-word expressions
        let multiword_terms_tokenizations: BTreeMap<String, Vec<lexide::Token>> =
            multiword_terms_tokenizations
                .into_iter()
                .filter(|(term, tokens)| {
                    if tokens.len() <= 1 {
                        log::info!(
                            "Dropping single-token multiword term: {:?} ({} tokens)",
                            term,
                            tokens.len()
                        );
                        false
                    } else {
                        true
                    }
                })
                .collect();

        // Process sentences with lexide
        let target_language_tokenization_file =
            target_language_dir.join("target_language_sentences_tokenization.jsonl");

        // Read sentences from file
        let sentences = {
            let file = File::open(&target_language_sentences_file)
                .context("Failed to open target language sentences file")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<String>, _>>()
                .context("Failed to parse target language sentences")?
        };

        // Process sentences using the new Rust implementation and get tokenizations
        // (incremental processing will skip already-processed sentences)
        let sentences_tokenizations = generate_data::nlp::process_sentences(
            sentences,
            &target_language_tokenization_file,
            course.target_language,
        )
        .await
        .context("Failed to process sentences tokenization")?;

        // Convert tokenizations to literals (without multiword detection)
        // Multiword detection will happen later, after omnigram training
        let sentence_literals = generate_data::nlp::convert_tokens_to_literals(
            &sentences_tokenizations,
            course.target_language,
        );

        // Filter out sentences containing banned words before gram processing
        let sentence_literals: BTreeMap<String, Vec<language_utils::Literal<String>>> =
            sentence_literals
                .iter()
                .filter(|(_, words)| {
                    !words.iter().any(|word| {
                        word.heteronym()
                            .map(|h| banned_words.contains(h))
                            .unwrap_or(false)
                    })
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

        // Filter out sentences containing unknown (X) POS tags
        let sentence_literals: BTreeMap<String, Vec<language_utils::Literal<String>>> =
            sentence_literals
                .into_iter()
                .filter(|(_, words)| {
                    !words.iter().any(|word| {
                        matches!(
                            &word.word.word_type,
                            language_utils::WordType::Other(language_utils::OtherWord {
                                other_tag: language_utils::OtherWordType::X
                            })
                        )
                    })
                })
                .collect();

        // Process restricted (Pimsleur) sentences — tokenize to a separate cache file
        let restricted_tokenization_file =
            target_language_dir.join("restricted_sentences_tokenization.jsonl");
        let restricted_sentence_texts: Vec<String> = restricted_sentences
            .iter()
            .map(|(s, _)| s.clone())
            .collect();
        let restricted_tokenizations = if !restricted_sentence_texts.is_empty() {
            generate_data::nlp::process_sentences(
                restricted_sentence_texts,
                &restricted_tokenization_file,
                course.target_language,
            )
            .await
            .context("Failed to process restricted sentences tokenization")?
        } else {
            BTreeMap::new()
        };
        let restricted_literals = generate_data::nlp::convert_tokens_to_literals(
            &restricted_tokenizations,
            course.target_language,
        );

        // Merge restricted literals into sentence_literals for omnigram training.
        // BTreeMap insert means duplicates (sentences in both sets) are naturally handled.
        let mut all_sentence_literals = sentence_literals.clone();
        for (text, lits) in &restricted_literals {
            all_sentence_literals
                .entry(text.clone())
                .or_insert_with(|| lits.clone());
        }

        // Convert multiword term tokenizations to grams for seeding into omnigram
        let multiword_term_literals = generate_data::nlp::convert_tokens_to_literals(
            &multiword_terms_tokenizations,
            course.target_language,
        );
        let seed_grams: Vec<Gram<String>> = multiword_term_literals
            .values()
            .map(|lits| {
                let (atoms, _) = language_utils::literals_to_atoms(lits, course.target_language);
                Gram::from(atoms)
            })
            .collect();
        let wiktionary_grams: HashSet<Gram<String>> = seed_grams.iter().cloned().collect();

        // Train supertokens and write whitespace diagnostics (using all sentences including restricted)
        generate_data::tokenize::train_supertokens_and_write_diagnostics(
            &all_sentence_literals,
            course.target_language,
            &target_language_dir,
            &seed_grams,
        );

        // Load gram vocabulary from vocabulary.jsonl
        let gram_vocabulary: Vec<GramVocabEntry<String>> = {
            let vocab_file = target_language_dir.join("vocabulary.jsonl");
            let file = File::open(&vocab_file).context("Failed to open vocabulary.jsonl")?;
            let reader = BufReader::new(file);
            let mut vocab = Vec::new();
            for line in reader.lines() {
                let line = line.context("Failed to read vocabulary line")?;
                let entry: serde_json::Value =
                    serde_json::from_str(&line).context("Failed to parse vocabulary entry")?;

                // Deserialize atoms from JSON array
                let atoms: Vec<Atom<String>> = entry["atoms"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();

                vocab.push(GramVocabEntry {
                    atoms: Gram::from(atoms),
                    frequency: entry["frequency"].as_u64().unwrap_or(0) as u32,
                });
            }
            vocab
        };

        // Get set of terms that are known to be discontinuous (had "..." in source files)
        let discontinuous_terms = generate_data::wiktionary_terms::get_discontinuous_terms(course);

        // Build phrase detection map from both Wiktionary multiword terms and unigram-learned multi-atom grams
        let mut discontinuous_grams: std::collections::HashSet<Gram<String>> =
            std::collections::HashSet::new();
        let phrase_detection_map: PhraseDetectionDataMap = {
            let mut map = PhraseDetectionDataMap::new();
            // Wiktionary/lexide multiword terms (with tokens)
            for (phrase, lits) in &multiword_term_literals {
                let (atoms, _) = language_utils::literals_to_atoms(lits, course.target_language);
                let gram = Gram::from(atoms);
                let tokens = multiword_terms_tokenizations.get(phrase).cloned();
                if discontinuous_terms.contains(phrase) {
                    discontinuous_grams.insert(gram.clone());
                }
                map.insert(gram, PhraseDetectionData { tokens });
            }
            // Unigram-learned multi-atom grams (no tokens)
            for entry in &gram_vocabulary {
                if entry.atoms.len() > 1 {
                    map.entry(entry.atoms.clone())
                        .or_insert(PhraseDetectionData { tokens: None });
                }
            }
            map
        };

        // Compute initial gram frequencies from vocabulary for filtering
        // (we'll compute the full gram+phrase frequencies after filtering)
        let mut initial_gram_frequencies: Vec<GramFrequencyEntry<String>> = gram_vocabulary
            .iter()
            .filter(|entry| entry.atoms.is_learnable())
            .map(|entry| GramFrequencyEntry {
                count: entry.frequency,
                direct_count: entry.frequency,
                disambiguation_key: entry.atoms.disambiguation_key(),
                gram: entry.atoms.clone(),
            })
            .collect();
        initial_gram_frequencies.sort_by_key(|entry| std::cmp::Reverse(entry.clone()));

        // Load encoded sentences from encoded_sentences.jsonl
        let encoded_sentences: Vec<(String, EncodedSentence)> = {
            let encoded_file = target_language_dir.join("encoded_sentences.jsonl");
            let file =
                File::open(&encoded_file).context("Failed to open encoded_sentences.jsonl")?;
            let reader = BufReader::new(file);
            let mut sentences = Vec::new();
            for line in reader.lines() {
                let line = line.context("Failed to read encoded sentence line")?;
                let entry: serde_json::Value =
                    serde_json::from_str(&line).context("Failed to parse encoded sentence")?;
                let text = entry["text"].as_str().unwrap_or_default().to_string();
                let tokens: Vec<u32> = entry["tokens"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as u32))
                            .collect()
                    })
                    .unwrap_or_default();
                let capitalize_first = entry["capitalize_first"].as_bool().unwrap_or(false);
                sentences.push((
                    text,
                    EncodedSentence {
                        tokens,
                        capitalize_first,
                    },
                ));
            }
            sentences
        };

        // Split encoded sentences: app-only (in sentence_literals) vs all (including restricted)
        let all_encoded_sentences = encoded_sentences;
        let encoded_sentences: Vec<(String, EncodedSentence)> = all_encoded_sentences
            .iter()
            .filter(|(text, _)| sentence_literals.contains_key(text))
            .cloned()
            .collect();
        println!(
            "Encoded sentences: {} total, {} app-only, {} restricted-only",
            all_encoded_sentences.len(),
            encoded_sentences.len(),
            all_encoded_sentences.len() - encoded_sentences.len(),
        );

        // Pre-compute matcher data from phrase detection map
        let lang = course.target_language;
        let lemma_patterns: BTreeMap<Gram<String>, Vec<(String, lexide::pos::PartOfSpeech)>> =
            phrase_detection_map
                .iter()
                .filter_map(|(gram, data)| {
                    // If we have lexide tokens, use their lemmas and POS
                    if let Some(tokens) = data.tokens.as_ref() {
                        if tokens.len() <= 1 {
                            return None;
                        }
                        return Some((
                            gram.clone(),
                            tokens
                                .iter()
                                .map(|t| (t.lemma.lemma.clone(), t.pos))
                                .collect(),
                        ));
                    }
                    // For omnigram-discovered grams without lexide tokens,
                    // build lemma+POS patterns from the gram's atoms directly
                    if gram.len() <= 1 {
                        return None;
                    }
                    let lemma_pos_pairs: Vec<(String, lexide::pos::PartOfSpeech)> = gram
                        .iter()
                        .filter_map(|atom| match atom {
                            language_utils::Atom::Tok(word) => match &word.word_type {
                                language_utils::WordType::Heteronym(h) => {
                                    Some((h.lemma.clone(), convert_pos(h.pos)))
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                        .collect();
                    // Only include if we got a lemma+POS for every atom
                    if lemma_pos_pairs.len() == gram.len() {
                        Some((gram.clone(), lemma_pos_pairs))
                    } else {
                        None
                    }
                })
                .collect();

        let tree_patterns: BTreeMap<Gram<String>, lexide::matching::TreeNode> =
            phrase_detection_map
                .iter()
                .filter_map(|(gram, data)| {
                    let tokens = data.tokens.as_ref()?;
                    let tokenization = lexide::Tokenization {
                        tokens: tokens.clone(),
                    };
                    let tree = lexide::matching::TreeNode::try_from(tokenization).ok()?;
                    Some((gram.clone(), tree))
                })
                .collect();

        // Build frequency map for deduplication heuristic
        let gram_freq_map: rustc_hash::FxHashMap<Gram<String>, u32> = gram_vocabulary
            .iter()
            .map(|entry| (entry.atoms.clone(), entry.frequency))
            .collect();

        // Deduplicate patterns: if two grams produce the same matcher, prefer the wiktionary one
        let lemma_patterns_before: std::collections::HashSet<Gram<String>> =
            lemma_patterns.keys().cloned().collect();
        let lemma_patterns = deduplicate_patterns(
            lemma_patterns,
            &wiktionary_grams,
            &gram_freq_map,
            &wiktionary_alt_forms,
            lang,
        );
        // Remove tree_patterns for grams that were deduplicated away by lemma dedup
        let lemma_survivors: std::collections::HashSet<Gram<String>> =
            lemma_patterns.keys().cloned().collect();
        let tree_patterns: BTreeMap<_, _> = tree_patterns
            .into_iter()
            .filter(|(gram, _)| {
                // Keep if it wasn't in lemma_patterns to begin with, or if it survived dedup
                !lemma_patterns_before.contains(gram) || lemma_survivors.contains(gram)
            })
            .collect();
        let tree_patterns = deduplicate_patterns(
            tree_patterns,
            &wiktionary_grams,
            &gram_freq_map,
            &wiktionary_alt_forms,
            lang,
        );

        // Split discontinuous patterns into their own map, but keep them in the
        // contiguous map too so truly contiguous occurrences still get high confidence
        // from the LemmaMatcher.
        let discontinuous_lemma_patterns: BTreeMap<
            Gram<String>,
            Vec<(String, lexide::pos::PartOfSpeech)>,
        > = lemma_patterns
            .iter()
            .filter(|(gram, _)| discontinuous_grams.contains(gram))
            .map(|(gram, lemma_pos_pairs)| (gram.clone(), lemma_pos_pairs.clone()))
            .collect();
        let contiguous_lemma_patterns = lemma_patterns;

        // Run multiword detection (after omnigram training)
        let mut nlp_sentences = generate_data::nlp::generate_nlp_sentences(
            &sentence_literals,
            &sentences_tokenizations,
            &contiguous_lemma_patterns,
            &discontinuous_lemma_patterns,
            &tree_patterns,
        )
        .await
        .context("Failed to generate NLP sentences")?;

        // Slot-loosened multiword matching: citation forms like "arriver à
        // quelqu'un" contain placeholder words that never appear literally in
        // real sentences, so their tree patterns above never fire. Compile
        // loosened realizations of the argument slots (slot filled by any
        // nominal, or realized as a clitic pronoun), match them, LLM-grade a
        // sample per (term, realization), and merge the survivors.
        {
            use generate_data::slot_analysis::{self, SlotRealization};

            let slot_specs = slot_analysis::analyze_slots(course, &multiword_terms_tokenizations)
                .await
                .context("Failed to analyze multiword term slots")?;

            // Clitic realizations are high-precision; process them first so a
            // sentence matching both realizations lands in high confidence.
            let mut slot_patterns: Vec<(
                &String,
                Gram<String>,
                SlotRealization,
                lexide::matching::PatternNode,
            )> = Vec::new();
            for (term, specs) in &slot_specs {
                let (Some(tokens), Some(lits)) = (
                    multiword_terms_tokenizations.get(term),
                    multiword_term_literals.get(term),
                ) else {
                    continue;
                };
                let (atoms, _) = language_utils::literals_to_atoms(lits, course.target_language);
                let gram = Gram::from(atoms);
                for (realization, pattern) in slot_analysis::compile_realizations(tokens, specs) {
                    slot_patterns.push((term, gram.clone(), realization, pattern));
                }
            }
            slot_patterns.sort_by_key(|(term, _, realization, _)| {
                (std::cmp::Reverse(*realization), term.to_string())
            });

            let matches_per_pattern = slot_analysis::find_slot_matches(
                &sentences_tokenizations,
                &slot_patterns
                    .iter()
                    .map(|(_, _, _, p)| p.clone())
                    .collect::<Vec<_>>(),
            );

            // Grade every pattern's sample in one batch, then keep only the
            // realizations that clear the precision bar. Per-pattern detail
            // goes to a TSV rather than the log — there are thousands of
            // patterns for a language like English.
            let grade_requests: Vec<slot_analysis::GradeRequest> = slot_patterns
                .iter()
                .zip(&matches_per_pattern)
                .filter(|(_, matched)| !matched.is_empty())
                .map(|((term, _, realization, _), matched)| {
                    slot_analysis::GradeRequest::new((*term).clone(), *realization, matched)
                })
                .collect();
            let graded = slot_analysis::grade_patterns(&grade_requests)
                .await
                .context("Failed to grade slot patterns")?;

            let summary_path = target_language_dir.join("slot_patterns.tsv");
            slot_analysis::write_summary(&summary_path, &graded)
                .context("Failed to write slot pattern summary")?;

            let kept_patterns: HashSet<(&str, SlotRealization)> = graded
                .iter()
                .filter(|g| g.kept())
                .map(|g| (g.term.as_str(), g.realization))
                .collect();

            let mut kept_matches = 0usize;
            for ((term, gram, realization, _), matched) in
                slot_patterns.iter().zip(&matches_per_pattern)
            {
                if !kept_patterns.contains(&(term.as_str(), *realization)) {
                    continue;
                }
                for slot_match in matched {
                    let Some(info) = nlp_sentences.get_mut(&slot_match.sentence) else {
                        continue;
                    };
                    let terms = &mut info.multiword_terms;
                    if terms.high_confidence.iter().any(|t| t.gram == *gram)
                        || terms.low_confidence.iter().any(|t| t.gram == *gram)
                    {
                        continue;
                    }
                    let term = language_utils::MultiwordTermMatch {
                        gram: gram.clone(),
                        matched_word_indices: slot_match
                            .matched_token_indices
                            .iter()
                            .map(|&i| i as u16)
                            .collect(),
                    };
                    match realization {
                        SlotRealization::Clitic => terms.high_confidence.push(term),
                        SlotRealization::Filled => terms.low_confidence.push(term),
                    }
                    kept_matches += 1;
                }
            }
            println!(
                "Slot patterns: {} graded, {} kept, {kept_matches} sentence matches added (details: {})",
                graded.len(),
                kept_patterns.len(),
                summary_path.display(),
            );
        }

        // Helper closure: convert encoded sentences to SentenceGrams using gram vocabulary + NLP data
        let convert_to_grams = |sentences: &[(String, EncodedSentence)],
                                nlp: &BTreeMap<String, language_utils::SentenceInfo>|
         -> Vec<(String, SentenceGrams<Gram<String>>)> {
            sentences
                .iter()
                .map(|(text, encoded)| {
                    let grams: Vec<SentenceGram<Gram<String>>> = encoded
                        .tokens
                        .iter()
                        .filter_map(|&token_id| {
                            gram_vocabulary
                                .get(token_id as usize)
                                .map(|entry| SentenceGram::from(entry.atoms.clone()))
                        })
                        .collect();
                    let sentence_gram_set: std::collections::HashSet<&Gram<String>> = grams
                        .iter()
                        .map(|sg| match sg {
                            SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
                        })
                        .collect();
                    let (multiword_terms, low_confidence_multiword_terms) = nlp
                        .get(text)
                        .map(|info| {
                            (
                                info.multiword_terms
                                    .high_confidence
                                    .iter()
                                    .filter(|m| !sentence_gram_set.contains(&m.gram))
                                    .cloned()
                                    .collect(),
                                info.multiword_terms
                                    .low_confidence
                                    .iter()
                                    .filter(|m| !sentence_gram_set.contains(&m.gram))
                                    .cloned()
                                    .collect(),
                            )
                        })
                        .unwrap_or_default();
                    (
                        text.clone(),
                        SentenceGrams {
                            grams,
                            capitalize_first: encoded.capitalize_first,
                            multiword_terms,
                            low_confidence_multiword_terms,
                        },
                    )
                })
                .collect()
        };

        // Convert app-only encoded sentences to grams
        let encoded_sentences_with_grams: Vec<(String, SentenceGrams<Gram<String>>)> =
            convert_to_grams(&encoded_sentences, &nlp_sentences);

        // Convert ALL encoded sentences (including restricted) to grams for per-source frequencies
        let all_encoded_sentences_with_grams: Vec<(String, SentenceGrams<Gram<String>>)> =
            convert_to_grams(&all_encoded_sentences, &nlp_sentences);

        // Filter initial gram frequencies to only include those with count > 3 (like regular dictionary)
        let filtered_initial_gram_frequencies: Vec<GramFrequencyEntry<String>> =
            initial_gram_frequencies
                .iter()
                .filter(|entry| entry.count > 3)
                .cloned()
                .collect();

        // Build a set of learnable grams that passed the frequency filter
        let filtered_gram_set: std::collections::HashSet<Gram<String>> =
            filtered_initial_gram_frequencies
                .iter()
                .map(|entry| entry.gram.clone())
                .collect();

        // Save unfiltered sentences (including restricted) for computing accurate per-source total gram counts
        let unfiltered_encoded_sentences = all_encoded_sentences_with_grams;

        // Filter encoded sentences to only include those where we have all the learnable grams
        let encoded_sentences_count_before = encoded_sentences_with_grams.len();
        let mut encoded_sentences_with_grams: Vec<(String, SentenceGrams<Gram<String>>)> =
            encoded_sentences_with_grams
                .into_iter()
                .filter(|(_, sentence_grams)| {
                    // Check if all learnable grams in this sentence are in the filtered set
                    sentence_grams.grams.iter().all(|sg| {
                        match sg {
                            // Learnable grams must be in the filtered set
                            SentenceGram::Learnable(gram) => filtered_gram_set.contains(gram),
                            // Obvious (non-learnable) grams are always OK
                            SentenceGram::Obvious(_) => true,
                        }
                    })
                })
                .collect();

        println!(
            "Filtered {} encoded sentences with uncommon grams ({} -> {} sentences)",
            encoded_sentences_count_before - encoded_sentences_with_grams.len(),
            encoded_sentences_count_before,
            encoded_sentences_with_grams.len()
        );

        // Compute final gram/phrase frequencies from filtered encoded sentences
        let gram_frequencies = generate_data::frequencies::compute_gram_frequencies(
            &encoded_sentences_with_grams,
            &gram_vocabulary,
        );
        let master_total_count: u64 = gram_frequencies.iter().map(|e| e.count as u64).sum();

        // Filter to only include those with count > 3
        let filtered_gram_frequencies: Vec<GramFrequencyEntry<String>> = gram_frequencies
            .iter()
            .filter(|entry| entry.count > 3)
            .cloned()
            .collect();

        // Create gram phrasebook (for multi-atom grams), excluding grams that already
        // have MWE phrasebook entries
        let gram_phrasebook_file = native_specific_dir.join("gram_phrasebook.jsonl");
        let gram_sentences_file = native_specific_dir.join("gram_sentences.jsonl");

        // Load cached gram -> example sentences mapping
        // Try new format (keyed by Gram) first, fall back to old format (keyed by display text)
        let mut gram_sentences: BTreeMap<Gram<String>, Vec<String>> = if gram_sentences_file
            .exists()
        {
            let file =
                File::open(&gram_sentences_file).context("Failed to open gram sentences file")?;
            let mut lines = BufReader::new(file).lines();

            // Check first line to detect format
            let first_line = lines.next().and_then(|l| l.ok());
            let is_new_format = first_line.as_ref().is_some_and(|line| {
                serde_json::from_str::<(Gram<String>, Vec<String>)>(line).is_ok()
            });

            let all_lines = first_line.into_iter().chain(lines.map_while(Result::ok));

            if is_new_format {
                all_lines
                    .filter_map(|line| {
                        serde_json::from_str::<(Gram<String>, Vec<String>)>(&line).ok()
                    })
                    .collect()
            } else {
                // Migrate from old format (display text keys):
                // For monosemantic grams, reuse old sentences; polysemantic ones will be re-sampled
                let old_format: BTreeMap<String, Vec<String>> = all_lines
                    .filter_map(|line| serde_json::from_str::<(String, Vec<String>)>(&line).ok())
                    .collect();

                // Count how many multi-atom grams share each display text
                let mut display_text_counts: BTreeMap<String, u32> = BTreeMap::new();
                for entry in &filtered_gram_frequencies {
                    if entry.gram.len() > 1 {
                        *display_text_counts
                            .entry(entry.gram.to_display_string(lang))
                            .or_default() += 1;
                    }
                }

                // Only migrate monosemantic entries
                let mut migrated = BTreeMap::new();
                for entry in &filtered_gram_frequencies {
                    if entry.gram.len() > 1 {
                        let display_text = entry.gram.to_display_string(lang);
                        let is_monosemantic =
                            display_text_counts.get(&display_text).copied().unwrap_or(0) <= 1;
                        if is_monosemantic && let Some(sentences) = old_format.get(&display_text) {
                            migrated.insert(entry.gram.clone(), sentences.clone());
                        }
                    }
                }
                let polysemantic_count = display_text_counts
                    .values()
                    .filter(|&&count| count > 1)
                    .count();
                println!(
                    "Migrated {} gram sentence entries from old format (display text keys), {} polysemantic display texts will be re-sampled",
                    migrated.len(),
                    polysemantic_count
                );
                migrated
            }
        } else {
            BTreeMap::new()
        };

        let gram_phrasebook = generate_data::dict::create_gram_phrasebook(
            *course,
            &filtered_gram_frequencies,
            &encoded_sentences_with_grams,
            &mut gram_sentences,
        )
        .await
        .context("Failed to create gram phrasebook")?;

        // Write updated gram sentences cache
        {
            let mut file = File::create(&gram_sentences_file)
                .context("Failed to create gram sentences file")?;
            for (gram, sentences) in &gram_sentences {
                let json = serde_json::to_string(&(gram, sentences))
                    .context("Failed to serialize gram sentences entry")?;
                writeln!(file, "{json}").context("Failed to write gram sentences entry to file")?;
            }
            println!(
                "Wrote {} gram sentence entries to {:?}",
                gram_sentences.len(),
                gram_sentences_file
            );
        }
        {
            let mut file = File::create(&gram_phrasebook_file)
                .context("Failed to create gram phrasebook file")?;
            for (gram, entry) in &gram_phrasebook {
                // Convert gram to display string for serialization
                let gram_text = gram.to_display_string(course.target_language);
                let json = serde_json::to_string(&(gram_text, entry))
                    .context("Failed to serialize gram phrasebook entry")?;
                writeln!(file, "{json}")
                    .context("Failed to write gram phrasebook entry to file")?;
            }
            println!(
                "Wrote {} gram phrasebook entries to {:?}",
                gram_phrasebook.len(),
                gram_phrasebook_file
            );
        }

        // Build phrasebook from gram phrasebook entries
        let phrasebook: BTreeMap<Gram<String>, language_utils::PhrasebookDefinitionEntry> =
            gram_phrasebook.into_iter().collect();

        // Generate proper noun definitions
        let proper_noun_definitions_file =
            native_specific_dir.join("proper_noun_definitions.jsonl");
        let proper_noun_definitions: BTreeMap<String, language_utils::ProperNounDefinition> = {
            let definitions =
                generate_data::proper_noun_definitions::generate_proper_noun_definitions(
                    *course,
                    &nlp_sentences,
                )
                .await
                .context("Failed to generate proper noun definitions")?;

            // Write the proper noun definitions to a jsonl file
            let mut file = File::create(proper_noun_definitions_file)
                .context("Failed to create proper noun definitions file")?;
            for entry in &definitions {
                let json = serde_json::to_string(&entry)
                    .context("Failed to serialize proper noun definition")?;
                writeln!(file, "{json}")
                    .context("Failed to write proper noun definition to file")?;
            }

            definitions
        };

        // Build set of frequent words from gram_frequencies for pronunciation filtering
        let frequent_heteronym_words: std::collections::HashSet<String> = gram_frequencies
            .iter()
            .filter_map(|entry| entry.gram.heteronym())
            .filter(|h| !banned_words.contains(h))
            .map(|h| h.word.clone())
            .collect();

        let custom_definitions = {
            let path = source_data_path.join("custom_definitions.jsonl");
            let lines = if path.exists() {
                BufReader::new(File::open(&path).context("Failed to open custom definitions file")?)
                    .lines()
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            lines
                .into_iter()
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_str(&line))
                .collect::<Result<
                    BTreeMap<
                        language_utils::Heteronym<String>,
                        language_utils::DictionaryDefinition,
                    >,
                    serde_json::Error,
                >>()
                .context("Failed to parse custom definitions")?
        };

        // Compute morphology up-front so etymology can see what grammatical
        // readings each word actually has (disambiguates syncretic tags like
        // French -e = 1sg AND 3sg). This is the same morphology pass used later
        // by gram_dictionary, so no duplicated LLM calls.
        let morphology: BTreeMap<
            language_utils::Heteronym<String>,
            Vec<language_utils::features::Morphology>,
        > = morphology_analysis::create_morphology(
            course.target_language,
            &filtered_gram_frequencies,
        )
        .await
        .context("Failed to create morphology")?;

        // Collapse to per-word Leipzig-style summary:
        //   `VERB rester [1.SG.PRS.IND / 3.SG.PRS.IND]; NOUN reste`
        let word_morphology: BTreeMap<String, String> = {
            let mut per_word: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (het, morphs) in &morphology {
                let readings = language_utils::features::Morphology::leipzig_many(morphs);
                let summary = if readings.is_empty() {
                    format!("{:?} {}", het.pos, het.lemma)
                } else {
                    format!("{:?} {} [{}]", het.pos, het.lemma, readings)
                };
                per_word.entry(het.word.clone()).or_default().push(summary);
            }
            per_word
                .into_iter()
                .map(|(word, summaries)| (word, summaries.join("; ")))
                .collect()
        };

        // Build etymology segmentations from the golden JSONL seed plus LLM
        // fills for anything uncovered.
        let golden_morphemes_path = PathBuf::from(format!(
            "generate-data/data/{}/golden_morphemes.jsonl",
            course.target_language.code()
        ));
        let etymology_segmentations = if golden_morphemes_path.exists() {
            // Extract learnable single-word grams as the word list (sorted by
            // frequency, since filtered_gram_frequencies is already sorted desc).
            let learnable_words: Vec<String> = filtered_gram_frequencies
                .iter()
                .filter_map(|entry| {
                    if entry.gram.0.len() != 1 {
                        return None;
                    }
                    match &entry.gram.0[0] {
                        Atom::Tok(word) if word.heteronym().is_some() => Some(word.text.clone()),
                        _ => None,
                    }
                })
                .collect();
            println!(
                "Building etymology segmentations for {} ({} learnable words)...",
                course.target_language,
                learnable_words.len()
            );

            generate_data::etymology::build_etymology_segmentations_with_llm(
                *course,
                &golden_morphemes_path,
                &learnable_words,
                &word_morphology,
            )
            .await?
        } else {
            std::collections::BTreeMap::new()
        };
        println!(
            "Etymology segmentations: {} words",
            etymology_segmentations.len()
        );

        // Write etymology segmentations to file
        {
            let segmentations_file = target_language_dir.join("etymology_segmentations.jsonl");
            let mut file = File::create(&segmentations_file)
                .context("Failed to create etymology segmentations file")?;
            for (word, segments) in &etymology_segmentations {
                let json = serde_json::to_string(&(word, segments))
                    .context("Failed to serialize etymology segmentation")?;
                writeln!(file, "{json}").context("Failed to write etymology segmentation")?;
            }
            println!(
                "Wrote {} etymology segmentations to {:?}",
                etymology_segmentations.len(),
                segmentations_file
            );
        }

        // Classify and define every morpheme that shows up in the segmentations.
        let morphemes: BTreeMap<
            language_utils::MorphemeSegment<String>,
            language_utils::MorphemeInfo<String>,
        > = if etymology_segmentations.is_empty() {
            BTreeMap::new()
        } else {
            let morpheme_to_words =
                generate_data::morpheme_info::invert_segmentations(&etymology_segmentations);

            // Build a word → (lemma, POS) map for trie-style prefix lookup.
            // Used when resolving Free morphemes back to dictionary entries.
            let word_info: BTreeMap<String, (String, language_utils::PartOfSpeech)> =
                filtered_gram_frequencies
                    .iter()
                    .filter_map(|entry| {
                        if entry.gram.0.len() != 1 {
                            return None;
                        }
                        match &entry.gram.0[0] {
                            Atom::Tok(word) => {
                                let het = word.heteronym()?;
                                Some((word.text.clone(), (het.lemma.clone(), het.pos)))
                            }
                            _ => None,
                        }
                    })
                    .collect();

            println!(
                "Analyzing {} unique morphemes for {} ({} dictionary words for prefix lookup)...",
                morpheme_to_words.len(),
                course.target_language,
                word_info.len(),
            );
            let analyses = generate_data::morpheme_info::analyze_morphemes(
                *course,
                &morpheme_to_words,
                &word_info,
            )
            .await;

            let morpheme_info_file = target_language_dir.join("morpheme_info.jsonl");
            let mut file =
                File::create(&morpheme_info_file).context("Failed to create morpheme info file")?;
            for analysis in &analyses {
                let json =
                    serde_json::to_string(analysis).context("Failed to serialize morpheme info")?;
                writeln!(file, "{json}").context("Failed to write morpheme info")?;
            }
            println!(
                "Wrote {} morpheme info entries to {:?}",
                analyses.len(),
                morpheme_info_file,
            );

            analyses.into_iter().map(|a| (a.segment, a.kind)).collect()
        };

        // Create gram dictionary (for single-atom grams)
        // Merge morphology data and custom definitions, producing DictionaryEntry values
        let gram_dict_file = native_specific_dir.join("gram_dictionary.jsonl");
        let gram_dictionary: BTreeMap<
            language_utils::Heteronym<String>,
            language_utils::DictionaryEntry,
        > = {
            let raw_dictionary =
                generate_data::dict::create_gram_dictionary(*course, &filtered_gram_frequencies)
                    .await
                    .context("Failed to create gram dictionary")?;
            // Reuse the morphology we computed earlier (before etymology).
            raw_dictionary
                .into_iter()
                .filter_map(|(heteronym, mut def)| {
                    // Apply custom definitions if available
                    if let Some(custom_def) = custom_definitions.get(&heteronym) {
                        def = custom_def.clone();
                    }
                    // Only keep entries that have morphology
                    let morph = morphology.get(&heteronym)?.clone();
                    let segments = etymology_segmentations
                        .get(&heteronym.word)
                        .cloned()
                        .unwrap_or_default();
                    Some((
                        heteronym,
                        language_utils::DictionaryEntry {
                            target_language_word: def.target_language_word,
                            definitions: def.definitions,
                            morphology: morph,
                            segments,
                        },
                    ))
                })
                .collect()
        };
        {
            let mut file =
                File::create(&gram_dict_file).context("Failed to create gram dictionary file")?;
            for (heteronym, definition) in &gram_dictionary {
                let json = serde_json::to_string(&(heteronym, definition))
                    .context("Failed to serialize gram dictionary entry")?;
                writeln!(file, "{json}")
                    .context("Failed to write gram dictionary entry to file")?;
            }
            println!(
                "Wrote {} gram dictionary entries to {:?}",
                gram_dictionary.len(),
                gram_dict_file
            );
        }

        // Generate conjugations/declensions JSONL
        {
            let morphology_groups = morphology_analysis::analyze_morphology(&gram_dictionary);

            let conjugations_path = native_specific_dir.join("conjugations.jsonl");
            morphology_analysis::write_conjugations_jsonl(&morphology_groups, &conjugations_path)
                .context("Failed to write conjugations file")?;
        }

        // Build set of grams that have definitions
        let gram_dictionary_set: std::collections::HashSet<_> =
            gram_dictionary.keys().cloned().collect();

        // Filter gram_frequencies to only include grams that have definitions
        let gram_frequencies_count_before = gram_frequencies.len();
        let gram_frequencies: Vec<GramFrequencyEntry<String>> = gram_frequencies
            .into_iter()
            .filter(|entry| {
                let gram = &entry.gram;
                if gram.len() == 1 {
                    // Single-atom gram: check if the heteronym is in gram_dictionary
                    if let Some(Atom::Tok(word)) = gram.first()
                        && let language_utils::WordType::Heteronym(heteronym) = &word.word_type
                    {
                        return gram_dictionary_set.contains(heteronym);
                    }
                    false
                } else {
                    // Multi-atom gram: check if it's in gram_phrasebook
                    phrasebook.contains_key(gram)
                }
            })
            .collect();

        {
            let removed_count = gram_frequencies_count_before - gram_frequencies.len();
            println!(
                "Filtered gram_frequencies: removed {removed_count} grams without definitions ({gram_frequencies_count_before} -> {})",
                gram_frequencies.len()
            );
        }

        // Build gram-keyed dictionary from filtered gram_frequencies
        // For each single-atom gram, look up its heteronym in gram_dictionary
        let gram_keyed_dictionary: BTreeMap<Gram<String>, language_utils::DictionaryEntry> = {
            let mut map = BTreeMap::new();
            for entry in &gram_frequencies {
                let gram = &entry.gram;
                if gram.len() == 1
                    && let Some(Atom::Tok(word)) = gram.first()
                    && let language_utils::WordType::Heteronym(heteronym) = &word.word_type
                    && let Some(dict_entry) = gram_dictionary.get(heteronym)
                {
                    map.insert(gram.clone(), dict_entry.clone());
                }
            }
            map
        };

        // Filter gram_vocabulary to remove learnable grams without definitions
        let defined_gram_set: std::collections::HashSet<Gram<String>> = gram_frequencies
            .iter()
            .map(|entry| entry.gram.clone())
            .collect();
        let gram_vocabulary_count_before = gram_vocabulary.len();
        let gram_vocabulary: Vec<GramVocabEntry<String>> = gram_vocabulary
            .into_iter()
            .filter(|entry| !entry.atoms.is_learnable() || defined_gram_set.contains(&entry.atoms))
            .collect();
        let removed_vocab_count = gram_vocabulary_count_before - gram_vocabulary.len();
        if removed_vocab_count > 0 {
            println!(
                "Removed {removed_vocab_count} grams from vocabulary without definitions ({gram_vocabulary_count_before} -> {})",
                gram_vocabulary.len()
            );
        }

        // Validate that all grams in gram_frequencies have definitions
        {
            let mut missing_grams = Vec::new();
            for entry in &gram_frequencies {
                let gram = &entry.gram;
                let has_definition = if gram.len() == 1 {
                    if let Some(Atom::Tok(word)) = gram.first() {
                        if let language_utils::WordType::Heteronym(heteronym) = &word.word_type {
                            gram_dictionary_set.contains(heteronym)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    phrasebook.contains_key(gram)
                };
                if !has_definition {
                    missing_grams.push(format!(
                        "Gram: {}",
                        gram.to_display_string(course.target_language)
                    ));
                }
            }

            if !missing_grams.is_empty() {
                missing_grams.sort();
                panic!(
                    "Found {} grams in gram_frequencies that don't have definitions:\n{}",
                    missing_grams.len(),
                    missing_grams.join("\n")
                );
            }
        }

        // Clean up multiword terms referencing grams that were removed from the vocabulary.
        // Omnigram-discovered multi-atom grams may have been used for NLP detection but then
        // filtered out because they lacked definitions or had too few occurrences.
        {
            let vocab_gram_set: std::collections::HashSet<&Gram<String>> =
                gram_vocabulary.iter().map(|entry| &entry.atoms).collect();
            let mut removed_high = 0usize;
            let mut removed_low = 0usize;
            for (_text, sg) in &mut encoded_sentences_with_grams {
                let before_high = sg.multiword_terms.len();
                let before_low = sg.low_confidence_multiword_terms.len();
                sg.multiword_terms
                    .retain(|term| vocab_gram_set.contains(&term.gram));
                sg.low_confidence_multiword_terms
                    .retain(|term| vocab_gram_set.contains(&term.gram));
                removed_high += before_high - sg.multiword_terms.len();
                removed_low += before_low - sg.low_confidence_multiword_terms.len();
            }
            if removed_high > 0 || removed_low > 0 {
                println!(
                    "Cleaned multiword terms: removed {removed_high} high-confidence and {removed_low} low-confidence terms referencing undefined grams"
                );
            }
        }

        // Write gram frequencies to file
        let gram_frequencies_file = target_language_dir.join("gram_frequencies.jsonl");
        generate_data::frequencies::write_gram_frequencies_file(
            &gram_frequencies,
            &gram_frequencies_file,
        )
        .context("Failed to write gram frequencies file")?;
        println!(
            "Wrote {} gram frequency entries to {:?}",
            gram_frequencies.len(),
            gram_frequencies_file
        );

        // Write a human-readable top-grams list (sorted descending, deduped by display text).
        {
            let top_grams_file = target_language_dir.join("top_grams.txt");
            let mut sorted = gram_frequencies.clone();
            sorted.sort_by_key(|entry| std::cmp::Reverse(entry.clone()));
            let mut seen = std::collections::HashSet::<String>::new();
            let mut lines = Vec::new();
            for entry in &sorted {
                let display = entry.gram.to_display_string(lang);
                if seen.insert(display.clone()) {
                    lines.push(display);
                    if lines.len() >= 200 {
                        break;
                    }
                }
            }
            std::fs::write(&top_grams_file, lines.join("\n") + "\n")
                .context("Failed to write top grams file")?;
            println!("Wrote {} top grams to {:?}", lines.len(), top_grams_file);
        }

        let wikipron_path = source_data_path
            .join("pronunciations.tsv")
            .canonicalize()
            .context("Failed to canonicalize wikipron path")?;
        let extra_pronunciations_path = source_data_path
            .join("extra_pronunciations.tsv")
            .canonicalize()
            .context("Failed to canonicalize extra pronunciations path")?;
        let word_to_pronunciation_file = target_language_dir.join("word_to_pronunciation.jsonl");
        let pronunciation_to_word_file = target_language_dir.join("pronunciation_to_words.jsonl");
        {
            // Create a set of words that appear in our frequency list for quick lookup
            let frequent_words = &frequent_heteronym_words;

            let parse_pronunciation_lines = |file: File| {
                BufReader::new(file)
                    .lines()
                    .filter_map(|line| {
                        let line = line.unwrap();
                        if line.trim().is_empty() {
                            return None;
                        }
                        let (word, ipa) = line.split_once('\t').unwrap();
                        Some((word.trim().to_lowercase(), ipa.trim().to_string()))
                    })
                    .filter(|(word, _)| frequent_words.contains(word))
                    .collect::<Vec<_>>()
            };
            let wikipron_lines = parse_pronunciation_lines(
                File::open(wikipron_path).context("Failed to open wikipron pronunciations file")?,
            );
            let extra_lines = parse_pronunciation_lines(
                File::open(extra_pronunciations_path)
                    .context("Failed to open extra pronunciations file")?,
            );

            let mut word_to_pronunciations: HashMap<String, BTreeSet<String>> = wikipron_lines
                .into_iter()
                .into_group_map()
                .into_iter()
                .map(|(word, pronunciations)| (word, pronunciations.into_iter().collect()))
                .collect();

            // Hand-curated entries always win: a word listed in
            // extra_pronunciations.tsv takes its first listed entry as the main
            // pronunciation (WikiPron entries are demoted to alternates) and
            // skips LLM selection entirely.
            let mut extra_by_word: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (word, ipa) in extra_lines {
                extra_by_word.entry(word).or_default().push(ipa);
            }
            let manual_pronunciations: BTreeMap<String, language_utils::Pronunciations> =
                extra_by_word
                    .into_iter()
                    .map(|(word, pronunciations)| {
                        let mut pronunciations = pronunciations.into_iter();
                        let main = pronunciations.next().expect("at least one pronunciation");
                        let others = pronunciations
                            .chain(word_to_pronunciations.remove(&word).into_iter().flatten())
                            .filter(|p| *p != main)
                            .unique()
                            .collect();
                        (word, language_utils::Pronunciations { main, others })
                    })
                    .collect();

            let mut word_to_pronunciation =
                generate_data::pronunciations::select_common_pronunciations(
                    *course,
                    word_to_pronunciations,
                )
                .await
                .context("Failed to select common pronunciations")?
                .into_iter()
                .collect::<BTreeMap<_, _>>();
            word_to_pronunciation.extend(manual_pronunciations);

            // Build word -> max frequency map for sorting
            let word_max_freq: std::collections::HashMap<&str, u32> = gram_frequencies
                .iter()
                .filter_map(|entry| {
                    let h = entry.gram.heteronym()?;
                    Some((h.word.as_str(), entry.count))
                })
                .into_group_map()
                .into_iter()
                .map(|(word, counts)| (word, counts.into_iter().max().unwrap_or(0)))
                .collect();

            // Reverse map uses only the main pronunciation; alternates are
            // only relevant for verification, not for display lookups.
            let pronunciation_to_words: BTreeMap<String, Vec<String>> = word_to_pronunciation
                .iter()
                .map(|(word, pronunciation)| (pronunciation.main.clone(), word.clone()))
                .into_group_map()
                .into_iter()
                .map(|(ipa, mut words)| {
                    // Sort by frequency descending, then alphabetically for ties
                    words.sort_by(|a, b| {
                        let freq_a = word_max_freq.get(a.as_str()).copied().unwrap_or(0);
                        let freq_b = word_max_freq.get(b.as_str()).copied().unwrap_or(0);
                        freq_b.cmp(&freq_a).then_with(|| a.cmp(b))
                    });
                    (ipa, words)
                })
                .collect();

            // Convert to Vec format for ConsolidatedLanguageData, sorted by frequency descending
            let mut word_to_pronunciation: Vec<(String, language_utils::Pronunciations)> =
                word_to_pronunciation.into_iter().collect();
            word_to_pronunciation.sort_by(|a, b| {
                let freq_a = word_max_freq.get(a.0.as_str()).copied().unwrap_or(0);
                let freq_b = word_max_freq.get(b.0.as_str()).copied().unwrap_or(0);
                freq_b.cmp(&freq_a).then_with(|| a.0.cmp(&b.0))
            });
            let mut pronunciation_to_words: Vec<(String, Vec<String>)> =
                pronunciation_to_words.into_iter().collect();
            pronunciation_to_words.sort_by(|a, b| {
                let freq_a =
                    a.1.first()
                        .and_then(|w| word_max_freq.get(w.as_str()).copied())
                        .unwrap_or(0);
                let freq_b =
                    b.1.first()
                        .and_then(|w| word_max_freq.get(w.as_str()).copied())
                        .unwrap_or(0);
                freq_b.cmp(&freq_a).then_with(|| a.0.cmp(&b.0))
            });

            let mut file = File::create(word_to_pronunciation_file)
                .context("Failed to create word to pronunciation file")?;
            for (word, pronunciation) in &word_to_pronunciation {
                let json = serde_json::to_string(&(word, pronunciation))
                    .context("Failed to serialize word to pronunciation entry")?;
                writeln!(file, "{json}").context("Failed to write word to pronunciation entry")?;
            }
            let mut file = File::create(pronunciation_to_word_file)
                .context("Failed to create pronunciation to words file")?;
            for (ipa, words) in &pronunciation_to_words {
                let json = serde_json::to_string(&(ipa, words))
                    .context("Failed to serialize pronunciation to words entry")?;
                writeln!(file, "{json}").context("Failed to write pronunciation to words entry")?;
            }
        }

        // Generate disambiguation practice data
        let homophones = generate_data::disambiguation_practice::generate_homophones(
            *course,
            &target_language_dir,
            &gram_frequencies,
            1000,
        )
        .context("Failed to generate homophones")?;

        // Generate homophone practice sentences
        let homophone_practice: BTreeMap<
            language_utils::HomophoneWordPair<String>,
            language_utils::HomophonePractice<String>,
        > = {
            let practice = generate_data::disambiguation_practice::generate_homophone_practice(
                *course,
                &homophones,
                &target_language_dir,
            )
            .await
            .context("Failed to generate homophone practice")?;

            let sentences = practice
                .values()
                .flat_map(|p| {
                    p.sentence_pairs
                        .iter()
                        .flat_map(|s| [s.sentence1.clone(), s.sentence2.clone()])
                })
                .collect();

            let tokenizations = generate_data::nlp::process_sentences(
                sentences,
                &target_language_dir.join("target_language_sentences_tokenization.jsonl"),
                course.target_language,
            )
            .await
            .context("Failed to process homophone practice sentences tokenization")?;

            let homophone_literals = generate_data::nlp::convert_tokens_to_literals(
                &tokenizations,
                course.target_language,
            );

            let nlp = generate_data::nlp::generate_nlp_sentences(
                &homophone_literals,
                &tokenizations,
                &contiguous_lemma_patterns,
                &discontinuous_lemma_patterns,
                &tree_patterns,
            )
            .await
            .context("Failed to generate NLP for homophone practice sentences")?;

            nlp_sentences.extend(nlp.clone());

            practice
                .into_iter()
                .map(|(pair, practice)| {
                    (
                        pair,
                        HomophonePractice {
                            sentence_pairs: practice
                                .sentence_pairs
                                .into_iter()
                                .filter(|p| {
                                    nlp_sentences.contains_key(&p.sentence1)
                                        && nlp_sentences.contains_key(&p.sentence2)
                                })
                                .collect(),
                        },
                    )
                })
                .filter(|(_, practice)| !practice.sentence_pairs.is_empty())
                .collect()
        };

        // Write all NLP sentences to file (now that we have both main and homophone sentences)
        let target_language_nlp_file =
            target_language_dir.join("target_language_sentences_nlp.jsonl");
        {
            let nlp_file = File::create(&target_language_nlp_file)
                .context("Failed to create NLP sentences file")?;
            let mut nlp_writer = BufWriter::new(nlp_file);
            for (sentence, sentence_info) in &nlp_sentences {
                let json = serde_json::to_string(&(sentence, sentence_info))
                    .context("Failed to serialize NLP sentence")?;
                writeln!(nlp_writer, "{json}").context("Failed to write NLP sentence to file")?;
            }
            nlp_writer.flush().context("Failed to flush NLP writer")?;
        }

        // Generate pronunciation sounds and guides
        let sounds_file = target_language_dir.join("pronunciation_sounds.jsonl");
        let guides_file = native_specific_dir.join("pronunciation_guides.jsonl");

        // Generate or load language sounds
        let sounds = {
            let sounds = generate_data::pronunciation_patterns::generate_language_sounds(
                course.target_language,
            )
            .await
            .context("Failed to generate language sounds")?;

            // Save to file
            let mut file =
                File::create(&sounds_file).context("Failed to create pronunciation sounds file")?;
            let json = serde_json::to_string(&sounds).context("Failed to serialize sounds")?;
            writeln!(file, "{json}").context("Failed to write sounds to file")?;

            sounds
        };

        // Generate or load pronunciation guides
        let guides = {
            let guides_with_thoughts =
                generate_data::pronunciation_patterns::generate_pronunciation_guides(
                    *course, &sounds,
                )
                .await
                .context("Failed to generate pronunciation guides")?;

            // Save to file
            let mut file =
                File::create(&guides_file).context("Failed to create pronunciation guides file")?;
            for (sound, guide_thoughts) in &guides_with_thoughts {
                let json = serde_json::to_string(&(sound, guide_thoughts))
                    .context("Failed to serialize pronunciation guide")?;
                writeln!(file, "{json}").context("Failed to write pronunciation guide to file")?;
            }

            guides_with_thoughts
        };

        // We'll calculate pattern frequencies after loading word_to_pronunciation data later

        // Consolidate all JSON files into a single rkyv file
        let rkyv_file = native_specific_dir.join("language_data.rkyv");

        // Load all the JSON files
        let target_language_sentences = {
            let file = File::open(target_language_dir.join("target_language_sentences.jsonl"))
                .context("Failed to open target language sentences file for consolidation")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<String>, _>>()
                .context("Failed to parse target language sentences for consolidation")?
        };

        let translations = {
            let file = File::open(
                native_specific_dir.join("target_language_to_native_translations.jsonl"),
            )
            .context("Failed to open translations file for consolidation")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()
                .context("Failed to parse translations for consolidation")?
        };

        // Calculate pattern frequencies using the gram frequency data
        let pattern_freq_map = generate_data::pronunciation_patterns::calculate_pattern_frequencies(
            &sounds,
            &gram_frequencies,
            course.target_language,
        );

        // Load and process phonetics data
        let word_to_pronunciation = {
            let file = File::open(target_language_dir.join("word_to_pronunciation.jsonl"))
                .context("Failed to open word to pronunciation file")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, language_utils::Pronunciations)>, _>>()
                .context("Failed to parse word to pronunciation data")?
        };

        // Sort patterns by frequency (descending)
        let mut pattern_frequencies: Vec<((String, language_utils::PatternPosition), u32)> =
            pattern_freq_map.into_iter().collect();
        pattern_frequencies.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        // Create PronunciationData with frequencies
        let pronunciation_data = language_utils::PronunciationData {
            sounds: sounds.clone(),
            guides: guides
                .into_iter()
                .map(|(_, guide_thoughts)| guide_thoughts.into())
                .collect(),
            pattern_frequencies: pattern_frequencies.clone(),
        };
        let pronunciation_to_words = {
            let file = File::open(target_language_dir.join("pronunciation_to_words.jsonl"))
                .context("Failed to open pronunciation to words file")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()
                .context("Failed to parse pronunciation to words data")?
        };

        let nlp_sentences = {
            let target_language_sentences_set = target_language_sentences
                .clone()
                .into_iter()
                .collect::<std::collections::HashSet<_>>();

            nlp_sentences
                .into_iter()
                .filter(|(sentence, _)| target_language_sentences_set.contains(sentence))
                .collect::<Vec<_>>()
        };

        // Build set of grams that have definitions (gram_frequencies was already filtered above)
        let defined_gram_set: std::collections::HashSet<Gram<String>> = gram_frequencies
            .iter()
            .map(|entry| entry.gram.clone())
            .collect();

        // Filter sentences that contain grams not in the defined set
        let (nlp_sentences, _removed_sentences): (Vec<_>, Vec<_>) = {
            // Build a map from sentence text to its encoded grams for lookup
            let sentence_to_grams: FxHashMap<&str, &SentenceGrams<Gram<String>>> =
                encoded_sentences_with_grams
                    .iter()
                    .map(|(text, grams)| (text.as_str(), grams))
                    .collect();

            nlp_sentences.into_iter().partition(|(sentence, _)| {
                // Check if all learnable grams in the encoded sentence are in the defined set
                sentence_to_grams
                    .get(sentence.as_str())
                    .map(|sg| {
                        sg.grams.iter().all(|g| match g {
                            SentenceGram::Learnable(gram) => defined_gram_set.contains(gram),
                            SentenceGram::Obvious(_) => true,
                        })
                    })
                    .unwrap_or(false)
            })
        };

        // Update target_language_sentences and translations to match filtered nlp_sentences
        let kept_sentences: std::collections::HashSet<String> = nlp_sentences
            .iter()
            .map(|(sentence, _)| sentence.clone())
            .collect();

        let mut target_language_sentences = target_language_sentences
            .into_iter()
            .filter(|sentence| kept_sentences.contains(sentence))
            .collect::<Vec<_>>();

        let translations = translations
            .into_iter()
            .filter(|(sentence, _)| kept_sentences.contains(sentence))
            .collect::<Vec<_>>();

        let encoded_sentences_with_grams = encoded_sentences_with_grams
            .into_iter()
            .filter(|(sentence, _)| kept_sentences.contains(sentence))
            .collect::<Vec<_>>();

        // Validate that all learnable grams in kept sentences have definitions
        {
            let mut missing_grams = Vec::new();
            for (sentence, _) in &nlp_sentences {
                if let Some(sg) = encoded_sentences_with_grams
                    .iter()
                    .find(|(s, _)| s == sentence)
                    .map(|(_, sg)| sg)
                {
                    for gram in &sg.grams {
                        if let SentenceGram::Learnable(g) = gram
                            && !defined_gram_set.contains(g)
                        {
                            missing_grams.push(g.to_display_string(course.target_language));
                        }
                    }
                }
            }

            if !missing_grams.is_empty() {
                missing_grams.sort();
                missing_grams.dedup();
                panic!(
                    "Found {} grams in kept sentences that don't have definitions:\n{}",
                    missing_grams.len(),
                    missing_grams.join("\n")
                );
            }
        }

        // Per-token contextual embeddings for the final sentence set, cached in
        // the osmo store (keyed per target language, so shared sentences across
        // courses embed once). Nothing consumes them yet — substrate for sense
        // discrimination. See generate_data::token_embeddings.
        generate_data::token_embeddings::ensure_token_embeddings(
            course.target_language,
            &nlp_sentences,
            &generate_data::cache_remote::store(),
            &reqwest::Client::new(),
        )
        .await
        .context("Failed to ensure token embeddings")?;

        let (pronunciation_to_words, word_to_pronunciation) = {
            let words_set = gram_frequencies
                .iter()
                .filter_map(|entry| entry.gram.heteronym())
                .map(|h| h.word.clone())
                .collect::<std::collections::HashSet<_>>();
            let pronunciation_to_words = pronunciation_to_words
                .into_iter()
                .map(|(ipa, words)| {
                    (
                        ipa,
                        words
                            .into_iter()
                            .filter(|word| words_set.contains(word))
                            .collect::<Vec<_>>(),
                    )
                })
                .filter(|(_, words)| !words.is_empty())
                .collect::<Vec<_>>();
            let word_to_pronunciation = word_to_pronunciation
                .into_iter()
                .filter(|(word, _)| words_set.contains(word))
                .collect::<Vec<_>>();
            (pronunciation_to_words, word_to_pronunciation)
        };

        // Detect minimal pairs once over the FINAL filtered word_to_pronunciation
        // so the pack's index can't reference words that have been dropped from
        // word_to_pronunciation by the filter above.
        let minimal_pair_groups = {
            let word_max_freq: std::collections::HashMap<&str, u32> = gram_frequencies
                .iter()
                .filter_map(|entry| {
                    let h = entry.gram.heteronym()?;
                    Some((h.word.as_str(), entry.count))
                })
                .into_group_map()
                .into_iter()
                .map(|(word, counts)| (word, counts.into_iter().max().unwrap_or(0)))
                .collect();
            // Minimal pairs use only the main pronunciation per word.
            let word_to_main: Vec<(String, String)> = word_to_pronunciation
                .iter()
                .map(|(w, p)| (w.clone(), p.main.clone()))
                .collect();
            let groups =
                language_utils::minimal_pairs::find_minimal_pairs(&word_to_main, &word_max_freq);
            let minimal_pairs_file = target_language_dir.join("minimal_pairs.jsonl");
            let mut file =
                File::create(&minimal_pairs_file).context("Failed to create minimal pairs file")?;
            for group in &groups {
                let json = serde_json::to_string(group)
                    .context("Failed to serialize minimal pair group")?;
                writeln!(file, "{json}").context("Failed to write minimal pair group")?;
            }
            let total_pairs: usize = groups.iter().map(|g| g.pairs.len()).sum();
            println!(
                "Wrote {} minimal pair groups ({} pairs total) to {:?}",
                groups.len(),
                total_pairs,
                minimal_pairs_file
            );
            groups
        };

        // Sort sentences by the frequency of their least common gram

        // Create a gram frequency map for quick lookup
        let gram_frequency_map: FxHashMap<&Gram<String>, u32> = gram_frequencies
            .iter()
            .map(|entry| (&entry.gram, entry.count))
            .collect();

        // Create a map from sentence to its encoded grams for quick lookup
        let sentence_to_grams: FxHashMap<&str, &SentenceGrams<Gram<String>>> =
            encoded_sentences_with_grams
                .iter()
                .map(|(text, grams)| (text.as_str(), grams))
                .collect();

        // Sort target_language_sentences by the frequency of their least common gram
        target_language_sentences.sort_by_cached_key(|sentence| {
            if let Some(sg) = sentence_to_grams.get(sentence.as_str()) {
                let mut freqs: Vec<u32> = sg
                    .grams
                    .iter()
                    .filter_map(|g| match g {
                        SentenceGram::Learnable(gram) => gram_frequency_map.get(gram).copied(),
                        SentenceGram::Obvious(_) => None,
                    })
                    .collect();
                freqs.sort_unstable();
                let mut it = freqs.into_iter();
                std::cmp::Reverse((it.next(), it.next(), it.next()))
            } else {
                eprintln!("No encoded grams found for sentence: {sentence}");
                std::cmp::Reverse((None, None, None))
            }
        });

        // Load movie metadata and subtitles
        let source_data_path = std::path::PathBuf::from(format!(
            "./generate-data/data/{}",
            course.target_language.code()
        ));
        // Load movie metadata
        let movies_dir = source_data_path.join("sentence-sources/movies");
        let movies = if movies_dir.exists() {
            let metadata_file = movies_dir.join("metadata.jsonl");
            if metadata_file.exists() {
                let metadata_content = std::fs::read_to_string(&metadata_file)
                    .context("Failed to read movie metadata file")?;
                let posters_dir = movies_dir.join("posters");
                let mut movies = FxHashMap::default();

                for line in metadata_content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let basic: language_utils::MovieMetadataBasic =
                        serde_json::from_str(line).context("Failed to parse movie metadata")?;

                    // Convert to full MovieMetadata and load poster bytes from separate file
                    let mut movie: language_utils::MovieMetadata = basic.into();
                    let poster_path = posters_dir.join(format!("{}.jpg", movie.id));
                    if poster_path.exists()
                        && let Ok(bytes) = std::fs::read(&poster_path)
                    {
                        // Resize and encode as lossy WebP for smaller file size
                        match image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
                        {
                            Ok(img) => {
                                let resized =
                                    img.resize(400, 600, image::imageops::FilterType::Lanczos3);
                                let resized = image::DynamicImage::ImageRgb8(resized.to_rgb8());
                                let encoder = webp::Encoder::from_image(&resized)
                                    .expect("Failed to create WebP encoder");
                                let mut config =
                                    webp::WebPConfig::new().expect("Failed to create WebP config");
                                config.quality = 40.0;
                                config.method = 6;
                                movie.poster_bytes = Some(
                                    encoder
                                        .encode_advanced(&config)
                                        .expect("Failed to encode WebP")
                                        .to_vec(),
                                );
                            }
                            Err(_) => {
                                movie.poster_bytes = Some(bytes);
                            }
                        }
                    }

                    movies.insert(movie.id.clone(), movie);
                }

                movies
            } else {
                FxHashMap::default()
            }
        } else {
            FxHashMap::default()
        };

        // Load book metadata (attribution for book-sourced sentences, incl. the
        // machine-translated flag) from each series folder's metadata.jsonl
        let books = {
            let books_dir = source_data_path.join("sentence-sources/books");
            let mut books = FxHashMap::default();
            if books_dir.exists() {
                for series_entry in
                    std::fs::read_dir(&books_dir).context("Failed to read books directory")?
                {
                    let series_dir = series_entry?.path();
                    let metadata_file = series_dir.join("metadata.jsonl");
                    if !series_dir.is_dir() || !metadata_file.exists() {
                        continue;
                    }
                    let content = std::fs::read_to_string(&metadata_file)
                        .context("Failed to read book metadata file")?;
                    for line in content.lines().filter(|l| !l.trim().is_empty()) {
                        let book: language_utils::BookMetadata =
                            serde_json::from_str(line).context("Failed to parse book metadata")?;
                        anyhow::ensure!(
                            series_dir
                                .file_name()
                                .is_some_and(|n| *n == *book.series.as_str()),
                            "book {:?} declares series {:?} but lives in {}",
                            book.id,
                            book.series,
                            series_dir.display()
                        );
                        books.insert(book.id.clone(), book);
                    }
                }
            }
            books
        };
        // Load sentence sources
        let sentence_sources = {
            let sentence_sources_file = target_language_dir.join("sentence_sources.jsonl");
            if sentence_sources_file.exists() {
                let file = File::open(&sentence_sources_file)
                    .context("Failed to open sentence sources file")?;
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .map(|line| serde_json::from_str(&line.unwrap()))
                    .collect::<Result<Vec<(String, language_utils::SentenceSource)>, _>>()
                    .context("Failed to parse sentence sources")?
            } else {
                Vec::new()
            }
        };
        for (_, source) in &sentence_sources {
            for book_id in &source.book_ids {
                anyhow::ensure!(
                    books.contains_key(book_id),
                    "sentence source references book {book_id:?} with no entry in \
                     sentence-sources/books/metadata.jsonl"
                );
            }
        }

        // Build sentence_to_sources map for per-source frequency computation
        let master_gram_set: std::collections::HashSet<Gram<String>> = gram_frequencies
            .iter()
            .map(|entry| entry.gram.clone())
            .collect();

        let mut sentence_to_sources: rustc_hash::FxHashMap<
            String,
            Vec<language_utils::FrequencySourceId>,
        > = rustc_hash::FxHashMap::default();
        // Add movie sources
        for (sentence, source) in &sentence_sources {
            for movie_id in &source.movie_ids {
                sentence_to_sources
                    .entry(sentence.clone())
                    .or_default()
                    .push(language_utils::FrequencySourceId::Movie(movie_id.clone()));
            }
        }
        // Add Pimsleur lesson sources
        for (sentence, lessons) in &restricted_sentences {
            for lesson in lessons {
                sentence_to_sources
                    .entry(sentence.clone())
                    .or_default()
                    .push(language_utils::FrequencySourceId::PimsleurLesson(
                        language_utils::PimsleurLesson {
                            level: lesson.level,
                            lesson: lesson.lesson,
                        },
                    ));
            }
        }

        // Compute per-source frequencies from UNFILTERED sentences (for accurate totals).
        // We need all_encoded_sentences here because restricted sentences are only in that set.
        let unfiltered_source_freqs =
            generate_data::frequencies::compute_per_source_gram_frequencies(
                &unfiltered_encoded_sentences,
                &sentence_to_sources,
                &gram_vocabulary,
            );

        // Compute total counts per source BEFORE filtering, then filter entries
        let source_gram_frequencies: rustc_hash::FxHashMap<
            language_utils::FrequencySourceId,
            language_utils::GramFrequencyList,
        > = unfiltered_source_freqs
            .into_iter()
            .map(|(source_id, freqs)| {
                let total_count: u64 = freqs.iter().map(|e| e.count as u64).sum();
                let filtered_entries: Vec<GramFrequencyEntry<String>> = freqs
                    .into_iter()
                    .filter(|entry| master_gram_set.contains(&entry.gram))
                    .collect();
                (
                    source_id,
                    language_utils::GramFrequencyList {
                        entries: filtered_entries,
                        total_count,
                    },
                )
            })
            .filter(|(_, list)| !list.entries.is_empty())
            .collect();

        // Generate landing page showcase data
        {
            let translations_map: FxHashMap<&str, &[String]> = translations
                .iter()
                .map(|(s, t)| (s.as_str(), t.as_slice()))
                .collect();

            let lang = course.target_language;

            // Collect candidate phrases: prefer multi-word, sorted by frequency
            let mut showcase_phrases = Vec::new();
            for entry in &gram_frequencies {
                if showcase_phrases.len() >= 10 {
                    break;
                }
                let gram = &entry.gram;

                // Get definition
                let definition = if gram.len() > 1 {
                    phrasebook.get(gram).map(|p| p.meaning.clone())
                } else if let Some(dict_entry) = gram_keyed_dictionary.get(gram) {
                    dict_entry.definitions.first().map(|d| d.native.clone())
                } else {
                    None
                };
                let Some(definition) = definition else {
                    continue;
                };

                // Get example sentences with translations
                let Some(sentences) = gram_sentences.get(gram) else {
                    continue;
                };
                let examples: Vec<language_utils::ShowcaseExampleSentence> = sentences
                    .iter()
                    .filter_map(|s| {
                        let native = translations_map.get(s.as_str())?.first()?;
                        Some(language_utils::ShowcaseExampleSentence {
                            target: s.clone(),
                            native: native.clone(),
                        })
                    })
                    .take(3)
                    .collect();

                if examples.len() < 2 {
                    continue;
                }

                showcase_phrases.push(language_utils::ShowcasePhrase {
                    display_text: gram.to_display_string(lang),
                    definition,
                    examples,
                });
            }

            let showcase = language_utils::CourseShowcase {
                target_language: course.target_language,
                native_language: course.native_language,
                sentence_count: target_language_sentences.len(),
                phrases: showcase_phrases,
            };

            let showcase_json =
                serde_json::to_string(&showcase).context("Failed to serialize showcase data")?;
            std::fs::write(native_specific_dir.join("showcase.json"), showcase_json)
                .context("Failed to write showcase.json")?;
            println!(
                "Wrote showcase.json with {} phrases for {:?}",
                showcase.phrases.len(),
                course
            );
        }

        let audio_failures_log = target_language_dir.join("audio_verification_failures.jsonl");
        let audio_all_results_log = target_language_dir.join("audio_verification_all.jsonl");
        let http_client = reqwest::Client::new();
        let human_audio = generate_data::human_audio::load_human_audio(
            &source_data_path,
            &word_to_pronunciation,
            &audio_failures_log,
            &audio_all_results_log,
            course.target_language,
            &http_client,
        )
        .await
        .with_context(|| format!("Failed to load human audio for {course:?}"))?;
        if !human_audio.is_empty() {
            let total_clips: usize = human_audio.values().map(|clips| clips.len()).sum();
            println!(
                "Loaded {total_clips} human audio clips from {} voice actor(s)",
                human_audio.len()
            );
        }

        // Create consolidated data structure
        let consolidated_data = language_utils::ConsolidatedLanguageData {
            target_language_sentences,
            translations,
            nlp_sentences,
            phrasebook,
            proper_noun_definitions,
            source_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            minimal_pairs: minimal_pair_groups,
            pronunciation_data,
            homophone_practice,
            movies,
            books,
            sentence_sources,
            gram_vocabulary,
            gram_frequencies: language_utils::GramFrequencyList {
                entries: gram_frequencies,
                total_count: master_total_count,
            },
            encoded_sentences: encoded_sentences_with_grams,
            gram_dictionary: gram_keyed_dictionary,
            morphemes,
            human_audio,
        };

        let language_pack =
            language_utils::language_pack::LanguagePack::new(consolidated_data, *course);

        // Serialize with rkyv
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&language_pack)
            .context("Failed to serialize language pack with rkyv")?;
        std::fs::write(&rkyv_file, bytes).context("Failed to write rkyv file")?;

        // Generate hash of the rkyv file
        let hash_file = native_specific_dir.join("language_data.hash");

        // Read the rkyv file and compute hash
        let rkyv_bytes =
            std::fs::read(&rkyv_file).context("Failed to read rkyv file for hashing")?;
        let hash = const_xxh3(&rkyv_bytes);
        let size = rkyv_bytes.len();

        // Write hash and size to file in format: hash;size_in_bytes
        std::fs::write(&hash_file, format!("{hash};{size}"))
            .context("Failed to write hash file")?;
    }

    // Push any new cache entries (LLM responses, translations, TTS, …) to the bucket.
    cache_remote::flush().await;

    Ok(())
}
