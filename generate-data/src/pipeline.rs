//! The per-course corpus pipeline, shared between the main generate-data
//! binary and the standalone `sense_discovery` tool. Two independent
//! branches hang off `target_sentences::get_target_sentences`:
//!
//! - [`segment_corpus`]: tokenization → multiword-term patterns → unigram
//!   encoding → multiword matching → slot-loosened matching. Produces the
//!   segmented corpus ([`SegmentedCorpus`]) entirely in memory; the files it
//!   writes (sentence list, sources, vocabulary, encodings, diagnostics) are
//!   pure outputs, never re-read in the same run.
//! - [`translate_sentences`]: the translation pass, returning the
//!   translations map (and writing the translations file as an output).
//!
//! Segmentation never touches the translator, so tools that only need the
//! segmented corpus (sense discovery, embeddings) run without translation
//! credentials or cost. Every inner step is incremental (tokenization files
//! are load-or-extend caches, LLM calls hit the tysm cache), so a warm
//! invocation is fast and a cold one is simply the pipeline doing its
//! normal work.

use anyhow::Context;
use indexmap::IndexSet;
use language_utils::{
    Course, Gram, GramFrequencyEntry, GramVocabEntry, MultiwordTermMatch, MultiwordTerms,
    SentenceInfo, SentenceSource,
};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::hash::Hash;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::cache_remote;
use crate::target_sentences::TargetSentences;
use crate::tokenize::{SentenceEncoder, TrainedEncoding};
use crate::translate::{TranslationBackend, Translator};
use language_utils::GramInterners;

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

/// A course's two output directories, created and canonicalized — a pure
/// derivation of the course.
pub struct CourseDirs {
    pub target_language_dir: PathBuf,
    pub native_specific_dir: PathBuf,
}

pub fn course_dirs(course: &Course) -> anyhow::Result<CourseDirs> {
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

    Ok(CourseDirs {
        target_language_dir,
        native_specific_dir,
    })
}

/// The banned-words list for a course's target language (empty if the file
/// doesn't exist).
pub fn load_banned_words(
    course: &Course,
) -> anyhow::Result<HashSet<language_utils::Heteronym<String>>> {
    let path = format!(
        "./generate-data/data/{}/banned_words.jsonl",
        course.target_language.code()
    );
    let path = Path::new(&path);
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let content = std::fs::read_to_string(path).context("Failed to read banned words file")?;
    Ok(content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<language_utils::Heteronym<String>>(line).unwrap())
        .collect())
}

/// Initial gram frequencies from the vocabulary, for filtering (the full
/// gram+phrase frequencies are computed after filtering).
pub fn initial_gram_frequencies(
    gram_vocabulary: &[GramVocabEntry<String>],
) -> Vec<GramFrequencyEntry<String>> {
    let mut frequencies: Vec<GramFrequencyEntry<String>> = gram_vocabulary
        .iter()
        .filter(|entry| entry.atoms.is_learnable())
        .map(|entry| GramFrequencyEntry {
            count: entry.frequency,
            direct_count: entry.frequency,
            disambiguation_key: entry.atoms.disambiguation_key(),
            gram: entry.atoms.clone(),
        })
        .collect();
    frequencies.sort_by_key(|entry| std::cmp::Reverse(entry.clone()));
    frequencies
}

/// The matcher patterns compiled from multiword terms and the gram
/// vocabulary (also used by `main()` to run multiword detection over
/// secondary sentence sets like homophone practice).
pub struct MatchingPatterns {
    pub contiguous_lemma_patterns: BTreeMap<Gram<String>, Vec<(String, lexide::pos::PartOfSpeech)>>,
    pub discontinuous_lemma_patterns:
        BTreeMap<Gram<String>, Vec<(String, lexide::pos::PartOfSpeech)>>,
    pub tree_patterns: BTreeMap<Gram<String>, lexide::matching::TreeNode>,
}

/// The segmented corpus: every sentence in its canonical encoded form with
/// its multiword matches, the vocabulary that decodes it, and the machinery
/// to segment new sentences the same way.
pub struct SegmentedCorpus {
    /// App sentences (multiword-matched).
    pub nlp_sentences: BTreeMap<String, SentenceInfo>,
    /// Restricted (Pimsleur) sentences: encoded, but never multiword-matched.
    pub restricted_nlp_sentences: BTreeMap<String, SentenceInfo>,
    /// Gram vocabulary; index = encoded token key's `into_usize()`.
    pub gram_vocabulary: Vec<GramVocabEntry<String>>,
    /// The interners behind the vocabulary (decode goes through these).
    pub interners: GramInterners,
    pub patterns: MatchingPatterns,
    /// Encoder for sentences minted after training (homophone practice).
    pub encoder: SentenceEncoder,
}

/// Build the segmented corpus for a course. No translation happens here; the
/// sentence set is every sentence_corpus app sentence (deduplicated by text).
pub async fn segment_corpus(
    course: &Course,
    sentence_corpus: &TargetSentences,
) -> anyhow::Result<SegmentedCorpus> {
    let course = *course;
    let CourseDirs {
        target_language_dir,
        ..
    } = course_dirs(&course)?;
    let banned_words = load_banned_words(&course)?;

    // The sentence set: sentence_corpus app sentences, deduplicated by text (last
    // source wins, matching the old BTreeMap-collect behavior). Written out
    // as pure outputs — the in-memory maps are the source of truth below.
    let sources_by_text: BTreeMap<&str, &SentenceSource> = sentence_corpus
        .app_sentences
        .iter()
        .map(|(text, _, source)| (text.as_str(), source))
        .collect();
    if sources_by_text.len() < 10 {
        panic!("Too few sentences: {}", sources_by_text.len());
    }
    {
        let file = File::create(target_language_dir.join("target_language_sentences.jsonl"))
            .context("Failed to create target language sentences file")?;
        let mut writer = BufWriter::new(file);
        for text in sources_by_text.keys() {
            writeln!(writer, "{}", serde_json::to_string(text)?)?;
        }
        writer.flush()?;

        let file = File::create(target_language_dir.join("sentence_sources.jsonl"))
            .context("Failed to create sentence sources file")?;
        let mut writer = BufWriter::new(file);
        for (text, source) in &sources_by_text {
            writeln!(writer, "{}", serde_json::to_string(&(text, source))?)?;
        }
        writer.flush()?;
    }

    // Tokenize (incremental cache file: already-processed sentences load).
    let sentences: Vec<String> = sources_by_text.keys().map(|s| s.to_string()).collect();
    let sentences_tokenizations = crate::nlp::process_sentences(
        sentences,
        &target_language_dir.join("target_language_sentences_tokenization.jsonl"),
        course.target_language,
    )
    .await
    .context("Failed to process sentences tokenization")?;

    // Convert tokenizations to literals (without multiword detection)
    // Multiword detection will happen later, after omnigram training
    let sentence_literals =
        crate::nlp::convert_tokens_to_literals(&sentences_tokenizations, course.target_language);

    // Filter out sentences containing banned words before gram processing
    let sentence_literals: BTreeMap<String, Vec<language_utils::Literal<String>>> =
        sentence_literals
            .into_iter()
            .filter(|(_, words)| {
                !words.iter().any(|word| {
                    word.heteronym()
                        .map(|h| banned_words.contains(h))
                        .unwrap_or(false)
                })
            })
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
    let restricted_sentence_texts: Vec<String> = sentence_corpus
        .restricted_sentences
        .iter()
        .map(|(s, _)| s.clone())
        .collect();
    let restricted_tokenizations = if !restricted_sentence_texts.is_empty() {
        crate::nlp::process_sentences(
            restricted_sentence_texts,
            &target_language_dir.join("restricted_sentences_tokenization.jsonl"),
            course.target_language,
        )
        .await
        .context("Failed to process restricted sentences tokenization")?
    } else {
        BTreeMap::new()
    };
    let restricted_literals =
        crate::nlp::convert_tokens_to_literals(&restricted_tokenizations, course.target_language);

    // Merge restricted literals into sentence_literals for omnigram training.
    // BTreeMap insert means duplicates (sentences in both sets) are naturally handled.
    let mut all_sentence_literals = sentence_literals.clone();
    for (text, lits) in &restricted_literals {
        all_sentence_literals
            .entry(text.clone())
            .or_insert_with(|| lits.clone());
    }

    // Multiword terms (the file is a load-or-build cache) and their
    // tokenizations.
    let multiword_terms =
        crate::wiktionary_terms::ensure_multiword_terms_file(&course, &target_language_dir)
            .await
            .context("Failed to ensure multiword terms file exists")?;
    let wiktionary_alt_forms =
        crate::wiktionary_terms::download_alt_forms(&multiword_terms, &target_language_dir)
            .await
            .context("Failed to download alt forms")?;
    let multiword_terms_tokenizations = crate::nlp::process_sentences(
        multiword_terms,
        &target_language_dir.join("target_language_multiword_terms_tokenization.jsonl"),
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

    // Convert multiword term tokenizations to grams for seeding into omnigram
    let multiword_term_literals = crate::nlp::convert_tokens_to_literals(
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

    // Train supertokens; the vocabulary, every sentence's encoding, and the
    // encoder come back in memory (the files are pure outputs).
    let TrainedEncoding {
        gram_vocabulary,
        interners,
        encoded_sentences,
        encoder,
    } = crate::tokenize::train_supertokens_and_write_diagnostics(
        &all_sentence_literals,
        course.target_language,
        &target_language_dir,
        &seed_grams,
    );

    // Get set of terms that are known to be discontinuous (had "..." in source files)
    let discontinuous_terms = crate::wiktionary_terms::get_discontinuous_terms(&course);

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

    let tree_patterns: BTreeMap<Gram<String>, lexide::matching::TreeNode> = phrase_detection_map
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
    let mut multiword_matches = crate::nlp::generate_nlp_sentences(
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
        use crate::slot_analysis::{self, SlotRealization};

        let slot_specs = slot_analysis::analyze_slots(&course, &multiword_terms_tokenizations)
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
                let Some(terms) = multiword_matches.get_mut(&slot_match.sentence) else {
                    continue;
                };
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

    // Assemble the corpus: every filtered sentence in its canonical encoded
    // form, paired with its matches.
    let empty_terms = || MultiwordTerms {
        high_confidence: Vec::new(),
        low_confidence: Vec::new(),
    };
    let assemble =
        |texts: &mut dyn Iterator<Item = &String>,
         matches: &mut BTreeMap<String, MultiwordTerms<MultiwordTermMatch<Gram<String>>>>|
         -> BTreeMap<String, SentenceInfo> {
            let mut out = BTreeMap::new();
            let mut unencoded = 0usize;
            for text in texts {
                let Some(sentence) = encoded_sentences.get(text) else {
                    unencoded += 1;
                    continue;
                };
                out.insert(
                    text.clone(),
                    SentenceInfo {
                        sentence: sentence.clone(),
                        multiword_terms: matches.remove(text).unwrap_or_else(empty_terms),
                    },
                );
            }
            if unencoded > 0 {
                log::warn!("{unencoded} sentences had no encoding and were dropped");
            }
            out
        };
    let nlp_sentences = assemble(&mut sentence_literals.keys(), &mut multiword_matches);
    let restricted_nlp_sentences = assemble(
        &mut restricted_literals
            .keys()
            .filter(|k| !sentence_literals.contains_key(*k)),
        &mut BTreeMap::new(),
    );
    println!(
        "Segmented corpus: {} app sentences, {} restricted-only",
        nlp_sentences.len(),
        restricted_nlp_sentences.len(),
    );

    Ok(SegmentedCorpus {
        nlp_sentences,
        restricted_nlp_sentences,
        gram_vocabulary,
        interners,
        patterns: MatchingPatterns {
            contiguous_lemma_patterns,
            discontinuous_lemma_patterns,
            tree_patterns,
        },
        encoder,
    })
}

/// Translate every sentence_corpus app sentence into the course's native language
/// (osmo-cached; primed in batches first). Returns text → translations —
/// sentences with no usable translation are absent from the map, which is
/// how downstream pack assembly excludes them. Also writes the translations
/// file as a pure output.
pub async fn translate_sentences(
    course: &Course,
    sentence_corpus: &TargetSentences,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let course = *course;
    let CourseDirs {
        native_specific_dir,
        ..
    } = course_dirs(&course)?;

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
    let targets: Vec<String> = sentence_corpus
        .app_sentences
        .iter()
        .map(|(target, _, _)| target.clone())
        .collect();
    translator.prime(&targets).await;

    let total = sentence_corpus.app_sentences.len() as u64;
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
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    use futures::StreamExt;
    let translator_ref = &translator;
    let pb_ref = &pb;
    let all_translations: BTreeMap<String, IndexSet<String>> =
        futures::stream::iter(sentence_corpus.app_sentences.iter().map(
            |(target_language_sentence, native_sentence, _)| {
                let target_language_sentence = target_language_sentence.clone();
                let native_sentence = native_sentence.clone();
                async move {
                    let mut translation_set = IndexSet::new();
                    match translator_ref.translate(&target_language_sentence).await {
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
                    translation_set
                        .retain(|t| !crate::target_sentences::contains_xprotect_tripwire(t));
                    pb_ref.inc(1);
                    (target_language_sentence, translation_set)
                }
            },
        ))
        .buffered(100)
        .collect()
        .await;

    pb.finish_with_message(format!(
        "~${:.4} · {} calls",
        translator.cost_estimate_usd(),
        translator.api_calls()
    ));
    drop(translator);

    let translations: BTreeMap<String, Vec<String>> = all_translations
        .into_iter()
        .filter(|(_, set)| !set.is_empty())
        .map(|(text, set)| (text, set.into_iter().collect()))
        .collect();

    let file =
        File::create(native_specific_dir.join("target_language_to_native_translations.jsonl"))
            .context("Failed to create translations file")?;
    let mut writer = BufWriter::new(file);
    for (text, translation_list) in &translations {
        writeln!(
            writer,
            "{}",
            serde_json::to_string(&(text, translation_list))?
        )?;
    }
    writer.flush()?;

    Ok(translations)
}
