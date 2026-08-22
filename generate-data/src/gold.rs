//! The hand-corrected gold token set (`out/cleaned_<lang>.jsonl`) and the one
//! place that reads it.
//!
//! Gold is the LLM-cleaned, human-reviewable analysis for a subset of the app
//! sentences. It has always overridden the teacher's silver in the training
//! export; [`overlay`] is what makes the *pipeline* honour it too, so a sentence
//! we have taken the trouble to get right is analyzed the same way in the app as
//! it is in lexide's training data.
//!
//! Everything here runs through `token_corrections::fix_tokens`, exactly as the
//! silver loader does. Gold is not exempt from the correction rules: it was
//! produced by a model too, and demonstrably reproduces some of the teacher's
//! mistakes (the French `les` article tagged `PRON` under a `det` relation
//! appears in both). Canonicalizing on load is also what lets a rule written
//! today repair gold that was cleaned under an older rule set, with no
//! re-cleaning run.

use anyhow::{Context, Result};
use language_utils::{Language, PartOfSpeechTag};
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use token_corrections::{PieceDep, TokenView, fix_tokens, lexide_pos_to_tag, tag_to_lexide_pos};

/// The gold `cleaned_*.jsonl` token schema: flat text/lemma, the dependency as
/// its UD label string. Every field is required — since English joined the
/// dependency pass every gold file carries dep/head, and a missing one should
/// fail loudly rather than be papered over.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CleanedToken {
    pub text: String,
    pub whitespace: String,
    pub pos: PartOfSpeechTag,
    pub lemma: String,
    pub dep: String,
    pub head: i32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CleanedSentence {
    pub sentence: String,
    pub tokens: Vec<CleanedToken>,
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
    fn dep_label(&self) -> Option<PieceDep> {
        PieceDep::from_ud_label(&self.dep)
    }
    fn set_dep_label(&mut self, dep: PieceDep) {
        self.dep = dep.ud_label().to_string();
    }
    fn copy_attachment(&mut self, from: &Self) {
        self.dep = from.dep.clone();
        self.head = from.head;
    }
}

/// Flatten lexide tokens into the gold schema.
pub fn to_flat(tokens: Vec<lexide::Token>) -> Vec<CleanedToken> {
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

/// The inverse of [`to_flat`], for feeding gold back into a pipeline that speaks
/// lexide tokens. The dependency round-trips through serde because that is how
/// [`to_flat`] wrote it, so the two stay in step by construction.
pub fn to_lexide(tokens: Vec<CleanedToken>) -> Result<Vec<lexide::Token>> {
    tokens
        .into_iter()
        .map(|t| {
            let dep = serde_json::from_value(serde_json::Value::String(t.dep.clone()))
                .with_context(|| format!("unknown dependency label {:?} in gold", t.dep))?;
            Ok(lexide::Token {
                text: lexide::Text { text: t.text },
                whitespace: t.whitespace,
                pos: tag_to_lexide_pos(t.pos),
                lemma: lexide::Lemma { lemma: t.lemma },
                dep,
                head: t.head,
            })
        })
        .collect()
}

/// Path of a language's gold set, alongside the per-language `out/` dirs.
pub fn gold_path(out_dir: &Path, language: Language) -> PathBuf {
    out_dir.join(format!("cleaned_{}.jsonl", language.code()))
}

/// Load a gold `cleaned_*.jsonl`, canonicalized, keyed by sentence.
pub fn load(path: &Path, language: Language) -> Result<BTreeMap<String, Vec<CleanedToken>>> {
    let mut gold = BTreeMap::new();
    let reader = std::io::BufReader::new(
        std::fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?,
    );
    for line in reader.lines() {
        let mut record: CleanedSentence = serde_json::from_str(&line?)?;
        fix_tokens(language, &mut record.tokens);
        gold.insert(record.sentence, record.tokens);
    }
    Ok(gold)
}

type GoldCache = BTreeMap<Language, std::sync::Arc<BTreeMap<String, Vec<lexide::Token>>>>;

/// A language's gold set as lexide tokens, loaded once per process.
///
/// Cached because every tokenization store consults it and the files run to
/// tens of megabytes; an absent file is a legitimate answer (most languages have
/// no gold yet) and caches as empty.
fn load_as_lexide(
    language: Language,
) -> Result<std::sync::Arc<BTreeMap<String, Vec<lexide::Token>>>> {
    static CACHE: OnceLock<Mutex<GoldCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(hit) = cache.lock().unwrap().get(&language) {
        return Ok(hit.clone());
    }
    let path = gold_path(Path::new("./out"), language);
    let loaded = if path.exists() {
        let flat = load(&path, language)?;
        let mut out = BTreeMap::new();
        for (sentence, tokens) in flat {
            out.insert(sentence, to_lexide(tokens)?);
        }
        println!(
            "gold[{}]: {} hand-corrected sentences loaded from {}",
            language.code(),
            out.len(),
            path.display()
        );
        out
    } else {
        BTreeMap::new()
    };
    let loaded = std::sync::Arc::new(loaded);
    cache
        .lock()
        .unwrap()
        .insert(language, std::sync::Arc::clone(&loaded));
    Ok(loaded)
}

/// Serve every requested sentence we have a gold analysis for from gold.
///
/// Scoped to `wanted` rather than to all of gold, because the pipeline analyzes
/// the corpus it was asked for — the training export injects gold-only
/// sentences since more data helps there, but a sentence nobody requested has
/// no business appearing in a store the app reads.
///
/// Both effects matter. A requested sentence already in the silver store has
/// its analysis replaced; a requested sentence the silver store never saw is
/// *added*, which also means it is no longer a cache miss and costs no
/// tokenizer call.
pub fn overlay<'a>(
    language: Language,
    wanted: impl IntoIterator<Item = &'a str>,
    store: &mut BTreeMap<String, Vec<lexide::Token>>,
) -> Result<()> {
    let gold = load_as_lexide(language)?;
    if gold.is_empty() {
        return Ok(());
    }
    let (mut replaced, mut added) = (0usize, 0usize);
    for sentence in wanted {
        let Some(tokens) = gold.get(sentence) else {
            continue;
        };
        match store.insert(sentence.to_string(), tokens.clone()) {
            Some(_) => replaced += 1,
            None => added += 1,
        }
    }
    if replaced > 0 || added > 0 {
        println!(
            "gold[{}]: {replaced} sentences taken from gold instead of silver, \
             {added} supplied by gold alone",
            language.code(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_and_lexide_round_trip() {
        let original = vec![
            lexide::Token {
                text: lexide::Text {
                    text: "les".to_string(),
                },
                whitespace: " ".to_string(),
                pos: lexide::pos::PartOfSpeech::Det,
                lemma: lexide::Lemma {
                    lemma: "le".to_string(),
                },
                dep: lexide::DependencyRelation::Det,
                head: 2,
            },
            lexide::Token {
                text: lexide::Text {
                    text: "gars".to_string(),
                },
                whitespace: String::new(),
                pos: lexide::pos::PartOfSpeech::Noun,
                lemma: lexide::Lemma {
                    lemma: "gars".to_string(),
                },
                dep: lexide::DependencyRelation::Root,
                head: 0,
            },
        ];
        let flat = to_flat(original.clone());
        // The dep survives as its UD label, which is how gold stores it.
        assert_eq!(flat[0].dep, "det");
        let back = to_lexide(flat).unwrap();
        assert_eq!(format!("{back:?}"), format!("{original:?}"));
    }

    #[test]
    fn unknown_dependency_label_fails_loudly() {
        let bogus = vec![CleanedToken {
            text: "les".to_string(),
            whitespace: String::new(),
            pos: PartOfSpeechTag::Det,
            lemma: "le".to_string(),
            dep: "not-a-relation".to_string(),
            head: 0,
        }];
        // Gold is hand-editable, so a typo in a dep must stop the run rather
        // than silently become some default relation.
        let err = to_lexide(bogus).unwrap_err().to_string();
        assert!(err.contains("not-a-relation"), "{err}");
    }

    #[test]
    fn gold_articles_are_corrected_on_load() {
        // Gold reproduces the teacher's PRON-under-det error, so the loader's
        // correction pass has to reach it through CleanedToken's dep_label.
        let mut tokens = vec![CleanedToken {
            text: "les".to_string(),
            whitespace: " ".to_string(),
            pos: PartOfSpeechTag::Pron,
            lemma: "le".to_string(),
            dep: "det".to_string(),
            head: 2,
        }];
        fix_tokens(Language::French, &mut tokens);
        assert_eq!(tokens[0].pos, PartOfSpeechTag::Det);
    }
}
