use anyhow::{Context, Result};
use language_utils::features::Morphology;
use language_utils::language_pack::LanguagePack;
use language_utils::{Atom, COURSES, Course, GramDefinition, PartOfSpeech, SentenceGram, WordType};
use lasso::Spur;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::path::Path;

/// One page in the dictionary, grouping all senses of the same display text.
#[derive(Serialize)]
struct PageEntry {
    slug: String,
    display_text: String,
    /// Best (lowest) frequency rank among all senses
    best_frequency_rank: usize,
    senses: Vec<Sense>,
    /// Words that sound similar (minimal pairs — differ by exactly one
    /// phoneme), sorted by frequency. Empty for phrases and words with no
    /// minimal pairs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sounds_similar: Vec<SoundsSimilar>,
    /// Morpheme segmentation of the word (root + affixes, each with a gloss),
    /// in surface order. Empty unless the word has a ≥2-morpheme breakdown.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    morpheme_breakdown: Vec<MorphemeSeg>,
}

/// One morpheme in a word's segmentation.
#[derive(Serialize)]
struct MorphemeSeg {
    surface: String,
    /// Canonical/dictionary form, only when it differs from the surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical: Option<String>,
    /// Native-language gloss for the morpheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    gloss: Option<String>,
}

/// A word that differs from this page's word by exactly one phoneme.
#[derive(Serialize)]
struct SoundsSimilar {
    word: String,
    /// Slug of the neighbor's dictionary page — always present, since we only
    /// emit neighbors that have a page to link to.
    slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation: Option<String>,
}

/// Cap on minimal-pair neighbors listed per page (sorted by frequency, so the
/// most useful appear first). Keeps common short words — which can have dozens
/// of minimal pairs — from ballooning the page JSON.
const MAX_SOUNDS_SIMILAR: usize = 24;

/// Morphological information for a word sense.
#[derive(Serialize)]
struct MorphologyInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tense: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    person: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    case: Option<String>,
}

/// A conjugation/declension table for a lemma.
#[derive(Serialize)]
struct ConjugationTable {
    lemma: String,
    pos: String,
    forms: Vec<ConjugationForm>,
}

/// A single inflected form in a conjugation/declension table.
#[derive(Debug, Serialize)]
struct ConjugationForm {
    word: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tense: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    person: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    case: Option<String>,
}

/// A single word sense or phrase meaning.
#[derive(Serialize)]
struct Sense {
    frequency_rank: usize,
    frequency_count: u32,
    is_phrase: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lemma: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    morphology: Option<MorphologyInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conjugation: Option<ConjugationTable>,
    definition: Definition,
    /// Sentence spurs stored temporarily during extraction, replaced with rich
    /// segments in a second pass once we have the gram→slug map.
    #[serde(skip)]
    sentence_spurs: Vec<Spur>,
    /// Rich linked sentences with optional translation.
    example_sentences: Vec<ExampleSentence>,
}

/// A sentence with linked segments and an optional native-language translation.
#[derive(Serialize)]
struct ExampleSentence {
    segments: Vec<SentenceSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    translation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

/// A segment of a sentence — one or more words belonging to a single gram,
/// optionally linking to a dictionary page.
#[derive(Serialize)]
struct SentenceSegment {
    text: String,
    whitespace: String,
    /// Slug of the dictionary page for this gram, if one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    /// Short native-language definition for tooltip/gloss display.
    #[serde(skip_serializing_if = "Option::is_none")]
    gloss: Option<String>,
}

#[derive(Serialize)]
struct CourseData {
    course_slug: String,
    target_language: String,
    native_language: String,
    total_pages: usize,
    total_senses: usize,
    pages: Vec<PageEntry>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum Definition {
    Dictionary {
        definitions: Vec<WordDefinition>,
    },
    Phrasebook {
        meaning: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_notes: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_language_example: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        native_language_example: Option<String>,
        informal: bool,
        compositional: bool,
        cognate: bool,
    },
}

#[derive(Serialize)]
struct WordDefinition {
    native: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "str::is_empty")]
    example_target: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    example_native: String,
    cognate: bool,
    false_cognate: bool,
}

/// Lightweight page entry for listing pages (no sentences).
#[derive(Serialize, Clone)]
struct PageIndexEntry {
    slug: String,
    display_text: String,
    best_frequency_rank: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_sense_preview: Option<SensePreview>,
}

#[derive(Serialize, Clone)]
struct SensePreview {
    is_phrase: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    definition_preview: String,
}

/// Letter manifest entry for the course landing page.
#[derive(Serialize)]
struct LetterEntry {
    letter: String,
    count: usize,
    /// A few of the most common words starting with this letter.
    preview_words: Vec<String>,
}

fn get_definition_preview(sense: &Sense) -> String {
    match &sense.definition {
        Definition::Dictionary { definitions } => definitions
            .iter()
            .map(|d| d.native.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        Definition::Phrasebook { meaning, .. } => meaning.clone(),
    }
}

fn course_slug(course: &Course) -> String {
    course.dictionary_slug()
}

fn course_dir_name(course: &Course) -> String {
    format!(
        "{}_for_{}",
        course.target_language.iso_639_3(),
        course.native_language.iso_639_3()
    )
}

use language_utils::dictionary_entry_slug as text_to_slug;

fn load_language_pack(rkyv_path: &Path) -> Result<LanguagePack> {
    let bytes = std::fs::read(rkyv_path)
        .with_context(|| format!("Failed to read {}", rkyv_path.display()))?;
    let archived = rkyv::access::<rkyv::Archived<LanguagePack>, rkyv::rancor::Error>(&bytes)
        .map_err(|e| anyhow::anyhow!("Failed to access rkyv archive: {e}"))?;
    let language_pack = rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize language pack: {e}"))?;
    Ok(language_pack)
}

fn extract_pages(language_pack: &LanguagePack, course: &Course) -> CourseData {
    let string_rodeo = &language_pack.string_rodeo;
    let gram_rodeo = &language_pack.gram_rodeo;
    let target_language = course.target_language;

    // Group senses by display text, preserving insertion order via BTreeMap on
    // (frequency_rank, display_text) so pages end up sorted by best rank.
    // We use a map from display_text -> Vec<Sense>.
    let mut pages_map: indexmap::IndexMap<String, Vec<Sense>> = indexmap::IndexMap::new();

    // Conjugation index: (lemma, pos) → Vec<(display_text, morphology)>
    let mut conjugation_index: FxHashMap<(String, PartOfSpeech), Vec<(String, Morphology)>> =
        FxHashMap::default();

    // display_text → heteronym word spur, for minimal-pair ("sounds similar")
    // lookup. First (most frequent) single-word occurrence wins, matching the
    // frequency-descending iteration order below.
    let mut display_text_to_word_spur: FxHashMap<String, Spur> = FxHashMap::default();

    // display_text → representative single-word gram, for morpheme breakdown.
    // First (most frequent) single-word occurrence wins.
    let mut display_text_to_main_gram: FxHashMap<String, language_utils::SpurGram> =
        FxHashMap::default();

    for (frequency_index, (spur_gram, freq)) in
        language_pack.gram_frequencies.entries.iter().enumerate()
    {
        let gram_def = match language_pack.gram_definitions.get(spur_gram) {
            Some(def) => def,
            None => continue,
        };

        let gram = gram_rodeo.resolve(spur_gram);
        let resolved = gram.resolve(string_rodeo);
        let display_text = resolved.to_display_string(target_language);

        let (pos, lemma, het_pos, het_word) = gram
            .atoms()
            .iter()
            .find_map(|atom| {
                if let Atom::Tok(word) = atom
                    && let WordType::Heteronym(h) = &word.word_type
                {
                    Some((
                        Some(format!("{:?}", h.pos)),
                        Some(string_rodeo.resolve(&h.lemma).to_string()),
                        Some(h.pos),
                        Some(string_rodeo.resolve(&h.word).to_string()),
                    ))
                } else {
                    None
                }
            })
            .unwrap_or((None, None, None, None));

        // Heteronym word spur (the interned word form) — used to look the word
        // up in the pack's minimal-pairs index.
        let het_word_spur: Option<Spur> = gram.atoms().iter().find_map(|atom| {
            if let Atom::Tok(word) = atom
                && let WordType::Heteronym(h) = &word.word_type
            {
                Some(h.word)
            } else {
                None
            }
        });

        let pronunciation = gram
            .atoms()
            .iter()
            .find_map(|atom| {
                if let Atom::Tok(word) = atom
                    && let WordType::Heteronym(h) = &word.word_type
                {
                    language_pack
                        .word_to_pronunciation
                        .get(&h.word)
                        .map(|p| string_rodeo.resolve(p).to_string())
                } else {
                    None
                }
            })
            .filter(|p| !p.is_empty());

        // Collect example sentences, prioritizing those where the gram appears
        // as a direct encoded gram over those where it only appears via multiword terms.
        let sentence_spurs: Vec<Spur> = {
            let mut seen_sentences = std::collections::HashSet::new();
            let all_sentences: Vec<Spur> = language_pack
                .sentences_containing_gram_index
                .get(spur_gram)
                .into_iter()
                .flat_map(|sentences| sentences.iter().copied())
                .filter(|s| seen_sentences.insert(*s))
                .collect();

            // Partition: direct gram matches first, then multiword matches
            let mut direct = Vec::new();
            let mut multiword = Vec::new();
            for sentence_spur in all_sentences {
                if let Some(sg) = language_pack.encoded_sentences.get(&sentence_spur) {
                    let is_direct = sg.grams.iter().any(|g| {
                        let s = match g {
                            SentenceGram::Learnable(s) | SentenceGram::Obvious(s) => s,
                        };
                        s == spur_gram
                    });
                    if is_direct {
                        direct.push(sentence_spur);
                    } else {
                        multiword.push(sentence_spur);
                    }
                }
            }

            // For multi-atom grams, skip multiword term matches entirely
            if gram.len() > 1 {
                direct.truncate(200);
                direct
            } else {
                let remaining = 200usize.saturating_sub(direct.len());
                direct.extend(multiword.into_iter().take(remaining));
                direct.truncate(200);
                direct
            }
        };

        let is_phrase = gram.len() > 1;
        let pronunciation = if is_phrase { None } else { pronunciation };

        if !is_phrase && let Some(ws) = het_word_spur {
            display_text_to_word_spur
                .entry(display_text.clone())
                .or_insert(ws);
            display_text_to_main_gram
                .entry(display_text.clone())
                .or_insert(*spur_gram);
        }

        // Extract morphology and prefix for single-word dictionary entries
        let (morphology, prefix) = if !is_phrase {
            if let GramDefinition::Dictionary(dict) = gram_def {
                let morph = dict.morphology.first().map(|m| MorphologyInfo {
                    gender: m.gender.map(|g| format!("{g:?}").to_lowercase()),
                    number: m.number.map(|n| format!("{n:?}").to_lowercase()),
                    tense: m.tense.map(|t| format!("{t:?}").to_lowercase()),
                    person: m.person.map(|p| format!("{p:?}").to_lowercase()),
                    mood: m.mood.map(|md| format!("{md:?}").to_lowercase()),
                    case: m.case.map(|c| format!("{c:?}").to_lowercase()),
                });
                let pfx = dict
                    .morphology
                    .first()
                    .and_then(|m| {
                        het_pos.and_then(|p| m.get_prefix(&display_text, p, target_language))
                    })
                    .map(|wp| format!("{}{}", wp.prefix, wp.separator));
                (morph, pfx)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Populate conjugation index for single-word dictionary entries with morphology.
        // Use het_word (the normalized heteronym word) rather than display_text (raw surface
        // form which may be capitalized from sentence-initial position).
        if !is_phrase
            && let GramDefinition::Dictionary(dict) = gram_def
            && let (Some(lemma_str), Some(pos_val), Some(word)) = (&lemma, het_pos, &het_word)
        {
            for morph in &dict.morphology {
                conjugation_index
                    .entry((lemma_str.clone(), pos_val))
                    .or_default()
                    .push((word.clone(), morph.clone()));
            }
        }

        let definition = match gram_def {
            GramDefinition::Dictionary(dict) => Definition::Dictionary {
                definitions: dict
                    .definitions
                    .iter()
                    .map(|d| WordDefinition {
                        native: d.native.clone(),
                        note: d.note.clone().filter(|n| !n.is_empty()),
                        example_target: d.example_sentence_target_language.clone(),
                        example_native: d.example_sentence_native_language.clone(),
                        cognate: d.cognate,
                        false_cognate: d.false_cognate,
                    })
                    .collect(),
            },
            GramDefinition::Phrasebook(pb) => Definition::Phrasebook {
                meaning: pb.meaning.clone(),
                additional_notes: Some(pb.additional_notes.clone()).filter(|n| !n.is_empty()),
                target_language_example: Some(pb.target_language_example.clone())
                    .filter(|s| !s.is_empty()),
                native_language_example: Some(pb.native_language_example.clone())
                    .filter(|s| !s.is_empty()),
                informal: pb.informal,
                compositional: pb.compositional,
                cognate: pb.cognate && !pb.false_cognate,
            },
        };

        let sense = Sense {
            frequency_rank: frequency_index + 1,
            frequency_count: freq.count,
            is_phrase,
            pos,
            lemma,
            pronunciation,
            prefix,
            morphology,
            conjugation: None,
            definition,
            sentence_spurs,
            example_sentences: Vec::new(),
        };

        pages_map.entry(display_text).or_default().push(sense);
    }

    // Convert to PageEntry list, deduplicate slugs
    let mut used_slugs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut total_senses = 0;

    let mut pages: Vec<PageEntry> = pages_map
        .into_iter()
        .map(|(display_text, senses)| {
            let best_frequency_rank = senses
                .iter()
                .map(|s| s.frequency_rank)
                .min()
                .unwrap_or(usize::MAX);

            let base_slug = text_to_slug(&display_text);

            let slug = if used_slugs.contains(&base_slug) {
                let mut i = 2;
                loop {
                    let candidate = format!("{base_slug}-{i}");
                    if !used_slugs.contains(&candidate) {
                        break candidate;
                    }
                    i += 1;
                }
            } else {
                base_slug
            };

            used_slugs.insert(slug.clone());
            total_senses += senses.len();

            PageEntry {
                slug,
                display_text,
                best_frequency_rank,
                senses,
                sounds_similar: Vec::new(),
                morpheme_breakdown: Vec::new(),
            }
        })
        .collect();

    // Build display_text → slug from the pages we just created
    let display_text_to_slug: FxHashMap<String, String> = pages
        .iter()
        .map(|p| (p.display_text.clone(), p.slug.clone()))
        .collect();

    // Build the "sounds similar to" (minimal-pair) links. Neighbors come from
    // the pack's precomputed minimal-pairs index (words differing by exactly
    // one phoneme, already sorted by frequency); we keep only those that have
    // their own dictionary page so every link resolves.
    let word_spur_to_slug: FxHashMap<Spur, String> = display_text_to_word_spur
        .iter()
        .filter_map(|(dt, ws)| display_text_to_slug.get(dt).map(|slug| (*ws, slug.clone())))
        .collect();
    for page in &mut pages {
        let Some(word_spur) = display_text_to_word_spur.get(&page.display_text) else {
            continue;
        };
        let Some(neighbors) = language_pack.minimal_pairs.by_word.get(word_spur) else {
            continue;
        };
        let mut seen_slugs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for neighbor_spur in neighbors {
            let Some(slug) = word_spur_to_slug.get(neighbor_spur) else {
                continue;
            };
            // Skip self-links and collapse neighbors that share a page.
            if *slug == page.slug || !seen_slugs.insert(slug.clone()) {
                continue;
            }
            let word = string_rodeo.resolve(neighbor_spur).to_string();
            let pronunciation = language_pack
                .word_to_pronunciation
                .get(neighbor_spur)
                .map(|p| string_rodeo.resolve(p).to_string())
                .filter(|s| !s.is_empty());
            page.sounds_similar.push(SoundsSimilar {
                word,
                slug: slug.clone(),
                pronunciation,
            });
            if page.sounds_similar.len() >= MAX_SOUNDS_SIMILAR {
                break;
            }
        }
    }

    // Attach the morpheme segmentation (root + affixes with glosses) per page,
    // computed from the page's representative single-word gram. Reuses the same
    // breakdown logic the app uses (LanguagePack::compute_breakdown).
    for page in &mut pages {
        let Some(main_gram) = display_text_to_main_gram.get(&page.display_text) else {
            continue;
        };
        let Some(segments) = language_pack.compute_breakdown(*main_gram) else {
            continue;
        };
        page.morpheme_breakdown = segments
            .into_iter()
            .map(|(surface, canonical, gloss)| MorphemeSeg {
                surface,
                canonical,
                gloss,
            })
            .collect();
    }

    // Build SpurGram → (slug, gloss) lookup for cross-linking sentences.
    let mut gram_to_info: FxHashMap<language_utils::SpurGram, GramInfo> = FxHashMap::default();
    for (spur_gram, _freq) in language_pack.gram_frequencies.entries.iter() {
        let gram = gram_rodeo.resolve(spur_gram);
        let resolved = gram.resolve(string_rodeo);
        let dt = resolved.to_display_string(target_language);
        let slug = display_text_to_slug.get(&dt).cloned();
        let gloss = language_pack
            .gram_definitions
            .get(spur_gram)
            .map(|def| match def {
                GramDefinition::Dictionary(d) => d
                    .definitions
                    .first()
                    .map(|dd| dd.native.clone())
                    .unwrap_or_default(),
                GramDefinition::Phrasebook(p) => p.meaning.clone(),
            })
            .filter(|g| !g.is_empty());
        if slug.is_some() || gloss.is_some() {
            gram_to_info.insert(*spur_gram, GramInfo { slug, gloss });
        }
    }

    // Second pass: resolve sentence spurs into rich linked segments
    for page in &mut pages {
        for sense in &mut page.senses {
            sense.example_sentences = sense
                .sentence_spurs
                .iter()
                .filter_map(|sentence_spur| {
                    resolve_sentence(language_pack, sentence_spur, target_language, &gram_to_info)
                })
                .collect();
            sense.sentence_spurs.clear();
        }
    }

    // Dedup assertion: no (lemma, pos, morphology) triple should map to two different words
    {
        let mut seen: std::collections::HashMap<(&str, PartOfSpeech, &Morphology), &str> =
            std::collections::HashMap::new();
        for ((lemma, pos), forms) in &conjugation_index {
            for (word, morph) in forms {
                if let Some(existing) = seen.get(&(lemma.as_str(), *pos, morph)) {
                    if *existing != word {
                        eprintln!(
                            "Warning: conjugation conflict for lemma={lemma}, pos={pos:?}, morph={morph:?}: \
                             {existing} vs {word} — keeping first"
                        );
                    }
                } else {
                    seen.insert((lemma.as_str(), *pos, morph), word.as_str());
                }
            }
        }
    }

    // Attach conjugation tables to senses (single-word only)
    for page in &mut pages {
        for sense in &mut page.senses {
            if sense.is_phrase {
                continue;
            }
            if let (Some(lemma), Some(pos_str)) = (&sense.lemma, &sense.pos) {
                // Parse pos string back to PartOfSpeech
                let pos_val = match pos_str.as_str() {
                    "Adj" => Some(PartOfSpeech::Adj),
                    "Adp" => Some(PartOfSpeech::Adp),
                    "Adv" => Some(PartOfSpeech::Adv),
                    "Aux" => Some(PartOfSpeech::Aux),
                    "Cconj" => Some(PartOfSpeech::Cconj),
                    "Det" => Some(PartOfSpeech::Det),
                    "Intj" => Some(PartOfSpeech::Intj),
                    "Noun" => Some(PartOfSpeech::Noun),
                    "Num" => Some(PartOfSpeech::Num),
                    "Part" => Some(PartOfSpeech::Part),
                    "Pron" => Some(PartOfSpeech::Pron),
                    "Sconj" => Some(PartOfSpeech::Sconj),
                    "Sym" => Some(PartOfSpeech::Sym),
                    "Verb" => Some(PartOfSpeech::Verb),
                    _ => None,
                };
                if let Some(pos) = pos_val
                    && let Some(forms) = conjugation_index.get(&(lemma.clone(), pos))
                {
                    // Deduplicate forms by (word, morphology)
                    let mut seen_forms: std::collections::HashSet<(&str, &Morphology)> =
                        std::collections::HashSet::new();
                    let mut unique_forms: Vec<ConjugationForm> = Vec::new();
                    for (word, morph) in forms {
                        if seen_forms.insert((word.as_str(), morph)) {
                            unique_forms.push(ConjugationForm {
                                word: word.clone(),
                                slug: display_text_to_slug.get(word).cloned(),
                                tense: morph.tense.map(|t| format!("{t:?}").to_lowercase()),
                                mood: morph.mood.map(|m| format!("{m:?}").to_lowercase()),
                                person: morph.person.map(|p| format!("{p:?}").to_lowercase()),
                                number: morph.number.map(|n| format!("{n:?}").to_lowercase()),
                                gender: morph.gender.map(|g| format!("{g:?}").to_lowercase()),
                                case: morph.case.map(|c| format!("{c:?}").to_lowercase()),
                            });
                        }
                    }
                    // Only attach if there's more than 1 form
                    if unique_forms.len() > 1 {
                        sense.conjugation = Some(ConjugationTable {
                            lemma: lemma.clone(),
                            pos: pos_str.clone(),
                            forms: unique_forms,
                        });
                    }
                }
            }
        }
    }

    let slug = course_slug(course);
    CourseData {
        course_slug: slug,
        target_language: course.target_language.to_string(),
        native_language: course.native_language.to_string(),
        total_pages: pages.len(),
        total_senses,
        pages,
    }
}

struct GramInfo {
    slug: Option<String>,
    gloss: Option<String>,
}

/// Resolve a sentence into linked segments, where each gram's words are grouped
/// and linked to their dictionary page.
fn resolve_sentence(
    language_pack: &LanguagePack,
    sentence_spur: &Spur,
    language: language_utils::Language,
    gram_to_info: &FxHashMap<language_utils::SpurGram, GramInfo>,
) -> Option<ExampleSentence> {
    let sentence_grams = language_pack.encoded_sentences.get(sentence_spur)?;
    let string_rodeo = &language_pack.string_rodeo;
    let gram_rodeo = &language_pack.gram_rodeo;

    // Collect all (word, slug, gloss) tuples
    let mut word_entries: Vec<(language_utils::Word<String>, Option<String>, Option<String>)> =
        Vec::new();
    for sg in &sentence_grams.grams {
        let spur_gram = match sg {
            SentenceGram::Learnable(g) | SentenceGram::Obvious(g) => g,
        };
        let info = gram_to_info.get(spur_gram);
        let slug = info.and_then(|i| i.slug.clone());
        let gloss = info.and_then(|i| i.gloss.clone());
        let gram = gram_rodeo.resolve(spur_gram).resolve(string_rodeo);
        for atom in gram.iter() {
            if let Atom::Tok(word) = atom {
                word_entries.push((word.clone(), slug.clone(), gloss.clone()));
            }
        }
    }

    // Capitalize first word if needed
    if sentence_grams.capitalize_first
        && let Some((first_word, _, _)) = word_entries.first_mut()
    {
        first_word.text = language_utils::capitalize_first_letter(&first_word.text);
    }

    // Build segments, grouping consecutive words with the same gram slug
    let mut segments: Vec<SentenceSegment> = Vec::new();
    for (i, (word, slug, gloss)) in word_entries.iter().enumerate() {
        let next_word = word_entries.get(i + 1).map(|(w, _, _)| w);
        let whitespace = language_utils::predict_whitespace(word, next_word, language)
            .to_str()
            .to_string();

        // Try to merge with previous segment if same slug (for multi-word grams)
        if let Some(last) = segments.last_mut()
            && last.slug == *slug
            && slug.is_some()
        {
            // Append this word's text to the previous segment
            let prev_ws = std::mem::replace(&mut last.whitespace, whitespace);
            last.text.push_str(&prev_ws);
            last.text.push_str(&word.text);
        } else {
            segments.push(SentenceSegment {
                text: word.text.clone(),
                whitespace,
                slug: slug.clone(),
                gloss: gloss.clone(),
            });
        }
    }

    let translation = language_pack
        .translations
        .get(sentence_spur)
        .and_then(|ts| ts.first())
        .map(|t| language_pack.string_rodeo.resolve(t).to_string());

    // Look up sentence source for attribution
    let source = language_pack
        .sentence_sources
        .get(sentence_spur)
        .and_then(|ss| ss.movie_ids.first())
        .and_then(|movie_id| language_pack.movies.get(movie_id))
        .map(|movie| {
            if let Some(year) = movie.year {
                format!("{} ({})", movie.title, year)
            } else {
                movie.title.clone()
            }
        });

    Some(ExampleSentence {
        segments,
        translation,
        source,
    })
}

fn main() -> Result<()> {
    let out_dir = Path::new("out");
    let data_out_dir = Path::new("static-site/src/data");
    std::fs::create_dir_all(data_out_dir)?;

    let mut courses_manifest: Vec<serde_json::Value> = Vec::new();

    for course in COURSES {
        let dir_name = course_dir_name(course);
        let rkyv_path = out_dir.join(&dir_name).join("language_data.rkyv");

        if !rkyv_path.exists() {
            eprintln!(
                "Skipping {} (no rkyv file at {})",
                dir_name,
                rkyv_path.display()
            );
            continue;
        }

        eprintln!("Loading {dir_name} ...");
        let language_pack = load_language_pack(&rkyv_path)?;

        eprintln!("Extracting dictionary data for {dir_name} ...");
        let course_data = extract_pages(&language_pack, course);

        let slug = course_slug(course);

        // Write per-page JSON files for entry pages (avoids loading everything into memory)
        let pages_dir = data_out_dir.join(&slug);
        std::fs::create_dir_all(&pages_dir)?;
        for page in &course_data.pages {
            let page_path = pages_dir.join(format!("{}.json", page.slug));
            let page_json = serde_json::to_string(page)?;
            std::fs::write(&page_path, page_json)?;
        }

        // Build PageIndexEntry list for all pages
        let all_index_entries: Vec<PageIndexEntry> = course_data
            .pages
            .iter()
            .map(|p| PageIndexEntry {
                slug: p.slug.clone(),
                display_text: p.display_text.clone(),
                best_frequency_rank: p.best_frequency_rank,
                first_sense_preview: p.senses.first().map(|s| SensePreview {
                    is_phrase: s.is_phrase,
                    pronunciation: s.pronunciation.clone(),
                    prefix: s.prefix.clone(),
                    definition_preview: get_definition_preview(s),
                }),
            })
            .collect();

        // Write top-1000 JSON files: combined, words-only, phrases-only
        let listing_dir = data_out_dir.join(&slug);
        let top_combined: Vec<_> = all_index_entries.iter().take(1000).cloned().collect();
        let top_words: Vec<_> = all_index_entries
            .iter()
            .filter(|e| !e.first_sense_preview.as_ref().is_some_and(|s| s.is_phrase))
            .take(1000)
            .cloned()
            .collect();
        let top_phrases: Vec<_> = all_index_entries
            .iter()
            .filter(|e| e.first_sense_preview.as_ref().is_some_and(|s| s.is_phrase))
            .take(1000)
            .cloned()
            .collect();
        for (name, entries) in [
            ("top-1000", &top_combined),
            ("top-1000-words", &top_words),
            ("top-1000-phrases", &top_phrases),
        ] {
            let path = listing_dir.join(format!("{name}.json"));
            std::fs::write(&path, serde_json::to_string(entries)?)?;
            eprintln!(
                "Wrote {name} ({} entries) to {}",
                entries.len(),
                path.display()
            );
        }

        // Group pages by first letter and write per-letter JSON files
        let letter_dir = listing_dir.join("letter");
        std::fs::create_dir_all(&letter_dir)?;

        let mut letter_groups: indexmap::IndexMap<String, Vec<PageIndexEntry>> =
            indexmap::IndexMap::new();
        for entry in &all_index_entries {
            let first_char = entry.display_text.chars().next().unwrap_or('?');
            let letter = if first_char.is_alphabetic() {
                use unicode_normalization::UnicodeNormalization;
                first_char
                    .to_uppercase()
                    .to_string()
                    .nfd()
                    .next()
                    .unwrap_or(first_char)
                    .to_uppercase()
                    .to_string()
            } else {
                first_char.to_string()
            };
            letter_groups.entry(letter).or_default().push(entry.clone());
        }

        let mut letters_manifest: Vec<LetterEntry> = Vec::new();
        for (letter, entries) in &letter_groups {
            // Write per-letter file
            let letter_path = letter_dir.join(format!("{letter}.json"));
            std::fs::write(&letter_path, serde_json::to_string(&entries)?)?;

            // Top 3 most common words for this letter (already frequency-sorted)
            let preview_words: Vec<String> = entries
                .iter()
                .filter(|e| e.first_sense_preview.as_ref().is_none_or(|p| !p.is_phrase))
                .take(3)
                .map(|e| e.display_text.clone())
                .collect();

            letters_manifest.push(LetterEntry {
                letter: letter.clone(),
                count: entries.len(),
                preview_words,
            });
        }
        letters_manifest.sort_by(|a, b| a.letter.cmp(&b.letter));

        let letters_path = listing_dir.join("letters.json");
        std::fs::write(&letters_path, serde_json::to_string(&letters_manifest)?)?;

        eprintln!(
            "Wrote {} letter indexes + letters manifest ({} pages, {} senses)",
            letter_groups.len(),
            course_data.total_pages,
            course_data.total_senses,
        );

        // Generate search index
        let search_dir = Path::new("static-site/public/search");
        std::fs::create_dir_all(search_dir)?;

        let mut results: Vec<(String, String, String)> = Vec::new(); // (slug, display_text, preview)
        let mut keys: Vec<(String, usize)> = Vec::new(); // (search_key, page_index)

        for (page_idx, page) in course_data.pages.iter().enumerate() {
            let preview = page
                .senses
                .first()
                .map(get_definition_preview)
                .unwrap_or_default();
            results.push((
                page.slug.clone(),
                page.display_text.clone(),
                preview.clone(),
            ));

            // Target language key
            keys.push((page.display_text.to_lowercase(), page_idx));

            // Native language keys from definition preview
            for part in preview.split(';') {
                let trimmed = part.trim().to_lowercase();
                if !trimmed.is_empty() {
                    keys.push((trimmed, page_idx));
                }
            }
        }

        keys.sort_by(|a, b| a.0.cmp(&b.0));

        let search_index = serde_json::json!({
            "k": keys.iter().map(|(k, i)| serde_json::json!([k, i])).collect::<Vec<_>>(),
            "r": results.iter().map(|(s, d, p)| serde_json::json!([s, d, p])).collect::<Vec<_>>(),
        });
        let search_path = search_dir.join(format!("{slug}.json"));
        std::fs::write(&search_path, serde_json::to_string(&search_index)?)?;
        eprintln!("Wrote search index to {}", search_path.display());

        courses_manifest.push(serde_json::json!({
            "slug": slug,
            "target_language": course.target_language.to_string(),
            "native_language": course.native_language.to_string(),
            "total_pages": course_data.total_pages,
            "total_senses": course_data.total_senses,
        }));
    }

    let manifest_path = data_out_dir.join("courses.json");
    let manifest_json = serde_json::to_string_pretty(&courses_manifest)?;
    std::fs::write(&manifest_path, manifest_json)?;
    eprintln!("Wrote courses manifest to {}", manifest_path.display());

    eprintln!("Done!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::{Course, Language};

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn load_and_extract(course: &Course) -> CourseData {
        let dir_name = course_dir_name(course);
        let rkyv_path = repo_root()
            .join("out")
            .join(&dir_name)
            .join("language_data.rkyv");
        let language_pack = load_language_pack(&rkyv_path)
            .unwrap_or_else(|e| panic!("Failed to load {}: {e}", rkyv_path.display()));
        extract_pages(&language_pack, course)
    }

    fn find_page_by_display<'a>(data: &'a CourseData, display_text: &str) -> &'a PageEntry {
        data.pages
            .iter()
            .find(|p| p.display_text == display_text)
            .unwrap_or_else(|| panic!("Page with display_text '{display_text}' not found"))
    }

    fn conjugation_words(table: &ConjugationTable) -> Vec<&str> {
        table.forms.iter().map(|f| f.word.as_str()).collect()
    }

    // --- French verb tests ---

    #[test]
    #[ignore]
    fn french_boire_second_person_singular() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "bois");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("boire"))
            .expect("No sense with lemma 'boire'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "boire");
        // "bois" should appear as both first and second person singular
        let has_second_sg = table.forms.iter().any(|f| {
            f.word == "bois"
                && f.person.as_deref() == Some("second")
                && f.number.as_deref() == Some("singular")
        });
        assert!(
            has_second_sg,
            "Expected second person singular 'bois' in boire conjugation. Forms: {:?}",
            table
                .forms
                .iter()
                .filter(|f| f.word == "bois")
                .collect::<Vec<_>>()
        );
    }

    #[test]
    #[ignore]
    fn french_etre_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "est");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("être"))
            .expect("No sense with lemma 'être'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "être");
        let words = conjugation_words(table);
        for expected in &["suis", "es", "est", "sommes", "êtes", "sont"] {
            assert!(
                words.contains(expected),
                "Missing form '{expected}' in être conjugation. Found: {words:?}"
            );
        }
    }

    #[test]
    #[ignore]
    fn french_avoir_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "a");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("avoir"))
            .expect("No sense with lemma 'avoir'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "avoir");
        let words = conjugation_words(table);
        for expected in &["ai", "as", "a", "avons", "avez", "ont"] {
            assert!(
                words.contains(expected),
                "Missing form '{expected}' in avoir conjugation. Found: {words:?}"
            );
        }
    }

    #[test]
    #[ignore]
    fn french_faire_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "fait");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("faire"))
            .expect("No sense with lemma 'faire'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "faire");
        assert!(table.forms.len() > 3, "Expected multiple forms for faire");
    }

    // --- French adjective test ---

    #[test]
    #[ignore]
    fn french_beau_adjective() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        // Find any page with lemma "beau" and pos Adj
        let page = data
            .pages
            .iter()
            .find(|p| {
                p.senses
                    .iter()
                    .any(|s| s.lemma.as_deref() == Some("beau") && s.pos.as_deref() == Some("Adj"))
            })
            .expect("No page with lemma 'beau' as Adj");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("beau") && s.pos.as_deref() == Some("Adj"))
            .unwrap();
        let table = sense
            .conjugation
            .as_ref()
            .expect("No conjugation table for beau");
        let words = conjugation_words(table);
        // Should have gender/number forms
        assert!(
            words.len() >= 2,
            "Expected multiple forms for beau. Found: {words:?}"
        );
    }

    // --- Spanish verb tests ---

    #[test]
    #[ignore]
    fn spanish_ser_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::Spanish,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "es");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("ser"))
            .expect("No sense with lemma 'ser'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "ser");
        let words = conjugation_words(table);
        for expected in &["soy", "eres", "es", "somos"] {
            assert!(
                words.contains(expected),
                "Missing form '{expected}' in ser conjugation. Found: {words:?}"
            );
        }
    }

    // --- German verb test ---

    #[test]
    #[ignore]
    fn german_sein_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::German,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "ist");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("sein"))
            .expect("No sense with lemma 'sein'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "sein");
        let words = conjugation_words(table);
        for expected in &["bin", "bist", "ist", "sind"] {
            assert!(
                words.contains(expected),
                "Missing form '{expected}' in sein conjugation. Found: {words:?}"
            );
        }
    }

    // --- Italian verb test ---

    #[test]
    #[ignore]
    fn italian_essere_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::Italian,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "è");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("essere"))
            .expect("No sense with lemma 'essere'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "essere");
        assert!(table.forms.len() > 3, "Expected multiple forms for essere");
    }

    // --- Portuguese verb test ---

    #[test]
    #[ignore]
    fn portuguese_ser_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::Portuguese,
        };
        let data = load_and_extract(&course);
        let page = find_page_by_display(&data, "é");
        let sense = page
            .senses
            .iter()
            .find(|s| s.lemma.as_deref() == Some("ser"))
            .expect("No sense with lemma 'ser'");
        let table = sense.conjugation.as_ref().expect("No conjugation table");
        assert_eq!(table.lemma, "ser");
        assert!(table.forms.len() > 3, "Expected multiple forms for ser");
    }

    // --- Negative tests ---

    #[test]
    #[ignore]
    fn phrase_has_no_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        // Find a phrase entry
        let phrase_page = data
            .pages
            .iter()
            .find(|p| p.senses.iter().any(|s| s.is_phrase))
            .expect("No phrase page found");
        for sense in &phrase_page.senses {
            if sense.is_phrase {
                assert!(
                    sense.conjugation.is_none(),
                    "Phrase should not have conjugation table"
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn single_form_no_conjugation() {
        let course = Course {
            native_language: Language::English,
            target_language: Language::French,
        };
        let data = load_and_extract(&course);
        // Find a word that is its own lemma and has no other forms
        // Adverbs typically have only one form
        let adv_page = data.pages.iter().find(|p| {
            p.senses.iter().any(|s| {
                s.pos.as_deref() == Some("Adv")
                    && s.lemma.as_deref() == Some(&p.display_text)
                    && s.conjugation.is_none()
            })
        });
        assert!(
            adv_page.is_some(),
            "Expected to find at least one adverb with no conjugation table"
        );
    }
}
