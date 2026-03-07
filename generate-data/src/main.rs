use anyhow::Context;
use futures::StreamExt;
use indexmap::IndexSet;
use itertools::Itertools;
use language_utils::{
    Atom, COURSES, EncodedSentence, Gram, GramFrequencyEntry, GramVocabEntry, HomophonePractice,
    SentenceGram, SentenceGrams,
};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::hash::Hash;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

mod google_translate;
use google_translate::GoogleTranslator;

use generate_data::morphology_analysis;

struct PhraseDetectionData {
    tokens: Option<Vec<lexide::Token>>, // we don't have this for grams
}
type PhraseDetectionDataMap = BTreeMap<Gram<String>, PhraseDetectionData>;

/// Deduplicates a pattern map where multiple grams may produce the same matcher pattern.
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

    // Optional: filter to a specific target language (e.g. "deu", "fra", "spa")
    let lang_filter = std::env::args().nth(1);

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
        if let Some(ref filter) = lang_filter {
            if course.target_language.iso_639_3() != filter {
                continue;
            }
        }

        println!();
        println!();
        println!(
            "Processing course: {} -> {}",
            course.native_language, course.target_language
        );
        println!("================================================");

        let target_language_dir =
            PathBuf::from(format!("./out/{}", course.target_language.iso_639_3()));
        std::fs::create_dir_all(&target_language_dir)
            .context("Failed to create target language directory")?;
        let target_language_dir = target_language_dir
            .canonicalize()
            .context("Failed to canonicalize target language output directory")?;

        let native_specific_dir = PathBuf::from(format!(
            "./out/{}_for_{}",
            course.target_language.iso_639_3(),
            course.native_language.iso_639_3()
        ));
        std::fs::create_dir_all(&native_specific_dir)
            .context("Failed to create native-specific directory")?;
        let native_specific_dir = native_specific_dir
            .canonicalize()
            .context("Failed to canonicalize native-specific output directory")?;

        let source_data_path = format!(
            "./generate-data/data/{}",
            course.target_language.iso_639_3()
        );
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
        {
            let mut total_sentences = 0;

            // Get target sentences with their existing translations (from Anki, Tatoeba, and manual sources)
            let sentences_with_translations_and_sources =
                generate_data::target_sentences::get_target_sentences(*course)
                    .context("Failed to get target sentences")?;

            // Create the translator once and share it across all async tasks
            let translator = GoogleTranslator::new(
                course.target_language, // translate from target to native
                course.native_language,
                PathBuf::from(".cache/google_translate/"),
            )
            .await
            .context("Failed to create Google Translator")?;

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
                        (target_language_sentence, (translation_set, source))
                    },
                ))
                .buffered(100)
                .collect::<BTreeMap<_, _>>()
                .await;

            // Drop the translator to trigger the Drop implementation
            drop(translator);

            let target_language_file = match File::create(target_language_sentences_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Error creating target_language sentences file: {}",
                        e
                    ));
                }
            };
            let mut target_language_writer = BufWriter::new(target_language_file);

            let translations_file_handle = match File::create(translations_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!("Error creating translations file: {}", e));
                }
            };
            let mut translations_writer = BufWriter::new(translations_file_handle);

            let sentence_sources_file_handle = match File::create(sentence_sources_file.clone()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Error creating sentence sources file: {}",
                        e
                    ));
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

        // Train supertokens and write whitespace diagnostics
        generate_data::tokenize::train_supertokens_and_write_diagnostics(
            &sentence_literals,
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

        // Pre-compute matcher data from phrase detection map
        let lang = course.target_language;
        let lemma_patterns: BTreeMap<Gram<String>, Vec<String>> = phrase_detection_map
            .iter()
            .filter_map(|(gram, data)| {
                // If we have lexide tokens, use their lemmas
                if let Some(tokens) = data.tokens.as_ref() {
                    if tokens.len() <= 1 {
                        return None;
                    }
                    return Some((
                        gram.clone(),
                        tokens.iter().map(|t| t.lemma.lemma.clone()).collect(),
                    ));
                }
                // For omnigram-discovered grams without lexide tokens,
                // build lemma patterns from the gram's atoms directly
                if gram.len() <= 1 {
                    return None;
                }
                let lemmas: Vec<String> = gram
                    .iter()
                    .filter_map(|atom| match atom {
                        language_utils::Atom::Tok(word) => match &word.word_type {
                            language_utils::WordType::Heteronym(h) => Some(h.lemma.clone()),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect();
                // Only include if we got a lemma for every atom
                if lemmas.len() == gram.len() {
                    Some((gram.clone(), lemmas))
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
        let discontinuous_lemma_patterns: BTreeMap<Gram<String>, Vec<String>> = lemma_patterns
            .iter()
            .filter(|(gram, _)| discontinuous_grams.contains(gram))
            .map(|(gram, lemmas)| (gram.clone(), lemmas.clone()))
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

        // Convert encoded sentences to use SentenceGram<Gram<String>> instead of token IDs
        let encoded_sentences_with_grams: Vec<(String, SentenceGrams<Gram<String>>)> =
            encoded_sentences
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
                    // Collect grams already in the encoded sentence so we can
                    // skip listing them again as multiword terms.
                    let sentence_gram_set: std::collections::HashSet<&Gram<String>> = grams
                        .iter()
                        .map(|sg| match sg {
                            SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
                        })
                        .collect();
                    let (multiword_terms, low_confidence_multiword_terms) = nlp_sentences
                        .get(text)
                        .map(|info| {
                            (
                                info.multiword_terms
                                    .high_confidence
                                    .iter()
                                    .filter(|g| !sentence_gram_set.contains(g))
                                    .cloned()
                                    .collect(),
                                info.multiword_terms
                                    .low_confidence
                                    .iter()
                                    .filter(|g| !sentence_gram_set.contains(g))
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
                .collect();

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

        // Filter encoded sentences to only include those where we have all the learnable grams
        let encoded_sentences_count_before = encoded_sentences_with_grams.len();
        let encoded_sentences_with_grams: Vec<(String, SentenceGrams<Gram<String>>)> =
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
                        if is_monosemantic {
                            if let Some(sentences) = old_format.get(&display_text) {
                                migrated.insert(entry.gram.clone(), sentences.clone());
                            }
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
            let file = File::open(source_data_path.join("custom_definitions.jsonl"))
                .context("Failed to open custom definitions file")?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| line.unwrap())
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
            let morphology = morphology_analysis::create_morphology(
                course.target_language,
                &filtered_gram_frequencies,
            )
            .await
            .context("Failed to create morphology")?;
            raw_dictionary
                .into_iter()
                .filter_map(|(heteronym, mut def)| {
                    // Apply custom definitions if available
                    if let Some(custom_def) = custom_definitions.get(&heteronym) {
                        def = custom_def.clone();
                    }
                    // Only keep entries that have morphology
                    let morph = morphology.get(&heteronym)?.clone();
                    Some((
                        heteronym,
                        language_utils::DictionaryEntry {
                            target_language_word: def.target_language_word,
                            definitions: def.definitions,
                            morphology: morph,
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
                {
                    if let Some(dict_entry) = gram_dictionary.get(heteronym) {
                        map.insert(gram.clone(), dict_entry.clone());
                    }
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

        // Write gram frequencies to file
        let gram_frequencies_file = native_specific_dir.join("gram_frequencies.jsonl");
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

            let phonetics_file =
                File::open(wikipron_path).context("Failed to open wikipron pronunciations file")?;
            let phonetics_file = BufReader::new(phonetics_file);
            let extra_phonetics_file = File::open(extra_pronunciations_path)
                .context("Failed to open extra pronunciations file")?;
            let extra_phonetics_file = BufReader::new(extra_phonetics_file);
            let word_to_pronunciations = phonetics_file
                .lines()
                .chain(extra_phonetics_file.lines())
                .filter_map(|line| {
                    let line = line.unwrap();
                    if line.trim().is_empty() {
                        return None;
                    }
                    let (word, ipa) = line.split_once('\t').unwrap();
                    let word = word.trim().to_lowercase();
                    let ipa = ipa.trim().to_string();
                    Some((word, ipa))
                })
                .filter(|(word, _)| frequent_words.contains(word))
                .into_group_map()
                .into_iter()
                .map(|(word, pronunciations)| (word, pronunciations.into_iter().collect()))
                .collect();
            let word_to_pronunciation =
                generate_data::pronunciations::select_common_pronunciations(
                    *course,
                    word_to_pronunciations,
                )
                .await
                .context("Failed to select common pronunciations")?
                .into_iter()
                .collect::<BTreeMap<_, _>>();

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

            let pronunciation_to_words: BTreeMap<String, Vec<String>> = word_to_pronunciation
                .iter()
                .map(|(word, pronunciation)| (pronunciation.clone(), word.clone()))
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
            let mut word_to_pronunciation: Vec<(String, String)> =
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
                .collect::<Result<Vec<(String, String)>, _>>()
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
            course.target_language.iso_639_3()
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
                        movie.poster_bytes = Some(bytes);
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

        // Compute per-movie frequencies
        let movie_gram_frequencies = if !movies.is_empty() {
            let movie_ids: Vec<String> = movies.keys().cloned().collect();
            let movie_gram_frequencies = generate_data::frequencies::compute_movie_gram_frequencies(
                &encoded_sentences_with_grams,
                &sentence_sources,
                &movie_ids,
                &gram_vocabulary,
            );

            // Filter movie_gram_frequencies to only include grams that have definitions
            let movie_gram_frequencies: FxHashMap<String, Vec<GramFrequencyEntry<String>>> =
                movie_gram_frequencies
                    .into_iter()
                    .map(|(movie_id, freqs)| {
                        let filtered_freqs: Vec<GramFrequencyEntry<String>> = freqs
                            .into_iter()
                            .filter(|entry| {
                                let gram = &entry.gram;
                                if gram.len() == 1 {
                                    if let Some(Atom::Tok(word)) = gram.first()
                                        && let language_utils::WordType::Heteronym(heteronym) =
                                            &word.word_type
                                    {
                                        return gram_dictionary_set.contains(heteronym);
                                    }
                                    false
                                } else {
                                    phrasebook.contains_key(gram)
                                }
                            })
                            .collect();
                        (movie_id, filtered_freqs)
                    })
                    .filter(|(_, freqs)| !freqs.is_empty())
                    .collect();

            movie_gram_frequencies
        } else {
            FxHashMap::default()
        };

        // Create consolidated data structure
        let consolidated_data = language_utils::ConsolidatedLanguageData {
            target_language_sentences,
            translations,
            nlp_sentences,
            phrasebook,
            proper_noun_definitions,
            movie_gram_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            pronunciation_data,
            homophone_practice,
            movies,
            sentence_sources,
            gram_vocabulary,
            gram_frequencies,
            encoded_sentences: encoded_sentences_with_grams,
            gram_dictionary: gram_keyed_dictionary,
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

    Ok(())
}
