use generate_data::morphology;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let lang = std::env::args().nth(1).unwrap_or_else(|| "eng".to_string());
    let canonical_path =
        PathBuf::from(format!("generate-data/data/{lang}/canonical_morphemes.tsv"));

    if !canonical_path.exists() {
        eprintln!("No canonical_morphemes.tsv for language: {lang}");
        std::process::exit(1);
    }

    let test_words_by_lang: Vec<String> = match lang.as_str() {
        "eng" => vec![
            // Unseen words (unigram segmentation)
            "megatransformational",
            "rebookmarking",
            "prediagnostical",
            "counterdestabilizing",
            // Known words (aligned from canonical data)
            "destabilization",
            "uncomfortable",
            "bookmarking",
            "internationally",
        ],
        "kor" => vec![
            "꽃가게",     // flower shop
            "손가락질",   // finger-pointing
            "물고기잡이", // fishing
            "새해맞이",   // new year greeting
            "길거리음식", // street food
            "해돋이",     // sunrise
            "마음먹다",   // make up one's mind
            "짝사랑",     // unrequited love
        ],
        "fra" => vec![
            "déstabilisation",
            "incompréhensible",
            "anticonstitutionnellement",
            "redécouvrir",
            "malheureusement",
        ],
        "deu" => vec![
            // Unknown compounds
            "Donaudampfschifffahrt",
            "Fußballweltmeisterschaft",
            "Kindergartenerzieherin",
            "Straßenbahnhaltestelle",
            "Wohnungseinrichtung",
            // Known
            "Handschuh",
            "Schadenfreude",
            "Bestellung",
            "unglaublich",
        ],
        _ => vec![],
    }
    .into_iter()
    .map(|s| s.to_string())
    .collect();

    let target_pieces = match lang.as_str() {
        "kor" | "jpn" | "hin" => 1000,
        _ => 3000,
    };

    let start = Instant::now();
    let segmentations = morphology::build_morphology_segmentations(
        &canonical_path,
        &test_words_by_lang,
        target_pieces,
    );
    println!("Built in {:.1?}\n", start.elapsed());

    println!("Segmentations for {lang}:");
    for word in &test_words_by_lang {
        if let Some(segments) = segmentations.get(word) {
            println!("  {word} → {}", segments.join(" | "));
        } else {
            println!("  {word} → (no segmentation)");
        }
    }
}
