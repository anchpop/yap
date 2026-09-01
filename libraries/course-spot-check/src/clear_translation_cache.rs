use anyhow::{Context as _, Result};
use chrono::{TimeZone, Utc};
use language_utils::Course;
use std::collections::HashSet;
use weapon::AppState;
use weapon::data_model::Timestamped;
use xxhash_rust::xxh3::xxh3_64;
use yap_frontend_rs::{
    Challenge, Deck, DeckState, TranscribeComprehensibleSentence, TranslateComprehensibleSentence,
};

const SENTENCES_PER_LANGUAGE: usize = 2_000;

fn create_deck_for_course(course: Course) -> Result<Deck> {
    let dir =
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../out")).join(format!(
            "{}_for_{}",
            course.target_language.code(),
            course.native_language.code()
        ));
    let language_pack = language_utils::language_pack::load_split_dir(&dir)?;
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
            timezone: Some(context.timezone),
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let filter = std::env::args().nth(1);
    // Translations live in the shared cache store under `translate/{hash}`;
    // deleting appends a tombstone, so the clear syncs to other machines too.
    let store = osmo::Store::open(".cache");

    let mut total_removed = 0;

    for course in language_utils::COURSES {
        if let Some(ref filter) = filter
            && course.target_language.code() != filter
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
        // so the legacy Google cache key is "<target_iso>::<native_iso>::<text>".
        let src = course.target_language.iso_639_1();
        let tgt = course.native_language.iso_639_1();

        let mut removed = 0;
        for sentence in &sentences {
            let hash = xxh3_64(format!("{src}::{tgt}::{sentence}").as_bytes());
            let key = format!("translate/{hash}");
            if store.read(&key).await.is_some() {
                store
                    .delete(&key)
                    .await
                    .with_context(|| format!("Failed to delete {key}"))?;
                removed += 1;
            }
        }
        println!(
            "  Cleared {removed}/{} sentences from cache",
            sentences.len()
        );
        total_removed += removed;
    }

    println!("\nRemoved {total_removed} cached translations");

    Ok(())
}
