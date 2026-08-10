//! Export canonicalized tokenization data for training the student tokenizer.
//!
//! The stores under `out/<lang>/` hold the teacher model's RAW output; the
//! deterministic corrections (`token_corrections::fix_tokens`) are applied at load
//! time everywhere inside generate-data, so the app is built from the canonical
//! form while the raw form stays on disk and the rules stay revisable. Training
//! pipelines (lexide's data_prep) read files instead of going through generate-data,
//! so this bin materializes the canonical view for them:
//!
//!     cargo run --release -p generate-data --bin export-training-data [out-dir] [export-dir]
//!
//! Defaults: out-dir `out`, export-dir `out/training-data`. The export mirrors the
//! `<lang>/<store>.jsonl` layout, covering every language (for languages with no
//! correction rules the content matches the raw store) and every store, the
//! synthetic-augmentation files included — so `out/training-data` is the complete
//! canonical training set, usable directly as data_prep's `--big-dir` (or synced
//! over lexide's `data/big` in one copy).

use anyhow::Result;

const STORES: &[&str] = &[
    "target_language_sentences_tokenization.jsonl",
    "restricted_sentences_tokenization.jsonl",
    "target_language_multiword_terms_tokenization.jsonl",
    // lexide's synthetic-augmentation sentences, labelled by the teacher on the
    // lexide side and synced into out/ so this one command covers every store
    "target_language_sentences_tokenization_augmented.jsonl",
];

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let out_dir = args.next().unwrap_or_else(|| "out".to_string());
    let export_dir = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(&out_dir).join("training-data"));

    for entry in std::fs::read_dir(&out_dir)? {
        let path = entry?.path();
        // non-language directories (including the export dir itself) don't parse
        let Some(code) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(language) = language_utils::Language::from_code(code) else {
            continue;
        };
        for store in STORES {
            let source = path.join(store);
            if !source.exists() {
                continue;
            }
            let dest = export_dir.join(code).join(store);
            let entries = generate_data::nlp::export_canonicalized(&source, language, &dest)?;
            println!(
                "{} → {} ({entries} entries)",
                source.display(),
                dest.display()
            );
        }
    }
    Ok(())
}
