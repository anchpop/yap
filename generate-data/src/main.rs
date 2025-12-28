use anyhow::Context;
use futures::StreamExt;
use indexmap::IndexSet;
use itertools::Itertools;
use language_utils::{COURSES, HomophonePractice};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

mod google_translate;
use google_translate::GoogleTranslator;

use generate_data::morphology_analysis;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    for course in COURSES {
        println!();
        println!();
        println!(
            "Processing course: {} -> {}",
            course.native_language, course.target_language
        );
        println!("================================================");

        let target_language_dir =
            PathBuf::from(format!("./out/{}", course.target_language.iso_639_3()));
        std::fs::create_dir_all(&target_language_dir)?;
        let target_language_dir = target_language_dir
            .canonicalize()
            .context("Failed to canonicalize target language output directory")?;

        let native_specific_dir = PathBuf::from(format!(
            "./out/{}_for_{}",
            course.target_language.iso_639_3(),
            course.native_language.iso_639_3()
        ));
        std::fs::create_dir_all(&native_specific_dir)?;
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
                generate_data::target_sentences::get_target_sentences(*course)?;

            // Create the translator once and share it across all async tasks
            let translator = GoogleTranslator::new(
                course.target_language, // translate from target to native
                course.native_language,
                PathBuf::from(".cache/google_translate/"),
            )
            .unwrap();

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
                // Write individual target language sentence
                let target_language_json = serde_json::to_string(&target_language_sentence)?;
                if let Err(e) = writeln!(target_language_writer, "{target_language_json}") {
                    eprintln!("Error writing to target_language sentences file: {e}");
                }

                let translation_json = serde_json::to_string(&(
                    &target_language_sentence,
                    native_translations.into_iter().collect::<Vec<_>>(),
                ))?;
                if let Err(e) = writeln!(translations_writer, "{translation_json}") {
                    eprintln!("Error writing to translations file: {e}");
                }

                let source_json = serde_json::to_string(&(&target_language_sentence, &source))?;
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
        .await?;

        // Process multiword terms with Rust NLP (lexide)
        let multiword_terms_tokenization_file =
            target_language_dir.join("target_language_multiword_terms_tokenization.jsonl");

        // Read multiword terms from file
        let multiword_terms = {
            let file = File::open(&multiword_terms_file)?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map_while(Result::ok)
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<String>>()
        };

        // Process multiword terms and get tokenizations
        let multiword_terms_tokenizations = generate_data::nlp::process_sentences(
            multiword_terms,
            &multiword_terms_tokenization_file,
            course.target_language,
        )
        .await?;

        // Process sentences with lexide
        let target_language_tokenization_file =
            target_language_dir.join("target_language_sentences_tokenization.jsonl");

        // Read sentences from file
        let sentences = {
            let file = File::open(&target_language_sentences_file)?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<String>, _>>()?
        };

        // Process sentences using the new Rust implementation and get tokenizations
        // (incremental processing will skip already-processed sentences)
        let sentences_tokenizations = generate_data::nlp::process_sentences(
            sentences,
            &target_language_tokenization_file,
            course.target_language,
        )
        .await?;

        // now add multiword terms to the tokenized sentences
        let target_language_nlp_file =
            target_language_dir.join("target_language_sentences_nlp.jsonl");

        // Generate NLP sentences
        let mut nlp_sentences = generate_data::nlp::generate_nlp_sentences(
            sentences_tokenizations,
            &multiword_terms_tokenizations,
            &target_language_nlp_file,
            course.target_language,
        )
        .await?;

        let all_lexemes: Vec<language_utils::Lexeme<String>> = nlp_sentences
            .iter()
            .flat_map(|(_, analysis)| analysis.all_lexemes())
            .filter(|lexeme| match lexeme {
                language_utils::Lexeme::Heteronym(heteronym) => !banned_words.contains(heteronym),
                _ => true,
            })
            .collect();

        // Generate frequencies file for combined sources
        let combined_freq_dir = target_language_dir.join("frequency_lists/combined");
        std::fs::create_dir_all(&combined_freq_dir)?;
        let frequencies_file = combined_freq_dir.join("frequencies.jsonl");
        {
            let frequencies = generate_data::frequencies::compute_frequencies(
                &nlp_sentences,
                course.target_language,
                &banned_words,
            );

            generate_data::frequencies::write_frequencies_file(frequencies, &frequencies_file)?;
        }
        let frequencies = {
            let file = File::open(&frequencies_file)?;
            let reader = BufReader::new(file);
            let frequencies = reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<language_utils::FrequencyEntry<String>>, _>>()?;
            frequencies
                .into_iter()
                .filter(|entry| entry.count > 3)
                .collect::<Vec<_>>()
        };

        // create and write dictionary
        let dict_file = native_specific_dir.join("dictionary.jsonl");
        let dictionary: BTreeMap<
            language_utils::Heteronym<String>,
            language_utils::DictionaryEntry,
        > = {
            let custom_definitions = {
                let file = File::open(source_data_path.join("custom_definitions.jsonl"))?;
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .map(|line| line.unwrap())
                    .filter(|line| !line.is_empty())
                    .map(|line| serde_json::from_str(&line))
                    .collect::<Result<
                        BTreeMap<
                            language_utils::Heteronym<String>,
                            language_utils::DictionaryEntryThoughts,
                        >,
                        serde_json::Error,
                    >>()?
            };

            let dictionary = generate_data::dict::create_dictionary(*course, &frequencies).await?;
            let morphology =
                morphology_analysis::create_morphology(course.target_language, &frequencies)
                    .await?;
            let dictionary = dictionary
                .into_iter()
                .filter_map(|(heteronym, def)| {
                    morphology
                        .get(&heteronym)
                        .map(|morphology| (heteronym, (def.clone(), morphology.clone())))
                })
                .map(|(heteronym, (def, morphology))| {
                    if let Some(def) = custom_definitions.get(&heteronym) {
                        (heteronym, (def.clone(), morphology))
                    } else {
                        (heteronym, (def, morphology))
                    }
                })
                .collect::<BTreeMap<_, _>>();

            // Write the dictionary to a jsonl file
            let mut file = File::create(dict_file)?;
            for entry in &dictionary {
                let json = serde_json::to_string(&entry)?;
                writeln!(file, "{json}")?;
            }
            dictionary
                .into_iter()
                .map(|(heteronym, thoughts)| (heteronym, thoughts.into()))
                .collect()
        };

        // Generate conjugations/declensions JSONL
        {
            let morphology_groups = morphology_analysis::analyze_morphology(&dictionary);

            let conjugations_path = native_specific_dir.join("conjugations.jsonl");
            morphology_analysis::write_conjugations_jsonl(&morphology_groups, &conjugations_path)?;
        }

        // create and write phrasebook
        let phrasebook_file = native_specific_dir.join("phrasebook.jsonl");
        let phrasebook: BTreeMap<String, language_utils::PhrasebookEntry> = {
            let phrasebook = generate_data::dict::create_phrasebook(*course, &frequencies).await?;
            let mut file = File::create(phrasebook_file)?;
            let phrasebook: BTreeMap<String, language_utils::PhrasebookEntry> = phrasebook
                .into_iter()
                .map(|(phrase, thoughts)| (phrase, thoughts.into()))
                .collect();
            for entry in phrasebook.iter() {
                let json = serde_json::to_string(&entry)?;
                writeln!(file, "{json}")?;
            }
            phrasebook
        };

        let wikipron_path = source_data_path.join("pronunciations.tsv").canonicalize()?;
        let extra_pronunciations_path = source_data_path
            .join("extra_pronunciations.tsv")
            .canonicalize()?;
        let word_to_pronunciation_file = target_language_dir.join("word_to_pronunciation.jsonl");
        let pronunciation_to_word_file = target_language_dir.join("pronunciation_to_words.jsonl");
        if !word_to_pronunciation_file.exists() || !pronunciation_to_word_file.exists() {
            // Create a set of words that appear in our frequency list for quick lookup
            let frequent_words: std::collections::HashSet<String> = all_lexemes
                .iter()
                .filter_map(|entry| entry.heteronym())
                .map(|h| h.word.clone())
                .collect();

            let phonetics_file = File::open(wikipron_path)?;
            let phonetics_file = BufReader::new(phonetics_file);
            let extra_phonetics_file = File::open(extra_pronunciations_path)?;
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
                .await?
                .into_iter()
                .collect::<BTreeMap<_, _>>();

            let pronunciation_to_words: std::collections::BTreeMap<
                String,
                std::collections::BTreeSet<String>,
            > = word_to_pronunciation
                .iter()
                .map(|(word, pronunciation)| (pronunciation.clone(), word.clone()))
                .into_group_map()
                .into_iter()
                .map(|(ipa, words)| (ipa, words.into_iter().collect()))
                .collect();

            // Convert to Vec format for ConsolidatedLanguageData
            let word_to_pronunciation: Vec<(String, String)> =
                word_to_pronunciation.into_iter().collect();
            let pronunciation_to_words: Vec<(String, Vec<String>)> = pronunciation_to_words
                .into_iter()
                .map(|(ipa, words)| (ipa, words.into_iter().collect()))
                .collect();

            let mut file = File::create(word_to_pronunciation_file)?;
            for (word, pronunciation) in &word_to_pronunciation {
                let json = serde_json::to_string(&(word, pronunciation))?;
                writeln!(file, "{json}")?;
            }
            let mut file = File::create(pronunciation_to_word_file)?;
            for (ipa, words) in &pronunciation_to_words {
                let json = serde_json::to_string(&(ipa, words))?;
                writeln!(file, "{json}")?;
            }
        }

        // Generate disambiguation practice data
        let homophones = generate_data::disambiguation_practice::generate_homophones(
            *course,
            &target_language_dir,
            &frequencies,
            1000,
        )?;

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
            .await?;

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
            .await?;

            let nlp = generate_data::nlp::generate_nlp_sentences(
                tokenizations,
                &multiword_terms_tokenizations,
                &target_language_nlp_file,
                course.target_language,
            )
            .await?;

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

        // Generate pronunciation sounds and guides
        let sounds_file = target_language_dir.join("pronunciation_sounds.jsonl");
        let guides_file = native_specific_dir.join("pronunciation_guides.jsonl");

        // Generate or load language sounds
        let sounds = if sounds_file.exists() {
            let file = File::open(&sounds_file)?;
            let reader = BufReader::new(file);
            let line = reader
                .lines()
                .next()
                .ok_or_else(|| anyhow::anyhow!("Empty sounds file"))??;
            serde_json::from_str(&line)?
        } else {
            let sounds = generate_data::pronunciation_patterns::generate_language_sounds(
                course.target_language,
            )
            .await?;

            // Save to file
            let mut file = File::create(&sounds_file)?;
            let json = serde_json::to_string(&sounds)?;
            writeln!(file, "{json}")?;

            sounds
        };

        // Generate or load pronunciation guides
        let guides = if guides_file.exists() {
            let file = File::open(&guides_file)?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| {
                    let line = line?;
                    Ok(serde_json::from_str(&line)?)
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?
        } else {
            let guides_with_thoughts =
                generate_data::pronunciation_patterns::generate_pronunciation_guides(
                    *course, &sounds,
                )
                .await?;

            // Save to file
            let mut file = File::create(&guides_file)?;
            for (sound, guide_thoughts) in &guides_with_thoughts {
                let json = serde_json::to_string(&(sound, guide_thoughts))?;
                writeln!(file, "{json}")?;
            }

            guides_with_thoughts
        };

        // We'll calculate pattern frequencies after loading word_to_pronunciation data later

        // Consolidate all JSON files into a single rkyv file
        let rkyv_file = native_specific_dir.join("language_data.rkyv");

        // Load all the JSON files
        let target_language_sentences = {
            let file = File::open(target_language_dir.join("target_language_sentences.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<String>, _>>()?
        };

        let translations = {
            let file = File::open(
                native_specific_dir.join("target_language_to_native_translations.jsonl"),
            )?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()?
        };

        // Calculate pattern frequencies using the word frequency data
        let pattern_freq_map = generate_data::pronunciation_patterns::calculate_pattern_frequencies(
            &sounds,
            &frequencies,
        );

        // Load and process phonetics data
        let word_to_pronunciation = {
            let file = File::open(target_language_dir.join("word_to_pronunciation.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, String)>, _>>()?
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
            let file = File::open(target_language_dir.join("pronunciation_to_words.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()?
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

        // Filter frequencies to only include lexemes that have definitions in dictionary/phrasebook
        let dictionary_set: std::collections::HashSet<_> = dictionary.keys().cloned().collect();
        let phrasebook_set: std::collections::HashSet<_> = phrasebook.keys().cloned().collect();

        let frequencies = frequencies
            .into_iter()
            .filter(|frequency| match &frequency.lexeme {
                language_utils::Lexeme::Heteronym(h) => dictionary_set.contains(h),
                language_utils::Lexeme::Multiword(m) => phrasebook_set.contains(m),
            })
            .collect::<Vec<_>>();

        // Filter sentences that contain words not in the frequency list
        let (nlp_sentences, _removed_sentences): (Vec<_>, Vec<_>) = {
            let lexeme_set = frequencies
                .iter()
                .map(|frequency| frequency.lexeme.clone())
                .collect::<std::collections::HashSet<_>>();
            nlp_sentences.into_iter().partition(|(_, sentence_info)| {
                // Check if sentence contains any infrequent lexeme
                sentence_info
                    .all_lexemes()
                    .all(|lexeme| lexeme_set.contains(&lexeme))
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

        // Validate that all multiword terms and heteronyms in nlp_sentences exist in the phrasebook/dictionary
        {
            let mut missing_multiwords = std::collections::HashSet::new();
            for (_sentence, info) in &nlp_sentences {
                for lexeme in info.lexemes() {
                    if let Some(multiword) = lexeme.multiword() {
                        if !phrasebook_set.contains(multiword) {
                            missing_multiwords.insert(multiword.clone());
                        }
                    }
                }
            }

            if !missing_multiwords.is_empty() {
                let mut missing_sorted: Vec<_> = missing_multiwords.into_iter().collect();
                missing_sorted.sort();
                panic!(
                    "Found {} multiword terms in NLP sentences that don't have phrasebook entries:\n{}",
                    missing_sorted.len(),
                    missing_sorted.join("\n")
                );
            }
        }

        {
            let mut missing_heteronyms = std::collections::HashSet::new();
            for (_sentence, info) in &nlp_sentences {
                for lexeme in info.lexemes() {
                    if let Some(heteronym) = lexeme.heteronym() {
                        if !dictionary_set.contains(heteronym) {
                            missing_heteronyms.insert(heteronym.clone());
                        }
                    }
                }
            }

            if !missing_heteronyms.is_empty() {
                let mut missing_sorted: Vec<_> = missing_heteronyms.into_iter().collect();
                missing_sorted.sort();
                panic!(
                    "Found {} heteronyms in NLP sentences that don't have dictionary entries:\n{:?}",
                    missing_sorted.len(),
                    missing_sorted
                );
            }
        }

        let (pronunciation_to_words, word_to_pronunciation) = {
            let words_set = frequencies
                .iter()
                .filter_map(|frequency| frequency.lexeme.heteronym())
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

        // Sort sentences by the frequency of their least common word

        // Create a frequency map for quick lookup
        let frequency_map: BTreeMap<_, _> = frequencies
            .iter()
            .map(|entry| (entry.lexeme.clone(), entry.count))
            .collect();

        // Create a map from sentence to its NLP info for quick lookup
        let sentence_to_info: BTreeMap<_, _> = nlp_sentences
            .iter()
            .map(|(sentence, info)| (sentence.clone(), info.clone()))
            .collect();

        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        enum SentenceWordFreq {
            Present(u32),
            NotPresent,
        }

        // Sort target_language_sentences by the frequency of their least common word
        target_language_sentences.sort_by_key(|sentence| {
            // Look up the NLP info for this sentence
            if let Some(info) = sentence_to_info.get(sentence) {
                // Find the three least common lexeme frequencies in the sentence
                let mut frequencies: Vec<_> = info
                    .all_lexemes()
                    .filter_map(|lexeme| frequency_map.get(&lexeme).copied())
                    .collect();
                frequencies.sort_unstable();

                let mut frequency_iter = frequencies.into_iter();
                let least_common = frequency_iter
                    .next()
                    .map(SentenceWordFreq::Present)
                    .unwrap_or(SentenceWordFreq::NotPresent);
                let second_least_common = frequency_iter
                    .next()
                    .map(SentenceWordFreq::Present)
                    .unwrap_or(SentenceWordFreq::NotPresent);
                let third_least_common = frequency_iter
                    .next()
                    .map(SentenceWordFreq::Present)
                    .unwrap_or(SentenceWordFreq::NotPresent);

                // Return reversed to sort descending (highest frequency first)
                std::cmp::Reverse((least_common, second_least_common, third_least_common))
            } else {
                // If no NLP info found, put at the end
                eprintln!("No NLP info found for sentence: {sentence}");
                std::cmp::Reverse((
                    SentenceWordFreq::NotPresent,
                    SentenceWordFreq::NotPresent,
                    SentenceWordFreq::NotPresent,
                ))
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
                let metadata_content = std::fs::read_to_string(&metadata_file)?;
                let posters_dir = movies_dir.join("posters");
                let mut movies = FxHashMap::default();

                for line in metadata_content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let basic: language_utils::MovieMetadataBasic = serde_json::from_str(line)?;

                    // Convert to full MovieMetadata and load poster bytes from separate file
                    let mut movie: language_utils::MovieMetadata = basic.into();
                    let poster_path = posters_dir.join(format!("{}.jpg", movie.id));
                    if poster_path.exists() {
                        if let Ok(bytes) = std::fs::read(&poster_path) {
                            movie.poster_bytes = Some(bytes);
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

        // Load sentence sources
        let sentence_sources = {
            let sentence_sources_file = target_language_dir.join("sentence_sources.jsonl");
            if sentence_sources_file.exists() {
                let file = File::open(&sentence_sources_file)?;
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .map(|line| serde_json::from_str(&line.unwrap()))
                    .collect::<Result<Vec<(String, language_utils::SentenceSource)>, _>>()?
            } else {
                Vec::new()
            }
        };

        // Compute per-movie frequencies
        let movie_frequencies = if !movies.is_empty() {
            let movie_ids: Vec<String> = movies.keys().cloned().collect();
            generate_data::frequencies::compute_movie_frequencies(
                &nlp_sentences,
                &sentence_sources,
                &movie_ids,
                course.target_language,
                &banned_words,
            )
        } else {
            FxHashMap::default()
        };

        // Create consolidated data structure
        let consolidated_data = language_utils::ConsolidatedLanguageData {
            target_language_sentences,
            translations,
            nlp_sentences,
            dictionary,
            phrasebook,
            frequencies,
            movie_frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
            pronunciation_data,
            homophone_practice,
            movies,
            sentence_sources,
        };

        let language_pack = language_utils::language_pack::LanguagePack::new(consolidated_data);

        // Serialize with rkyv
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&language_pack)?;
        std::fs::write(&rkyv_file, bytes)?;

        // Generate hash of the rkyv file
        let hash_file = native_specific_dir.join("language_data.hash");

        // Read the rkyv file and compute hash
        let rkyv_bytes = std::fs::read(&rkyv_file)?;
        let hash = const_xxh3(&rkyv_bytes);

        // Write hash to file
        std::fs::write(&hash_file, hash.to_string())?;
    }

    Ok(())
}
