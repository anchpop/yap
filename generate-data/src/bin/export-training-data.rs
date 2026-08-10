//! Export the complete, canonical training dataset in one command:
//!
//!     cargo run --release -p generate-data --bin export-training-data [out-dir] [export-dir]
//!
//! Defaults: out-dir `out`, export-dir `out/training-data`. Everything comes out in
//! ONE format — the flat gold token schema (`{text, whitespace, pos, lemma, dep,
//! head}`) — and everything has been through the deterministic correction pass
//! (`token_corrections::fix_tokens`). The raw stores are read-only throughout; they
//! keep the model's uncorrected output so the rules stay revisable.
//!
//! Per language, the export contains:
//! - `<lang>/target_language_sentences_tokenization.jsonl` — the merged best
//!   available analysis per sentence: gold's answer wherever a sentence appears in
//!   `cleaned_<lang>.jsonl` (overriding the teacher's silver), the canonicalized
//!   silver otherwise, plus gold-only sentences the silver store never saw.
//! - `<lang>/…restricted…` / `…multiword…` / `…augmented….jsonl` — canonicalized
//!   silver, format-normalized.
//! - `cleaned_<lang>.jsonl` — the gold set alone, also run through the correction
//!   pass (a no-op for freshly-cleaned languages, but gold generated under an older
//!   rule set comes out canonical without an LLM re-run).
//!
//! The export dir works directly as lexide data_prep's `--big-dir` and
//! `--gold-dir`. Note that gold rows then appear both in the merged store (as
//! silver-kind) and in `cleaned_*` (as gold-kind) — identical tokens, so the only
//! effect is oversampling gold.

use anyhow::Result;
use language_utils::PartOfSpeechTag;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use token_corrections::{PieceDep, TokenView, fix_tokens, lexide_pos_to_tag};

/// The export's one token format — the gold `cleaned_*.jsonl` schema: flat
/// text/lemma, the dependency as its UD label string. Every field is required:
/// since English joined the dependency pass, every gold file carries dep/head,
/// and a missing field should fail the export loudly rather than be papered over.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct CleanedToken {
    text: String,
    whitespace: String,
    pos: PartOfSpeechTag,
    lemma: String,
    dep: String,
    head: i32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CleanedSentence {
    sentence: String,
    tokens: Vec<CleanedToken>,
}

impl TokenView for CleanedToken {
    fn text(&self) -> &str {
        &self.text
    }
    fn whitespace(&self) -> &str {
        &self.whitespace
    }
    fn pos(&self) -> PartOfSpeechTag {
        self.pos
    }
    fn lemma(&self) -> &str {
        &self.lemma
    }
    fn push_text(&mut self, more: &str) {
        self.text.push_str(more);
    }
    fn set_text(&mut self, text: String) {
        self.text = text;
    }
    fn set_whitespace(&mut self, ws: String) {
        self.whitespace = ws;
    }
    fn set_pos(&mut self, pos: PartOfSpeechTag) {
        self.pos = pos;
    }
    fn set_lemma(&mut self, lemma: String) {
        self.lemma = lemma;
    }
    fn head(&self) -> i32 {
        self.head
    }
    fn set_head(&mut self, head: i32) {
        self.head = head;
    }
    fn set_dep_label(&mut self, dep: PieceDep) {
        self.dep = dep.ud_label().to_string();
    }
    fn copy_attachment(&mut self, from: &Self) {
        self.dep = from.dep.clone();
        self.head = from.head;
    }
}

fn to_flat(tokens: Vec<lexide::Token>) -> Vec<CleanedToken> {
    tokens
        .into_iter()
        .map(|t| CleanedToken {
            text: t.text.text,
            whitespace: t.whitespace,
            pos: lexide_pos_to_tag(t.pos),
            lemma: t.lemma.lemma,
            dep: serde_json::to_value(t.dep)
                .expect("dep serializes")
                .as_str()
                .expect("dep is a string")
                .to_string(),
            head: t.head,
        })
        .collect()
}

/// Load a gold `cleaned_*.jsonl`, canonicalized, keyed by sentence.
fn load_gold(
    path: &Path,
    language: language_utils::Language,
) -> Result<BTreeMap<String, Vec<CleanedToken>>> {
    let mut gold = BTreeMap::new();
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    for line in reader.lines() {
        let mut record: CleanedSentence = serde_json::from_str(&line?)?;
        fix_tokens(language, &mut record.tokens);
        gold.insert(record.sentence, record.tokens);
    }
    Ok(gold)
}

/// Write entries as flat-format jsonl, sorted by sentence (deterministic export).
fn write_flat(dest: &Path, entries: &BTreeMap<String, Vec<CleanedToken>>) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = std::io::BufWriter::new(std::fs::File::create(dest)?);
    for (sentence, tokens) in entries {
        let json = serde_json::json!({ "sentence": sentence, "tokens": tokens });
        writeln!(writer, "{json}")?;
    }
    writer.flush()?;
    Ok(())
}

/// The silver stores exported without gold merging (the gold pool draws from the
/// main app-sentence corpus, not from these).
const UNMERGED_STORES: &[&str] = &[
    "restricted_sentences_tokenization.jsonl",
    "target_language_multiword_terms_tokenization.jsonl",
    "target_language_sentences_tokenization_augmented.jsonl",
];

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let out_dir = args.next().unwrap_or_else(|| "out".to_string());
    let export_dir = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(&out_dir).join("training-data"));

    for entry in std::fs::read_dir(&out_dir)? {
        let path = entry?.path();
        let Some(code) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(language) = language_utils::Language::from_code(code) else {
            continue; // non-language dirs (including the export dir itself)
        };

        let gold_path = Path::new(&out_dir).join(format!("cleaned_{code}.jsonl"));
        let gold = if gold_path.exists() {
            load_gold(&gold_path, language)?
        } else {
            BTreeMap::new()
        };

        // main store: gold's answer wins, gold-only sentences are included
        let main_store = path.join("target_language_sentences_tokenization.jsonl");
        if main_store.exists() {
            let mut merged: BTreeMap<String, Vec<CleanedToken>> =
                generate_data::nlp::load_canonicalized(&main_store, language)?
                    .into_iter()
                    .map(|(sentence, tokens)| (sentence, to_flat(tokens)))
                    .collect();
            let silver_n = merged.len();
            let overridden = gold.keys().filter(|s| merged.contains_key(*s)).count();
            merged.extend(gold.iter().map(|(s, t)| (s.clone(), t.clone())));
            write_flat(
                &export_dir
                    .join(code)
                    .join("target_language_sentences_tokenization.jsonl"),
                &merged,
            )?;
            println!(
                "{code}: merged store {} entries ({silver_n} silver; gold overrode {overridden}, added {})",
                merged.len(),
                gold.len() - overridden,
            );
        }

        for store in UNMERGED_STORES {
            let source = path.join(store);
            if !source.exists() {
                continue;
            }
            let entries: BTreeMap<String, Vec<CleanedToken>> =
                generate_data::nlp::load_canonicalized(&source, language)?
                    .into_iter()
                    .map(|(sentence, tokens)| (sentence, to_flat(tokens)))
                    .collect();
            write_flat(&export_dir.join(code).join(store), &entries)?;
            println!("{code}: {store} {} entries", entries.len());
        }

        if !gold.is_empty() {
            write_flat(&export_dir.join(format!("cleaned_{code}.jsonl")), &gold)?;
            println!("{code}: gold {} entries", gold.len());
        }
    }
    Ok(())
}
