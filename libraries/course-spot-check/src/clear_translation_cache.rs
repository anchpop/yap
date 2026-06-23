use anyhow::{Context as _, Result};
use chrono::{TimeZone, Utc};
use language_utils::{Course, Language};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use weapon::AppState;
use weapon::data_model::Timestamped;
use xxhash_rust::xxh3::xxh3_64;
use yap_frontend_rs::{
    Challenge, Deck, DeckState, TranscribeComprehensibleSentence, TranslateComprehensibleSentence,
};

const SENTENCES_PER_LANGUAGE: usize = 2_000;

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
        (Language::French, Language::Portuguese) => {
            include_bytes!("../../../out/por_for_fra/language_data.rkyv").to_vec()
        }
        (Language::English, Language::Russian) => {
            include_bytes!("../../../out/rus_for_eng/language_data.rkyv").to_vec()
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
        timezone: chrono::FixedOffset::east_opt(0).unwrap(),
    };
    let state = DeckState::new();
    let mut deck = <Deck as weapon::AppState>::finalize(state, &context);

    if let Some(event) = deck.get_no_cards_ready_info(vec![], None).smart_add_event {
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

fn collect_sentences(course: Course) -> Result<HashSet<String>> {
    let deck = create_deck_for_course(course)?;
    let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut simulator = deck.simulate_usage(fixed_time);
    let mut unique_sentences: HashSet<String> = HashSet::new();

    let pb = indicatif::ProgressBar::new(SENTENCES_PER_LANGUAGE as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sentences ({per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    'outer: for _day in 0..400 {
        let mut day = simulator.next_day();
        for challenge in day.by_ref() {
            let sentence = match challenge {
                Challenge::TranslateComprehensibleSentence(TranslateComprehensibleSentence {
                    target_language_literals,
                    ..
                }) => target_language_literals
                    .iter()
                    .flat_map(|literal| vec![literal.word.text.clone(), literal.whitespace.clone()])
                    .collect::<Vec<_>>()
                    .join(""),
                Challenge::TranscribeComprehensibleSentence(TranscribeComprehensibleSentence {
                    parts,
                    ..
                }) => parts
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
                    .join(""),
                _ => continue,
            };

            if unique_sentences.insert(sentence) {
                pb.set_position(unique_sentences.len() as u64);
                if unique_sentences.len() >= SENTENCES_PER_LANGUAGE {
                    break 'outer;
                }
            }
        }
        simulator = day.finish_day();
    }

    pb.finish_with_message("done");
    Ok(unique_sentences)
}

fn main() -> Result<()> {
    env_logger::init();

    let filter = std::env::args().nth(1);
    let cache_path = PathBuf::from(".cache/google_translate/master_cache.json");

    let mut cache: BTreeMap<String, String> = if cache_path.exists() {
        let content = std::fs::read_to_string(&cache_path)
            .with_context(|| format!("Failed to read {cache_path:?}"))?;
        serde_json::from_str(&content).context("Failed to parse master_cache.json")?
    } else {
        anyhow::bail!("No cache file at {cache_path:?}");
    };

    println!("Loaded {} cache entries", cache.len());

    let mut total_removed = 0;

    for course in language_utils::COURSES {
        if let Some(ref filter) = filter
            && course.target_language.iso_639_3() != filter
        {
            continue;
        }

        println!(
            "\n=== {:?} -> {:?} ===",
            course.native_language, course.target_language
        );

        let sentences = match collect_sentences(*course) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping course {course:?}: {e}");
                continue;
            }
        };

        // Translator is created with (target_language -> native_language),
        // so the cache key is "<target_iso>::<native_iso>::<text>".
        let src = course.target_language.iso_639_1();
        let tgt = course.native_language.iso_639_1();

        let mut removed = 0;
        for sentence in &sentences {
            let key = xxh3_64(format!("{src}::{tgt}::{sentence}").as_bytes()).to_string();
            if cache.remove(&key).is_some() {
                removed += 1;
            }
        }
        println!(
            "  Cleared {removed}/{} sentences from cache",
            sentences.len()
        );
        total_removed += removed;
    }

    println!(
        "\nWriting cache back ({} entries, {total_removed} removed)",
        cache.len()
    );
    let json = serde_json::to_string_pretty(&cache)?;
    std::fs::write(&cache_path, json)?;

    Ok(())
}
