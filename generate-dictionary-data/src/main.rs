use anyhow::{Context, Result};
use language_utils::language_pack::LanguagePack;
use language_utils::{Atom, COURSES, Course, GramDefinition, SentenceGram, WordType};
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
}

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

/// Lightweight course index for the listing page (no sentences).
#[derive(Serialize)]
struct CourseIndex {
    course_slug: String,
    target_language: String,
    native_language: String,
    total_pages: usize,
    total_senses: usize,
    pages: Vec<PageIndexEntry>,
}

#[derive(Serialize)]
struct PageIndexEntry {
    slug: String,
    display_text: String,
    best_frequency_rank: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_sense_preview: Option<SensePreview>,
}

#[derive(Serialize)]
struct SensePreview {
    is_phrase: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation: Option<String>,
    definition_preview: String,
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
    format!(
        "{}-to-{}",
        course.target_language.to_string().to_lowercase(),
        course.native_language.to_string().to_lowercase()
    )
}

fn course_dir_name(course: &Course) -> String {
    format!(
        "{}_for_{}",
        course.target_language.iso_639_3(),
        course.native_language.iso_639_3()
    )
}

fn text_to_slug(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else if c == ' ' || c == '\'' || c == '-' || c == '\u{2019}' {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace("--", "-")
        .trim_matches('-')
        .trim_matches('_')
        .to_string()
}

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

    for (frequency_index, (spur_gram, freq)) in language_pack.gram_frequencies.iter().enumerate() {
        let gram_def = match language_pack.gram_definitions.get(spur_gram) {
            Some(def) => def,
            None => continue,
        };

        let gram = gram_rodeo.resolve(spur_gram);
        let resolved = gram.resolve(string_rodeo);
        let display_text = resolved.to_display_string(target_language);

        let (pos, lemma, het_pos) = gram
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
                    ))
                } else {
                    None
                }
            })
            .unwrap_or((None, None, None));

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

        let sentence_spurs: Vec<Spur> = language_pack
            .sentences_containing_gram_index
            .get(spur_gram)
            .into_iter()
            .flat_map(|sentences| sentences.iter().copied())
            .take(200)
            .collect();

        let is_phrase = gram.len() > 1;
        let pronunciation = if is_phrase { None } else { pronunciation };

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
            }
        })
        .collect();

    // Build display_text → slug from the pages we just created
    let display_text_to_slug: FxHashMap<String, String> = pages
        .iter()
        .map(|p| (p.display_text.clone(), p.slug.clone()))
        .collect();

    // Build SpurGram → (slug, gloss) lookup for cross-linking sentences.
    let mut gram_to_info: FxHashMap<language_utils::SpurGram, GramInfo> = FxHashMap::default();
    for (spur_gram, _freq) in language_pack.gram_frequencies.iter() {
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
    if sentence_grams.capitalize_first {
        if let Some((first_word, _, _)) = word_entries.first_mut() {
            first_word.text = language_utils::capitalize_first_letter(&first_word.text);
        }
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
    let data_out_dir = Path::new("dictionary-site/src/data");
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

        // Write index file without example_sentences (for the course listing page)
        let index_data = CourseIndex {
            course_slug: course_data.course_slug.clone(),
            target_language: course_data.target_language.clone(),
            native_language: course_data.native_language.clone(),
            total_pages: course_data.total_pages,
            total_senses: course_data.total_senses,
            pages: course_data
                .pages
                .iter()
                .map(|p| PageIndexEntry {
                    slug: p.slug.clone(),
                    display_text: p.display_text.clone(),
                    best_frequency_rank: p.best_frequency_rank,
                    first_sense_preview: p.senses.first().map(|s| SensePreview {
                        is_phrase: s.is_phrase,
                        pronunciation: s.pronunciation.clone(),
                        definition_preview: get_definition_preview(s),
                    }),
                })
                .collect(),
        };
        let output_path = data_out_dir.join(format!("{slug}.json"));
        eprintln!(
            "Writing {} pages ({} senses), index to {} ...",
            course_data.total_pages,
            course_data.total_senses,
            output_path.display()
        );
        let json = serde_json::to_string(&index_data)?;
        std::fs::write(&output_path, json)?;

        // Generate search index
        let search_dir = Path::new("dictionary-site/public/search");
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
