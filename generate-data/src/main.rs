use anyhow::Context;
use futures::StreamExt;
use itertools::Itertools;
use language_utils::{COURSES, NlpAnalyzedSentence, SentenceInfo, strip_punctuation};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

mod google_translate;
use google_translate::GoogleTranslator;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    for course in COURSES {
        let output_dir = format!("./out/{}", course.target_language.iso_639_3());
        let output_dir = Path::new(output_dir.as_str())
            .canonicalize()
            .context("Failed to canonicalize output directory")?;
        // ensure output_dir exists
        std::fs::create_dir_all(output_dir.clone())?;

        let source_data_path = format!(
            "./generate-data/data/{}",
            course.target_language.iso_639_3()
        );
        let source_data_path = Path::new(source_data_path.as_str());

        // Load banned sentences
        let banned_sentences_file = source_data_path.join("banned_sentences.txt");
        let banned_sentences = if banned_sentences_file.exists() {
            let content = std::fs::read_to_string(banned_sentences_file)
                .context("Failed to read banned sentences file")?;
            content
                .lines()
                .map(|line| line.trim().to_lowercase())
                .filter(|line| !line.is_empty())
                .collect::<std::collections::HashSet<_>>()
        } else {
            println!("No banned sentences file found, proceeding without filtering");
            std::collections::HashSet::new()
        };

        if !banned_sentences.is_empty() {
            println!("Loaded {} banned sentences", banned_sentences.len());
        }

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
            println!("No banned words file found, proceeding without filtering");
            std::collections::HashSet::new()
        };

        if !banned_words.is_empty() {
            println!("Loaded {} banned words", banned_words.len());
        }

        // write sentences
        let target_language_sentences_file = output_dir.join("target_language_sentences.jsonl");
        let translations_file = output_dir.join("target_language_to_native_translations.jsonl");
        if target_language_sentences_file.exists() && translations_file.exists() {
            println!("Skipping sentence writing because files already exist");
        } else {
            let mut total_sentences = 0;

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

            let all_cards = generate_data::read_anki::get_all_cards(source_data_path);
            let mut all_sentences = futures::stream::iter(
                all_cards
                    .iter()
                    .flat_map(|card| {
                        card.target.iter().map(|target_language_sentence| {
                            (target_language_sentence.clone(), card.native.clone())
                        })
                    })
                    .filter(|(target_language_sentence, _)| {
                        !banned_sentences.contains(&target_language_sentence.to_lowercase())
                    })
                    .map(async move |(target_language_sentence, native_sentence)| {
                        let mut translator = GoogleTranslator::new(
                            course.target_language, // translate from target to native
                            course.native_language,
                            PathBuf::from(".cache/google_translate"),
                        )
                        .unwrap();

                        let mut translation_set = BTreeSet::new();
                        translation_set.insert(native_sentence);
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
                        (target_language_sentence, translation_set)
                    }),
            )
            .buffered(100);

            while let Some((target_language_sentence, native_translations)) =
                all_sentences.next().await
            {
                // Write individual target language sentence
                let target_language_json = serde_json::to_string(&target_language_sentence)?;
                if let Err(e) = writeln!(target_language_writer, "{target_language_json}") {
                    eprintln!("Error writing to target_language sentences file: {e}");
                }

                let translation_json =
                    serde_json::to_string(&(&target_language_sentence, native_translations))?;
                if let Err(e) = writeln!(translations_writer, "{translation_json}") {
                    eprintln!("Error writing to translations file: {e}");
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

            println!(
                "\nTotal sentences written to {} and {}: {}",
                target_language_sentences_file.display(),
                translations_file.display(),
                total_sentences
            );
        }

        // Process sentences with Python NLP to detect multiword terms;
        let target_language_nlp_file = output_dir.join("target_language_sentences_nlp.jsonl");
        if target_language_nlp_file.exists() {
            println!("Skipping Python NLP because file already exists");
        } else {
            // potentially skip this for now because it's slow
            if true {
                println!("\nProcessing sentences with Python NLP...");

                // Ensure multiword terms file exists, download if needed
                let multiword_terms_file =
                    generate_data::wiktionary::ensure_multiword_terms_file(course, &output_dir)
                        .await?;

                // Run the Python script
                let script: &str = "main.py";
                let script_path = Path::new("./generate-data/nlp/")
                    .canonicalize()
                    .context("Failed to canonicalize script path")?;
                let status = Command::new("uv")
                    .arg("run")
                    .arg(script)
                    .arg(course.target_language.iso_639_3())
                    .arg(&target_language_sentences_file)
                    .arg(&multiword_terms_file)
                    .arg(&target_language_nlp_file)
                    .current_dir(script_path)
                    .status()
                    .context(format!("Failed to run Python script {script}."))?;

                if status.success() {
                    println!("Successfully processed sentences with multiword terms.");
                    println!(
                        "Multiword terms for sentences written to: {}",
                        target_language_nlp_file.display()
                    );
                } else {
                    return Err(anyhow::anyhow!(
                        "Python script failed with exit code: {:?}",
                        status.code()
                    ));
                }
            }
        }

        let nlp_sentences = {
            // read the nlp file
            let nlp_file = File::open(target_language_nlp_file)?;
            let reader = BufReader::new(nlp_file);
            let nlp_sentences: Vec<language_utils::NlpAnalyzedSentence> = reader
                .lines()
                .map(|line| {
                    let line = line.unwrap();
                    serde_json::from_str::<language_utils::NlpAnalyzedSentence>(&line)
                        .unwrap_or_else(|e| {
                            panic!("Could not deserialize to NlpAnalyzedSentence: `{line}` {e:?}")
                        })
                })
                .map(
                    |NlpAnalyzedSentence {
                         sentence,
                         doc,
                         multiword_terms,
                     }| NlpAnalyzedSentence {
                        sentence,
                        doc,
                        multiword_terms,
                    },
                )
                .collect::<Vec<_>>();

            nlp_sentences
        };

        let proper_nouns_file = output_dir.join("corrected_proper_nouns.jsonl");
        if proper_nouns_file.exists() {
            println!("Skipping proper nouns filter because file already exists");
        } else {
            let known_never_proper_nouns = source_data_path.join("never-proper-nouns.txt");
            let known_never_proper_nouns = BufReader::new(File::open(known_never_proper_nouns)?)
                .lines()
                .map(|line| line.unwrap())
                .filter(|line| !line.is_empty())
                .map(|word| {
                    serde_json::from_str::<(String, language_utils::Heteronym<String>)>(&word).map(
                        |(word, heteronym)| (word, language_utils::Lexeme::Heteronym(heteronym)),
                    )
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;

            let proper_nouns = {
                let mut proper_nouns = BTreeMap::new();
                for sentence in nlp_sentences.iter() {
                    for token in sentence.doc.iter() {
                        if token.pos == language_utils::PartOfSpeech::Propn {
                            proper_nouns
                                .entry(strip_punctuation(&token.text).to_string())
                                .or_insert(BTreeSet::new())
                                .insert(sentence.sentence.clone());
                        }
                    }
                }
                proper_nouns
            };
            let proper_nouns: BTreeMap<String, language_utils::Lexeme<String>> =
                generate_data::proper_noun_filter::correct_proper_nouns(*course, proper_nouns)
                    .await?
                    .into_iter()
                    .chain(known_never_proper_nouns.into_iter())
                    .collect();
            let mut file = File::create(&proper_nouns_file)?;
            for (word, lexeme) in proper_nouns {
                let json = serde_json::to_string(&(word, lexeme))?;
                writeln!(file, "{json}")?;
            }
        }
        let proper_nouns = {
            let file = File::open(&proper_nouns_file)?;
            let reader = BufReader::new(file);
            let lexemes = reader
                .lines()
                .map(|line| {
                    serde_json::from_str::<(String, language_utils::Lexeme<String>)>(&line.unwrap())
                })
                .collect::<Result<Vec<(String, language_utils::Lexeme<String>)>, _>>()?;

            lexemes
                .into_iter()
                .filter_map(|(word, lexeme)| {
                    if let language_utils::Lexeme::Heteronym(heteronym) = lexeme {
                        Some((word, heteronym))
                    } else {
                        None
                    }
                })
                .collect::<BTreeMap<_, _>>()
        };

        let nlp_sentences: Vec<(String, SentenceInfo)> = nlp_sentences
            .into_iter()
            .map(|analysis| {
                (
                    analysis.sentence.clone(),
                    SentenceInfo::from_nlp_analyzed_sentence(
                        analysis,
                        &proper_nouns,
                        course.target_language,
                    ),
                )
            })
            .collect();

        let all_lexemes: Vec<language_utils::Lexeme<String>> = nlp_sentences
            .iter()
            .flat_map(|(_, analysis)| analysis.all_lexemes())
            .filter(|lexeme| match lexeme {
                language_utils::Lexeme::Heteronym(heteronym) => !banned_words.contains(heteronym),
                _ => true,
            })
            .collect();

        // create and write dictionary
        let dict_file = output_dir.join("dictionary.jsonl");
        if dict_file.exists() {
            println!("Skipping dictionary creation because file already exists");
        } else {
            let dictionary =
                generate_data::dict::create_dictionary(*course, &nlp_sentences).await?;

            let custom_definitions = {
                let file = File::open(source_data_path.join("custom_definitions.jsonl"))?;
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .map(|line| line.unwrap())
                    .filter(|line| !line.is_empty())
                    .map(|line| serde_json::from_str(&line))
                    .collect::<Result<
                        Vec<(
                            language_utils::Heteronym<String>,
                            language_utils::DictionaryEntryThoughts,
                        )>,
                        serde_json::Error,
                    >>()?
            };
            let dictionary = dictionary
                .into_iter()
                .chain(custom_definitions.into_iter())
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect::<Vec<_>>();

            // Write the dictionary to a jsonl file
            let mut file = File::create(dict_file)?;
            for entry in dictionary {
                let json = serde_json::to_string(&entry)?;
                writeln!(file, "{json}")?;
            }
        }

        // create and write phrasebook
        let phrasebook_file = output_dir.join("phrasebook.jsonl");
        if phrasebook_file.exists() {
            println!("Skipping phrasebook creation because file already exists");
        } else {
            let phrasebook =
                generate_data::dict::create_phrasebook(*course, &nlp_sentences).await?;
            let mut file = File::create(phrasebook_file)?;
            for entry in phrasebook {
                let json = serde_json::to_string(&entry)?;
                writeln!(file, "{json}")?;
            }
        }

        // Generate frequencies file for anki source
        let anki_freq_dir = output_dir.join("frequency_lists/anki");
        std::fs::create_dir_all(&anki_freq_dir)?;
        let frequencies_file = anki_freq_dir.join("frequencies.jsonl");
        if frequencies_file.exists() {
            println!("Skipping frequencies creation because file already exists");
        } else {
            println!("\nGenerating word and phrase frequencies from Anki source...");

            let frequencies = generate_data::frequencies::compute_frequencies(all_lexemes.clone());
            println!("Computed {} frequencies", frequencies.len());

            generate_data::frequencies::write_frequencies_file(frequencies, &frequencies_file)?;

            println!("Frequencies written to: {}", frequencies_file.display());
        }

        let wikipron_path = source_data_path.join("pronunciations.tsv").canonicalize()?;
        let extra_pronunciations_path = source_data_path
            .join("extra_pronunciations.tsv")
            .canonicalize()?;
        let word_to_pronunciation_file = output_dir.join("word_to_pronunciation.jsonl");
        let pronunciation_to_word_file = output_dir.join("pronunciation_to_words.jsonl");
        if word_to_pronunciation_file.exists() && pronunciation_to_word_file.exists() {
            println!(
                "Skipping word to pronunciation and pronunciation to word creation because files already exist"
            );
        } else {
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
            for (word, pronunciation) in word_to_pronunciation {
                let json = serde_json::to_string(&(word, pronunciation))?;
                writeln!(file, "{json}")?;
            }
            let mut file = File::create(pronunciation_to_word_file)?;
            for (ipa, words) in pronunciation_to_words {
                let json = serde_json::to_string(&(ipa, words))?;
                writeln!(file, "{json}")?;
            }
        }

        // Consolidate all JSON files into a single rkyv file
        let rkyv_file = output_dir.join("language_data.rkyv");
        println!("\nConsolidating all data into rkyv format...");

        // Load all the JSON files
        println!("Loading target_language sentences...");
        let target_language_sentences = {
            let file = File::open(output_dir.join("target_language_sentences.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<String>, _>>()?
        };

        println!("Loading translations...");
        let translations = {
            let file = File::open(output_dir.join("target_language_to_native_translations.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()?
        };

        println!("Loading dictionary...");
        let dictionary = {
            let file = File::open(output_dir.join("dictionary.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .map(
                    |result: Result<(_, language_utils::DictionaryEntryThoughts), _>| {
                        result.map(|(heteronym, thoughts)| (heteronym, thoughts.into()))
                    },
                )
                .collect::<Result<
                    Vec<(
                        language_utils::Heteronym<String>,
                        language_utils::DictionaryEntry,
                    )>,
                    _,
                >>()?
        };

        println!("Loading phrasebook...");
        let phrasebook = {
            let file = File::open(output_dir.join("phrasebook.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .map(
                    |result: Result<(_, language_utils::PhrasebookEntryThoughts), _>| {
                        result.map(|(heteronym, thoughts)| (heteronym, thoughts.into()))
                    },
                )
                .collect::<Result<Vec<(String, language_utils::PhrasebookEntry)>, _>>()?
        };

        println!("Loading frequencies...");
        let frequencies = {
            // For now, we'll load from the anki frequency file
            let anki_freq_file = output_dir.join("frequency_lists/anki/frequencies.jsonl");
            let file = File::open(&anki_freq_file)?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<language_utils::FrequencyEntry<String>>, _>>()?
        };

        // Load and process phonetics data
        println!("Loading phonetics data...");
        let word_to_pronunciation = {
            let file = File::open(output_dir.join("word_to_pronunciation.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, String)>, _>>()?
        };
        let pronunciation_to_words = {
            let file = File::open(output_dir.join("pronunciation_to_words.jsonl"))?;
            let reader = BufReader::new(file);
            reader
                .lines()
                .map(|line| serde_json::from_str(&line.unwrap()))
                .collect::<Result<Vec<(String, Vec<String>)>, _>>()?
        };

        // ensure all sentences in the NLP analysis are in the target_language_sentences list
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

        // ensure the dictionary and phrasebook don't contain any words that are not in the frequencies list
        let dictionary = {
            let words_set = frequencies
                .iter()
                .filter_map(|frequency| frequency.lexeme.heteronym())
                .collect::<std::collections::HashSet<_>>();
            // also add in the custom definitions
            dictionary
                .into_iter()
                .filter(|(heteronym, _)| words_set.contains(heteronym))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect()
        };
        let phrasebook = {
            let phrase_set = frequencies
                .iter()
                .filter_map(|frequency| frequency.lexeme.multiword())
                .collect::<std::collections::HashSet<_>>();
            phrasebook
                .into_iter()
                .filter(|(phrase, _)| phrase_set.contains(phrase))
                .collect::<Vec<_>>()
        };

        // Create consolidated data structure
        let consolidated_data = language_utils::ConsolidatedLanguageData {
            target_language_sentences,
            translations,
            nlp_sentences,
            dictionary,
            phrasebook,
            frequencies,
            word_to_pronunciation,
            pronunciation_to_words,
        };

        let mut rodeo = lasso::Rodeo::new();
        consolidated_data.intern(&mut rodeo);

        println!(
            "Rodeo memory usage WITHOUT setting capacity: {} bytes",
            rodeo.current_memory_usage()
        );
        let num_strings = rodeo.strings().len() as u32;
        let num_string_bytes = rodeo.strings().map(|s| s.len()).sum::<usize>() as u32;
        let consolidated_data_with_capacity =
            language_utils::ConsolidatedLanguageDataWithCapacity {
                consolidated_language_data: consolidated_data,
                num_strings,
                num_string_bytes,
            };
        let rodeo = consolidated_data_with_capacity.intern();
        println!(
            "Rodeo memory usage WITH setting capacity: {} bytes",
            rodeo.current_memory_usage()
        );
        println!("(Interned {num_strings} strings, {num_string_bytes} bytes)");

        // Serialize with rkyv
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&consolidated_data_with_capacity)?;
        std::fs::write(&rkyv_file, bytes)?;

        println!("Consolidated data written to: {}", rkyv_file.display());
        println!("File size: {} bytes", std::fs::metadata(&rkyv_file)?.len());

        // Generate hash of the rkyv file
        let hash_file = output_dir.join("language_data.hash");
        println!("Generating hash of rkyv file...");

        // Read the rkyv file and compute hash
        let rkyv_bytes = std::fs::read(&rkyv_file)?;
        let hash = const_xxh3(&rkyv_bytes);

        // Write hash to file
        std::fs::write(&hash_file, hash.to_string())?;

        println!("Hash written to: {}", hash_file.display());
        println!("Hash: {hash}");
    }

    Ok(())
}
