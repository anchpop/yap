//! Deterministic, idempotent per-language token corrections — the single home for
//! tokenization/POS/lemma policy that a model can't be trusted to apply
//! consistently. Two consumers: generate-data's silver tokenization pipeline
//! (`nlp::process_sentences` applies these at load time over the raw lexide
//! stores) and clean-nlp-data's LLM cleaning (both the NLP proposal it shows the
//! LLM and the LLM's output). Keeping one implementation here is what keeps the
//! two datasets consistent — the corrections exist precisely because the corpus
//! used to write the same string both ways.
//!
//! Everything operates through [`TokenView`], a common window onto the three token
//! types the pipelines use: `language_utils::DocToken` (spaCy-side proposals),
//! clean-nlp-data's `SimplifiedTokenPrime` (LLM output; implemented over there), and
//! `lexide::Token` (silver data). lexide tokens also carry a dependency head, so the
//! structural edits (splits and merges) renumber heads across the sentence; token
//! types without heads report 0 for every head and the renumbering is a no-op.
//!
//! The segmentation tables came from an audit of strings our own corpus tokenizes
//! more than one way: an entry qualifies only when the same context shows up under
//! both segmentations (so the variation is noise, not meaning) and the cleaning
//! prompt's policy or the cleaned data's clear majority fixes a single correct form.
//! A string whose segmentation legitimately depends on the sentence — Japanese のに
//! (conjunction) vs の|に (purposive), Korean 밖에 ("only") vs 밖|에 ("outside at"),
//! Thai ที่อยู่ ("address") vs ที่|อยู่ (relativizer + "live"), Chinese 的话 ("if") vs
//! 的|话 ("'s words") — must be left to the LLM and must NOT be added to these tables.

use language_utils::{Language, PartOfSpeechTag};

/// Common view over the token types the correctors touch. Lets a deterministic fix be
/// written once and applied identically to the NLP proposal, the LLM output, and the
/// silver lexide data.
pub trait TokenView {
    fn text(&self) -> &str;
    fn whitespace(&self) -> &str;
    fn pos(&self) -> PartOfSpeechTag;
    fn lemma(&self) -> &str;
    fn push_text(&mut self, more: &str);
    fn set_text(&mut self, text: String);
    fn set_whitespace(&mut self, ws: String);
    fn set_pos(&mut self, pos: PartOfSpeechTag);
    fn set_lemma(&mut self, lemma: String);

    /// 1-based dependency head (0 = root) for token types that carry one.
    /// The defaults make head bookkeeping a no-op for head-less token types.
    fn head(&self) -> i32 {
        0
    }
    fn set_head(&mut self, _head: i32) {}
    fn set_dep_label(&mut self, _dep: PieceDep) {}
    /// The token's dependency label, for the rules that key off syntax rather
    /// than surface. `None` both for token types that carry no dependency and
    /// for labels outside [`PieceDep`], so a dep-conditioned rule is a no-op on
    /// those rather than silently misfiring on an unknown relation.
    fn dep_label(&self) -> Option<PieceDep> {
        None
    }
    /// Take another token's dependency attachment (label + head). Used when a merge
    /// discards the piece that carried the pair's external attachment.
    fn copy_attachment(&mut self, _from: &Self)
    where
        Self: Sized,
    {
    }
}

macro_rules! impl_token_view {
    ($ty:ty) => {
        impl TokenView for $ty {
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
        }
    };
}

impl_token_view!(language_utils::DocToken);

/// lexide's POS enum and language-utils' are the same UPOS set; public so
/// format-normalizing exporters can convert without a serde round-trip.
pub fn lexide_pos_to_tag(pos: lexide::pos::PartOfSpeech) -> PartOfSpeechTag {
    use lexide::pos::PartOfSpeech as P;
    match pos {
        P::Adj => PartOfSpeechTag::Adj,
        P::Adp => PartOfSpeechTag::Adp,
        P::Adv => PartOfSpeechTag::Adv,
        P::Aux => PartOfSpeechTag::Aux,
        P::Cconj => PartOfSpeechTag::Cconj,
        P::Det => PartOfSpeechTag::Det,
        P::Intj => PartOfSpeechTag::Intj,
        P::Noun => PartOfSpeechTag::Noun,
        P::Num => PartOfSpeechTag::Num,
        P::Part => PartOfSpeechTag::Part,
        P::Pron => PartOfSpeechTag::Pron,
        P::Propn => PartOfSpeechTag::Propn,
        P::Punct => PartOfSpeechTag::Punct,
        P::Sconj => PartOfSpeechTag::Sconj,
        P::Sym => PartOfSpeechTag::Sym,
        P::Verb => PartOfSpeechTag::Verb,
        P::Space => PartOfSpeechTag::Space,
        P::X => PartOfSpeechTag::X,
    }
}

pub fn tag_to_lexide_pos(pos: PartOfSpeechTag) -> lexide::pos::PartOfSpeech {
    use lexide::pos::PartOfSpeech as P;
    match pos {
        PartOfSpeechTag::Adj => P::Adj,
        PartOfSpeechTag::Adp => P::Adp,
        PartOfSpeechTag::Adv => P::Adv,
        PartOfSpeechTag::Aux => P::Aux,
        PartOfSpeechTag::Cconj => P::Cconj,
        PartOfSpeechTag::Det => P::Det,
        PartOfSpeechTag::Intj => P::Intj,
        PartOfSpeechTag::Noun => P::Noun,
        PartOfSpeechTag::Num => P::Num,
        PartOfSpeechTag::Part => P::Part,
        PartOfSpeechTag::Pron => P::Pron,
        PartOfSpeechTag::Propn => P::Propn,
        PartOfSpeechTag::Punct => P::Punct,
        PartOfSpeechTag::Sconj => P::Sconj,
        PartOfSpeechTag::Sym => P::Sym,
        PartOfSpeechTag::Verb => P::Verb,
        PartOfSpeechTag::Space => P::Space,
        PartOfSpeechTag::X => P::X,
    }
}

impl TokenView for lexide::Token {
    fn text(&self) -> &str {
        &self.text.text
    }
    fn whitespace(&self) -> &str {
        &self.whitespace
    }
    fn pos(&self) -> PartOfSpeechTag {
        lexide_pos_to_tag(self.pos)
    }
    fn lemma(&self) -> &str {
        &self.lemma.lemma
    }
    fn push_text(&mut self, more: &str) {
        self.text.text.push_str(more);
    }
    fn set_text(&mut self, text: String) {
        self.text.text = text;
    }
    fn set_whitespace(&mut self, ws: String) {
        self.whitespace = ws;
    }
    fn set_pos(&mut self, pos: PartOfSpeechTag) {
        self.pos = tag_to_lexide_pos(pos);
    }
    fn set_lemma(&mut self, lemma: String) {
        self.lemma.lemma = lemma;
    }
    fn head(&self) -> i32 {
        self.head
    }
    fn set_head(&mut self, head: i32) {
        self.head = head;
    }
    fn dep_label(&self) -> Option<PieceDep> {
        use lexide::DependencyRelation as D;
        Some(match self.dep {
            D::Advmod => PieceDep::Advmod,
            D::Aux => PieceDep::Aux,
            D::Case => PieceDep::Case,
            D::Clf => PieceDep::Clf,
            D::Compound => PieceDep::Compound,
            D::CompoundLvc => PieceDep::CompoundLvc,
            D::Cop => PieceDep::Cop,
            D::Det => PieceDep::Det,
            D::Discourse => PieceDep::Discourse,
            D::Fixed => PieceDep::Fixed,
            D::Mark => PieceDep::Mark,
            D::Nummod => PieceDep::Nummod,
            D::Obj => PieceDep::Obj,
            _ => return None,
        })
    }
    fn set_dep_label(&mut self, dep: PieceDep) {
        use lexide::DependencyRelation as D;
        self.dep = match dep {
            PieceDep::Advmod => D::Advmod,
            PieceDep::Aux => D::Aux,
            PieceDep::Case => D::Case,
            PieceDep::Clf => D::Clf,
            PieceDep::Compound => D::Compound,
            PieceDep::CompoundLvc => D::CompoundLvc,
            PieceDep::Cop => D::Cop,
            PieceDep::Det => D::Det,
            PieceDep::Discourse => D::Discourse,
            PieceDep::Fixed => D::Fixed,
            PieceDep::Mark => D::Mark,
            PieceDep::Nummod => D::Nummod,
            PieceDep::Obj => D::Obj,
        };
    }
    fn copy_attachment(&mut self, from: &Self) {
        self.dep = from.dep;
        self.head = from.head;
    }
}

/// Dependency label a non-head split piece gets, for token types that carry deps.
/// A small subset of UD — just what the tables below need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceDep {
    Advmod,
    Aux,
    Case,
    Clf,
    Compound,
    CompoundLvc,
    Cop,
    Det,
    Discourse,
    Fixed,
    Mark,
    Nummod,
    Obj,
}

impl PieceDep {
    /// The Universal Dependencies label string, for token types that store the
    /// dependency as text (the gold cleaned_*.jsonl format) rather than as
    /// lexide's enum.
    pub fn ud_label(self) -> &'static str {
        match self {
            PieceDep::Advmod => "advmod",
            PieceDep::Aux => "aux",
            PieceDep::Case => "case",
            PieceDep::Clf => "clf",
            PieceDep::Compound => "compound",
            PieceDep::CompoundLvc => "compound:lvc",
            PieceDep::Cop => "cop",
            PieceDep::Det => "det",
            PieceDep::Discourse => "discourse",
            PieceDep::Fixed => "fixed",
            PieceDep::Mark => "mark",
            PieceDep::Nummod => "nummod",
            PieceDep::Obj => "obj",
        }
    }

    /// Inverse of [`ud_label`](Self::ud_label), for token types that store the
    /// dependency as text. `None` for any label outside this enum, which keeps
    /// dep-conditioned rules inert on relations they were not written for.
    pub fn from_ud_label(label: &str) -> Option<Self> {
        [
            PieceDep::Advmod,
            PieceDep::Aux,
            PieceDep::Case,
            PieceDep::Clf,
            PieceDep::Compound,
            PieceDep::CompoundLvc,
            PieceDep::Cop,
            PieceDep::Det,
            PieceDep::Discourse,
            PieceDep::Fixed,
            PieceDep::Mark,
            PieceDep::Nummod,
            PieceDep::Obj,
        ]
        .into_iter()
        .find(|d| d.ud_label() == label)
    }
}

/// Where a split piece attaches. Exactly one piece per split is `Head`. The labels
/// were read off the cleaned gold data's dominant convention for each construction.
#[derive(Clone, Copy, Debug)]
pub enum PieceAttach {
    /// The syntactic head of the split: inherits the original token's dep and head.
    Head,
    /// Attaches to the head piece with this label (Korean 가 → case → 내).
    ToHead(PieceDep),
    /// Attaches to the same head as the head piece, with its own label — for pieces
    /// that hang off something *outside* the split, like the head piece does (Thai
    /// ควร|จะ: both aux on the external verb; Korean 은|요: case + discourse on the
    /// external noun).
    Sibling(PieceDep),
    /// Attaches to the token immediately before the split (Japanese は marks the
    /// preceding nominal). Falls back to the head piece at sentence start.
    Prev(PieceDep),
}

/// One split-table piece: (text, pos, lemma, attachment).
type Piece = (&'static str, PartOfSpeechTag, &'static str, PieceAttach);

/// One language's deterministic re-segmentation table. `splits` maps a token the
/// policy says is two-or-more words onto its pieces; `merges` maps an adjacent,
/// contiguous pair the policy says is one word onto the POS/lemma of the whole.
struct SegmentationRules {
    splits: &'static [(&'static str, &'static [Piece])],
    merges: &'static [(&'static str, &'static str, PartOfSpeechTag, &'static str)],
}

/// Split `tokens[i]` into `pieces`. Any leading whitespace embedded in the text (a
/// sentence-initial token can carry it) stays on the first piece, and the trailing
/// whitespace field stays on the last, so the sentence still reconstructs exactly.
/// Dependency heads across the sentence are renumbered around the widened span.
fn split_token<T: TokenView + Clone>(
    tokens: &mut Vec<T>,
    i: usize,
    pieces: &[(&str, PartOfSpeechTag, &str, PieceAttach)],
) {
    let n = pieces.len() as i32;
    let p = (i + 1) as i32; // 1-based position of the original token
    let head_off = pieces
        .iter()
        .position(|(_, _, _, a)| matches!(a, PieceAttach::Head))
        .expect("every split has a Head piece") as i32;
    let new_head_pos = p + head_off;

    // Renumber before cloning, so the head piece inherits an already-adjusted head.
    for t in tokens.iter_mut() {
        let h = t.head();
        if h > p {
            t.set_head(h + n - 1);
        } else if h == p {
            t.set_head(new_head_pos);
        }
    }

    let template = tokens[i].clone();
    let text = template.text().to_string();
    let lead = &text[..text.len() - text.trim_start().len()];
    for (j, (piece, pos, lemma, attach)) in pieces.iter().enumerate() {
        let mut tok = template.clone();
        tok.set_text(if j == 0 {
            format!("{lead}{piece}")
        } else {
            (*piece).to_string()
        });
        tok.set_pos(*pos);
        tok.set_lemma((*lemma).to_string());
        if j + 1 < pieces.len() {
            tok.set_whitespace(String::new());
        }
        match attach {
            PieceAttach::Head => {} // keeps the template's dep and (adjusted) head
            PieceAttach::ToHead(dep) => {
                tok.set_dep_label(*dep);
                tok.set_head(new_head_pos);
            }
            PieceAttach::Sibling(dep) => {
                tok.set_dep_label(*dep); // keeps the template's external head
            }
            PieceAttach::Prev(dep) => {
                tok.set_dep_label(*dep);
                tok.set_head(if p > 1 { p - 1 } else { new_head_pos });
            }
        }
        if j == 0 {
            tokens[i] = tok;
        } else {
            tokens.insert(i + j, tok);
        }
    }
}

/// Merge the contiguous `tokens[i + 1]` into `tokens[i]`, giving the whole the
/// supplied POS and lemma. Trailing whitespace follows the right-hand piece. If the
/// left piece hung off the right one, the merged token takes the right's external
/// attachment; heads across the sentence are renumbered around the closed gap.
fn merge_pair<T: TokenView>(tokens: &mut Vec<T>, i: usize, pos: PartOfSpeechTag, lemma: &str) {
    let p_l = (i + 1) as i32;
    let p_r = (i + 2) as i32;
    let right = tokens.remove(i + 1);
    let left = &mut tokens[i];
    if left.head() == p_r && right.head() != p_l {
        left.copy_attachment(&right);
    }
    left.push_text(right.text());
    left.set_whitespace(right.whitespace().to_string());
    left.set_pos(pos);
    left.set_lemma(lemma.to_string());
    for t in tokens.iter_mut() {
        let h = t.head();
        if h == p_r {
            t.set_head(p_l);
        } else if h > p_r {
            t.set_head(h - 1);
        }
    }
    // A mutually-pointing pair (malformed input) would leave the merged token
    // pointing at itself; demote it to root rather than keep a cycle.
    if tokens[i].head() == p_l {
        tokens[i].set_head(0);
    }
}

fn apply_segmentation<T: TokenView + Clone>(
    tokens: &mut Vec<T>,
    rules: &SegmentationRules,
) -> Vec<String> {
    let mut fixes = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if i + 1 < tokens.len() && tokens[i].whitespace().is_empty() {
            let left = tokens[i].text().trim().to_string();
            let right = tokens[i + 1].text().to_string();
            if let Some((_, _, pos, lemma)) = rules
                .merges
                .iter()
                .find(|(l, r, _, _)| *l == left && *r == right)
            {
                fixes.push(format!("Merged '{left}' + '{right}' into one token"));
                merge_pair(tokens, i, *pos, lemma);
                continue; // the merged token may itself start another merge
            }
        }
        let text = tokens[i].text().trim().to_string();
        if let Some((_, pieces)) = rules.splits.iter().find(|(s, _)| *s == text) {
            fixes.push(format!(
                "Split '{text}' into {}",
                pieces
                    .iter()
                    .map(|(t, _, _, _)| *t)
                    .collect::<Vec<_>>()
                    .join("|")
            ));
            split_token(tokens, i, pieces);
            i += pieces.len();
            continue;
        }
        i += 1;
    }
    fixes
}

// ---------------------------------------------------------------------------------
// Chinese
// ---------------------------------------------------------------------------------

/// Modal verbs that are AUX before another verb and VERB standalone.
pub const ZH_MODAL_VERBS: &[&str] = &[
    "会", "能", "能够", "可以", "应该", "应当", "必须", "敢", "肯", "愿意", "想", "要", "得",
];

/// Deterministic per-token fixes, applied identically to the NLP proposal, the LLM
/// output, and the silver lexide data.
fn fix_chinese_token(token: &mut impl TokenView) -> Vec<String> {
    let mut fixes = Vec::new();

    // Chinese words do not inflect: the lemma is the surface form, for every token.
    // (Trimmed — the first token of a sentence may carry leading whitespace in its text
    // to keep the reconstruction invariant, and that whitespace is not part of the word.)
    let want_lemma = token.text().trim();
    if token.lemma() != want_lemma && !want_lemma.is_empty() {
        fixes.push(format!(
            "Set lemma of '{}' to its surface form (was '{}')",
            token.text().trim(),
            token.lemma()
        ));
        token.set_lemma(want_lemma.to_string());
    }

    // 的 is always the structural particle in modern text — attributive, genitive, or
    // nominalizing, all PART. (的 as part of a content word never stands alone.)
    if token.text().trim() == "的" && token.pos() != PartOfSpeechTag::Part {
        fixes.push(format!("Retagged 的 from {:?} to PART", token.pos()));
        token.set_pos(PartOfSpeechTag::Part);
    }

    // Standalone 了 is always a particle (perfective or change-of-state). The verb
    // reading liǎo only survives inside compounds (了解, 受不了), which are one token.
    if token.text().trim() == "了" && token.pos() != PartOfSpeechTag::Part {
        fixes.push(format!("Retagged 了 from {:?} to PART", token.pos()));
        token.set_pos(PartOfSpeechTag::Part);
    }

    fixes
}

/// Deterministic context fixes over the whole token sequence.
fn fix_chinese_context<T: TokenView>(tokens: &mut [T]) -> Vec<String> {
    let mut fixes = Vec::new();

    for i in 0..tokens.len() {
        let text = tokens[i].text().trim().to_string();
        if !ZH_MODAL_VERBS.contains(&text.as_str()) {
            continue;
        }
        // The complement-marker 得 (PART) is a different word; only the verb readings
        // participate in the modal rule.
        if !matches!(
            tokens[i].pos(),
            PartOfSpeechTag::Verb | PartOfSpeechTag::Aux
        ) {
            continue;
        }
        // Walk past adverbs (negation, degree, time: 不, 没, 也, 再, 很) to the word the
        // modal would govern.
        let next_pos = tokens[i + 1..]
            .iter()
            .map(|t| t.pos())
            .find(|p| *p != PartOfSpeechTag::Adv);
        match next_pos {
            // Modal before a verb: AUX (我会说中文, 你应该不去).
            Some(PartOfSpeechTag::Verb | PartOfSpeechTag::Aux) => {
                if tokens[i].pos() == PartOfSpeechTag::Verb {
                    fixes.push(format!("Retagged modal {text} before a verb to AUX"));
                    tokens[i].set_pos(PartOfSpeechTag::Aux);
                }
            }
            // Standalone modal — sentence-final or followed only by particles/punctuation:
            // VERB (我不会。, 他会的). Anything else (nouns, 把-phrases…) is left for the
            // LLM: the verb the modal governs may sit further right.
            None | Some(PartOfSpeechTag::Part | PartOfSpeechTag::Punct)
                if tokens[i].pos() == PartOfSpeechTag::Aux =>
            {
                fixes.push(format!("Retagged standalone modal {text} to VERB"));
                tokens[i].set_pos(PartOfSpeechTag::Verb);
            }
            _ => {}
        }
    }

    fixes
}

/// Chinese canonical segmentation. The prompt's policy is fine-grained CTB: negator,
/// particle, and measure-word boundaries stay (不要 → 不|要, 这|个, 吃|了), while
/// compounds the dictionary lists stay whole (没有, 怎么, 什么). Both the segmenter
/// and the LLM drift on these — the corpus writes 不能 both ways in the very same
/// contexts — so the policy is enforced here.
const ZH_SEGMENTATION: SegmentationRules = SegmentationRules {
    splits: &[
        // 不 + verb/modal: compositional negation, never one word under this policy.
        // (Lexicalized 不错/不过/不同 etc. are real dictionary words and stay whole.)
        (
            "不是",
            &[
                (
                    "不",
                    PartOfSpeechTag::Adv,
                    "不",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
                ("是", PartOfSpeechTag::Verb, "是", PieceAttach::Head),
            ],
        ),
        // modals land as AUX; fix_chinese_context retags the standalone case to VERB
        (
            "不要",
            &[
                (
                    "不",
                    PartOfSpeechTag::Adv,
                    "不",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
                ("要", PartOfSpeechTag::Aux, "要", PieceAttach::Head),
            ],
        ),
        (
            "不会",
            &[
                (
                    "不",
                    PartOfSpeechTag::Adv,
                    "不",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
                ("会", PartOfSpeechTag::Aux, "会", PieceAttach::Head),
            ],
        ),
        (
            "不能",
            &[
                (
                    "不",
                    PartOfSpeechTag::Adv,
                    "不",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
                ("能", PartOfSpeechTag::Aux, "能", PieceAttach::Head),
            ],
        ),
        (
            "不行",
            &[
                (
                    "不",
                    PartOfSpeechTag::Adv,
                    "不",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
                ("行", PartOfSpeechTag::Adj, "行", PieceAttach::Head),
            ],
        ),
        // A-not-A question: 有 + the dictionary word 没有
        (
            "有没有",
            &[
                ("有", PartOfSpeechTag::Verb, "有", PieceAttach::Head),
                (
                    "没有",
                    PartOfSpeechTag::Adv,
                    "没有",
                    PieceAttach::ToHead(PieceDep::Advmod),
                ),
            ],
        ),
        // demonstrative/numeral + measure word
        (
            "这个",
            &[
                ("这", PartOfSpeechTag::Det, "这", PieceAttach::Head),
                (
                    "个",
                    PartOfSpeechTag::Noun,
                    "个",
                    PieceAttach::ToHead(PieceDep::Clf),
                ),
            ],
        ),
        (
            "那个",
            &[
                ("那", PartOfSpeechTag::Det, "那", PieceAttach::Head),
                (
                    "个",
                    PartOfSpeechTag::Noun,
                    "个",
                    PieceAttach::ToHead(PieceDep::Clf),
                ),
            ],
        ),
        (
            "一个",
            &[
                (
                    "一",
                    PartOfSpeechTag::Num,
                    "一",
                    PieceAttach::ToHead(PieceDep::Nummod),
                ),
                ("个", PartOfSpeechTag::Noun, "个", PieceAttach::Head),
            ],
        ),
        (
            "一个人",
            &[
                (
                    "一",
                    PartOfSpeechTag::Num,
                    "一",
                    PieceAttach::ToHead(PieceDep::Nummod),
                ),
                (
                    "个",
                    PartOfSpeechTag::Noun,
                    "个",
                    PieceAttach::ToHead(PieceDep::Clf),
                ),
                ("人", PartOfSpeechTag::Noun, "人", PieceAttach::Head),
            ],
        ),
        (
            "什么时候",
            &[
                (
                    "什么",
                    PartOfSpeechTag::Det,
                    "什么",
                    PieceAttach::ToHead(PieceDep::Det),
                ),
                ("时候", PartOfSpeechTag::Noun, "时候", PieceAttach::Head),
            ],
        ),
        // word + sentence particle
        (
            "好吗",
            &[
                ("好", PartOfSpeechTag::Adj, "好", PieceAttach::Head),
                (
                    "吗",
                    PartOfSpeechTag::Part,
                    "吗",
                    PieceAttach::ToHead(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "好吧",
            &[
                ("好", PartOfSpeechTag::Adj, "好", PieceAttach::Head),
                (
                    "吧",
                    PartOfSpeechTag::Part,
                    "吧",
                    PieceAttach::ToHead(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "好了",
            &[
                ("好", PartOfSpeechTag::Adj, "好", PieceAttach::Head),
                (
                    "了",
                    PartOfSpeechTag::Part,
                    "了",
                    PieceAttach::ToHead(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "好的",
            &[
                ("好", PartOfSpeechTag::Adj, "好", PieceAttach::Head),
                (
                    "的",
                    PartOfSpeechTag::Part,
                    "的",
                    PieceAttach::ToHead(PieceDep::Fixed),
                ),
            ],
        ),
        (
            "是的",
            &[
                ("是", PartOfSpeechTag::Verb, "是", PieceAttach::Head),
                (
                    "的",
                    PartOfSpeechTag::Part,
                    "的",
                    PieceAttach::ToHead(PieceDep::Fixed),
                ),
            ],
        ),
        // verb + aspect particle
        (
            "看着",
            &[
                ("看", PartOfSpeechTag::Verb, "看", PieceAttach::Head),
                (
                    "着",
                    PartOfSpeechTag::Part,
                    "着",
                    PieceAttach::ToHead(PieceDep::Aux),
                ),
            ],
        ),
    ],
    merges: &[
        // dictionary words the segmenter/LLM sometimes shred
        ("没", "有", PartOfSpeechTag::Adv, "没有"),
        ("没", "事", PartOfSpeechTag::Adj, "没事"),
        ("怎", "么", PartOfSpeechTag::Adv, "怎么"),
        ("什", "么", PartOfSpeechTag::Pron, "什么"),
    ],
};

/// The full deterministic pass for Chinese (Simplified or Traditional; the table
/// entries are Simplified strings and simply never match Traditional text).
fn fix_chinese_once<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = apply_segmentation(tokens, &ZH_SEGMENTATION);
    // 个人 directly after a determiner or numeral can only be measure word + 人 —
    // the noun 个人 "personal" never follows 每/一/这/那/哪/几/两. (Standalone 个人
    // stays whole: it is a real dictionary word.)
    let mut i = 1;
    while i < tokens.len() {
        if tokens[i].text().trim() == "个人"
            && tokens[i - 1].whitespace().is_empty()
            && matches!(
                tokens[i - 1].text().trim(),
                "每" | "一" | "这" | "那" | "哪" | "几" | "两"
            )
        {
            fixes.push("Split 个人 after a determiner/numeral into 个|人".to_string());
            split_token(
                tokens,
                i,
                &[
                    (
                        "个",
                        PartOfSpeechTag::Noun,
                        "个",
                        PieceAttach::ToHead(PieceDep::Clf),
                    ),
                    ("人", PartOfSpeechTag::Noun, "人", PieceAttach::Head),
                ],
            );
            i += 2;
            continue;
        }
        i += 1;
    }
    for token in tokens.iter_mut() {
        fixes.extend(fix_chinese_token(token));
    }
    fixes.extend(fix_chinese_context(tokens.as_mut_slice()));
    fixes
}

// ---------------------------------------------------------------------------------
// Thai
// ---------------------------------------------------------------------------------

/// Preverbal tense-aspect-mood markers that are AUX when they modify a following
/// verb: irrealis จะ, progressive กำลัง, experiential เคย, and the modals. ได้ is
/// deliberately absent: its preverbal (past attainment), postverbal (potential
/// "can"), and main-verb ("get") readings need context the adjacency rule can't see.
pub const TH_PREVERBAL_AUX: &[&str] = &[
    "จะ",
    "กำลัง",
    "เคย",
    "ต้อง",
    "ควร",
    "อาจ",
    "คง",
    "มัก",
    "น่าจะ",
    "ย่อม",
];

/// Words with no content reading at all: the negator ไม่ and the politeness/mood
/// particles. Always PART. (Question particle ไหม is absent — it is also the noun
/// "silk"; the sentence-final case is handled in `fix_thai_context`.)
pub const TH_ALWAYS_PART: &[&str] = &[
    "ไม่",
    "ครับ",
    "ค่ะ",
    "คะ",
    "นะ",
    "จ้ะ",
    "จ๊ะ",
    "ฮะ",
    "เถอะ",
    "หรอก",
    "สิ",
    "ล่ะ",
];

/// Deterministic per-token fixes, applied identically to every pipeline's tokens.
fn fix_thai_token(token: &mut impl TokenView) -> Vec<String> {
    let mut fixes = Vec::new();

    // Thai words do not inflect: the lemma is the surface form, for every token —
    // including colloquial spellings (เค้า, มั้ย), which keep their own spelling.
    // (Trimmed — the first token of a sentence may carry leading whitespace.)
    let want_lemma = token.text().trim();
    if token.lemma() != want_lemma && !want_lemma.is_empty() {
        fixes.push(format!(
            "Set lemma of '{}' to its surface form (was '{}')",
            token.text().trim(),
            token.lemma()
        ));
        token.set_lemma(want_lemma.to_string());
    }

    let text = token.text().trim().to_string();
    if TH_ALWAYS_PART.contains(&text.as_str()) && token.pos() != PartOfSpeechTag::Part {
        fixes.push(format!("Retagged {} from {:?} to PART", text, token.pos()));
        token.set_pos(PartOfSpeechTag::Part);
    }

    // The repetition mark ๆ is a grammatical sign (pluralize/intensify the previous
    // word), not punctuation — PART, its own token.
    if text == "ๆ" && token.pos() != PartOfSpeechTag::Part {
        fixes.push(format!("Retagged ๆ from {:?} to PART", token.pos()));
        token.set_pos(PartOfSpeechTag::Part);
    }

    fixes
}

/// Deterministic context fixes over the whole token sequence.
fn fix_thai_context<T: TokenView>(tokens: &mut [T]) -> Vec<String> {
    let mut fixes = Vec::new();

    for i in 0..tokens.len() {
        let text = tokens[i].text().trim().to_string();
        if !TH_PREVERBAL_AUX.contains(&text.as_str()) {
            continue;
        }
        if !matches!(
            tokens[i].pos(),
            PartOfSpeechTag::Verb | PartOfSpeechTag::Aux
        ) {
            continue;
        }
        // Walk past negation (ไม่ — PART) and adverbs (ก็, ยัง, เพิ่ง) to the word the
        // marker would govern.
        let next_pos = tokens[i + 1..]
            .iter()
            .map(|t| t.pos())
            .find(|p| !matches!(p, PartOfSpeechTag::Adv | PartOfSpeechTag::Part));
        // Marker before a verb: AUX (ผมจะไป, เขาคงไม่มา). The standalone direction is
        // left to the LLM: a bare ต้อง/ควร can still govern an elided verb.
        if matches!(next_pos, Some(PartOfSpeechTag::Verb | PartOfSpeechTag::Aux))
            && tokens[i].pos() == PartOfSpeechTag::Verb
        {
            fixes.push(format!(
                "Retagged preverbal marker {text} before a verb to AUX"
            ));
            tokens[i].set_pos(PartOfSpeechTag::Aux);
        }
    }

    // A sentence-final question particle — last token, or followed only by
    // punctuation and other particles (ไปไหมครับ) — is PART. Mid-sentence ไหม can be
    // the noun "silk", so position matters.
    for i in 0..tokens.len() {
        let text = tokens[i].text().trim().to_string();
        if !matches!(text.as_str(), "ไหม" | "มั้ย" | "เหรอ" | "หรอ") {
            continue;
        }
        let only_tail = tokens[i + 1..]
            .iter()
            .all(|t| matches!(t.pos(), PartOfSpeechTag::Punct | PartOfSpeechTag::Part));
        if only_tail && tokens[i].pos() != PartOfSpeechTag::Part {
            fixes.push(format!(
                "Retagged sentence-final question particle {text} to PART"
            ));
            tokens[i].set_pos(PartOfSpeechTag::Part);
        }
    }

    fixes
}

/// Thai canonical segmentation. Splits follow the prompt's compositional classes —
/// verb + object (ทำ|งาน like กิน|ข้าว), preverbal marker chains (ควร|จะ; both are
/// listed markers), ได้ before a verb (AUX), and the complementizer ว่า — and merges
/// follow its lexicalized classes: พวก-pronouns (like พวกเขา), the pronoun ทุกคน,
/// directional verb compounds (เข้าไป, ออกมา), and sentence-final tags (หรือเปล่า,
/// อีกแล้ว). Lemmas equal surfaces throughout (Thai does not inflect; fix_thai_token
/// re-enforces this anyway).
const TH_SEGMENTATION: SegmentationRules = SegmentationRules {
    splits: &[
        (
            "ทำงาน",
            &[
                ("ทำ", PartOfSpeechTag::Verb, "ทำ", PieceAttach::Head),
                (
                    "งาน",
                    PartOfSpeechTag::Noun,
                    "งาน",
                    PieceAttach::ToHead(PieceDep::Obj),
                ),
            ],
        ),
        (
            "มีชีวิต",
            &[
                ("มี", PartOfSpeechTag::Verb, "มี", PieceAttach::Head),
                (
                    "ชีวิต",
                    PartOfSpeechTag::Noun,
                    "ชีวิต",
                    PieceAttach::ToHead(PieceDep::Obj),
                ),
            ],
        ),
        (
            "ได้รับ",
            &[
                (
                    "ได้",
                    PartOfSpeechTag::Aux,
                    "ได้",
                    PieceAttach::ToHead(PieceDep::Aux),
                ),
                ("รับ", PartOfSpeechTag::Verb, "รับ", PieceAttach::Head),
            ],
        ),
        (
            "ได้โปรด",
            &[
                (
                    "ได้",
                    PartOfSpeechTag::Aux,
                    "ได้",
                    PieceAttach::ToHead(PieceDep::Aux),
                ),
                ("โปรด", PartOfSpeechTag::Verb, "โปรด", PieceAttach::Head),
            ],
        ),
        (
            "ควรจะ",
            &[
                ("ควร", PartOfSpeechTag::Aux, "ควร", PieceAttach::Head),
                (
                    "จะ",
                    PartOfSpeechTag::Aux,
                    "จะ",
                    PieceAttach::Sibling(PieceDep::Aux),
                ),
            ],
        ),
        (
            "หมายความว่า",
            &[
                (
                    "หมายความ",
                    PartOfSpeechTag::Verb,
                    "หมายความ",
                    PieceAttach::Head,
                ),
                (
                    "ว่า",
                    PartOfSpeechTag::Sconj,
                    "ว่า",
                    PieceAttach::ToHead(PieceDep::Mark),
                ),
            ],
        ),
        // แบบ + demonstrative, like the คน|นี้ frame; the corpus wrote the นี้/นั้น
        // twins in opposite majority forms, so consistency has to be imposed.
        (
            "แบบนี้",
            &[
                ("แบบ", PartOfSpeechTag::Noun, "แบบ", PieceAttach::Head),
                (
                    "นี้",
                    PartOfSpeechTag::Det,
                    "นี้",
                    PieceAttach::ToHead(PieceDep::Det),
                ),
            ],
        ),
        (
            "แบบนั้น",
            &[
                ("แบบ", PartOfSpeechTag::Noun, "แบบ", PieceAttach::Head),
                (
                    "นั้น",
                    PartOfSpeechTag::Det,
                    "นั้น",
                    PieceAttach::ToHead(PieceDep::Det),
                ),
            ],
        ),
    ],
    merges: &[
        ("พวก", "มัน", PartOfSpeechTag::Pron, "พวกมัน"),
        ("พวก", "นั้น", PartOfSpeechTag::Pron, "พวกนั้น"),
        ("ทุก", "คน", PartOfSpeechTag::Pron, "ทุกคน"),
        ("เข้า", "ไป", PartOfSpeechTag::Verb, "เข้าไป"),
        ("ออก", "มา", PartOfSpeechTag::Verb, "ออกมา"),
        ("หรือ", "เปล่า", PartOfSpeechTag::Part, "หรือเปล่า"),
        ("อีก", "แล้ว", PartOfSpeechTag::Adv, "อีกแล้ว"),
    ],
};

/// The full deterministic pass for Thai.
fn fix_thai_once<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = apply_segmentation(tokens, &TH_SEGMENTATION);
    for token in tokens.iter_mut() {
        fixes.extend(fix_thai_token(token));
    }
    fixes.extend(fix_thai_context(tokens.as_mut_slice()));
    fixes
}

// ---------------------------------------------------------------------------------
// Korean
// ---------------------------------------------------------------------------------

/// Korean canonical segmentation. The prompt's particle rule — split whenever both
/// pieces are visible in the surface — covers every split here: subject pronouns
/// before 가 (내/네/니 are the pre-가 stems of 나/너 and words in their own right),
/// particle + politeness 요, and the 도/에/의 particles after their noun. The merges
/// are true fusions the same policy keeps whole: 누가 (누구+가, no visible 누구) and
/// 어떡해 (어떡하다 conjugated; 어떡 alone is not a word).
const KO_SEGMENTATION: SegmentationRules = SegmentationRules {
    splits: &[
        (
            "내가",
            &[
                ("내", PartOfSpeechTag::Pron, "나", PieceAttach::Head),
                (
                    "가",
                    PartOfSpeechTag::Adp,
                    "가",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "네가",
            &[
                ("네", PartOfSpeechTag::Pron, "너", PieceAttach::Head),
                (
                    "가",
                    PartOfSpeechTag::Adp,
                    "가",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "니가",
            &[
                ("니", PartOfSpeechTag::Pron, "너", PieceAttach::Head),
                (
                    "가",
                    PartOfSpeechTag::Adp,
                    "가",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "은요",
            &[
                ("은", PartOfSpeechTag::Adp, "은", PieceAttach::Head),
                (
                    "요",
                    PartOfSpeechTag::Part,
                    "요",
                    PieceAttach::Sibling(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "는요",
            &[
                ("는", PartOfSpeechTag::Adp, "는", PieceAttach::Head),
                (
                    "요",
                    PartOfSpeechTag::Part,
                    "요",
                    PieceAttach::Sibling(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "도요",
            &[
                ("도", PartOfSpeechTag::Adp, "도", PieceAttach::Head),
                (
                    "요",
                    PartOfSpeechTag::Part,
                    "요",
                    PieceAttach::Sibling(PieceDep::Discourse),
                ),
            ],
        ),
        (
            "아무도",
            &[
                ("아무", PartOfSpeechTag::Pron, "아무", PieceAttach::Head),
                (
                    "도",
                    PartOfSpeechTag::Adp,
                    "도",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "아무것도",
            &[
                ("아무것", PartOfSpeechTag::Pron, "아무것", PieceAttach::Head),
                (
                    "도",
                    PartOfSpeechTag::Adp,
                    "도",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "여기에",
            &[
                ("여기", PartOfSpeechTag::Noun, "여기", PieceAttach::Head),
                (
                    "에",
                    PartOfSpeechTag::Adp,
                    "에",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "자유의",
            &[
                ("자유", PartOfSpeechTag::Noun, "자유", PieceAttach::Head),
                (
                    "의",
                    PartOfSpeechTag::Adp,
                    "의",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "건가요",
            &[
                ("건", PartOfSpeechTag::Noun, "것+은", PieceAttach::Head),
                (
                    "가요",
                    PartOfSpeechTag::Aux,
                    "이다",
                    PieceAttach::ToHead(PieceDep::Cop),
                ),
            ],
        ),
    ],
    merges: &[
        ("누", "가", PartOfSpeechTag::Pron, "누구+가"),
        ("어떡", "해", PartOfSpeechTag::Verb, "어떡하다"),
    ],
};

/// The full deterministic pass for Korean.
fn fix_korean_once<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    apply_segmentation(tokens, &KO_SEGMENTATION)
}

// ---------------------------------------------------------------------------------
// Japanese
// ---------------------------------------------------------------------------------

/// Deterministic per-token lemma/POS fixes, applied identically to every pipeline's
/// tokens. Returns descriptions of what changed.
fn fix_japanese_token(token: &mut impl TokenView) -> Vec<String> {
    let mut fixes = Vec::new();

    // Fix copula lemma → だ. Keyed on the lemma, not the text, so it covers every
    // copula form the analyzer lemmatizes as です (でしょう, でし, でした…) — a
    // text-keyed version left でしょう with lemma です while です itself got だ,
    // splitting one word across two keys.
    if token.pos() == PartOfSpeechTag::Aux && token.lemma() == "です" {
        fixes.push(format!(
            "Fixed copula '{}' lemma from 'です' to 'だ'",
            token.text()
        ));
        token.set_lemma("だ".to_string());
    }

    // Fix i-adjective adverbial form used as lemma (大きく → 大きい)
    // Only fire on ADJ: if the model tagged this as ADJ with lemma == text ending in く,
    // it's almost certainly an i-adjective adverbial form. ADV cases (しばらく, ごく,
    // せっかく, etc.) are correct as-is and handled by classifier hints instead.
    if token.pos() == PartOfSpeechTag::Adj
        && token.text().ends_with("く")
        && token.lemma() == token.text()
        && token.text().chars().count() >= 2
    {
        let stem = &token.text()[..token.text().len() - "く".len()];
        let fixed = format!("{stem}い");
        fixes.push(format!(
            "Fixed i-adjective lemma '{}' to '{}'",
            token.lemma(),
            fixed
        ));
        token.set_lemma(fixed);
    }

    // (Removed: rules that rewrote よい→いい and 達→たち. Both existed to make spelling
    // variants share one lemma, which the analyzer now does natively — it normalizes both
    // よい and いい to 良い, and both 子供達 and 子供たち to 子供達. Re-canonicalizing on top
    // of that does not add grouping, it splits one key into two, which is the exact failure
    // the lemma exists to prevent. The lemma is an identifier, not a display form.)

    // Punctuation lemmas drift between the analyzer's normalization and the mark
    // itself (？ carries lemma "?" and "？" in the same corpus). Pure noise —
    // canonicalize so one mark is one key.
    let punct_lemma = match token.text() {
        "？" => Some("?"),
        "！" => Some("!"),
        "…" => Some("…"),
        _ => None,
    };
    if let Some(canonical) = punct_lemma
        && token.lemma() != canonical
    {
        fixes.push(format!(
            "Fixed punctuation '{}' lemma from '{}' to '{canonical}'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma(canonical.to_string());
    }

    // The analyzer normalizes numerals in lemmas to arabic (一つ→1つ, 三人→3人);
    // the LLM drifts back to kanji. Keep the arabic identifier so 一番/1番 and
    // 一人/1人 each group onto one key. (一番だ also lands here: 一番 is never a
    // na-adjective.)
    let numeral_lemma = match token.lemma() {
        "一番" | "一番だ" => Some("1番"),
        "一人" => Some("1人"),
        _ => None,
    };
    if let Some(canonical) = numeral_lemma
        && token.pos() != PartOfSpeechTag::Propn
    {
        fixes.push(format!(
            "Fixed '{}' lemma from '{}' to '{canonical}'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma(canonical.to_string());
    }

    // The adverb よく (well/often) is the analyzer's 良く — one word whether written
    // よく or 良く. The LLM drifts to 良い (the adjective's lemma) or leaves よく;
    // both split the key three ways.
    if matches!(token.text(), "よく" | "良く")
        && token.pos() == PartOfSpeechTag::Adv
        && token.lemma() != "良く"
    {
        fixes.push(format!(
            "Fixed adverb '{}' lemma from '{}' to '良く'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("良く".to_string());
    }

    // Quotative/topic って is its own word, not a spelling variant of と.
    if token.text() == "って" && token.pos() == PartOfSpeechTag::Adp && token.lemma() != "って"
    {
        fixes.push(format!(
            "Fixed 'って' lemma from '{}' to 'って'",
            token.lemma()
        ));
        token.set_lemma("って".to_string());
    }

    // Sentence adverb 確か ("if I recall") is its own key, distinct from both the
    // na-adjective 確かだ and the adverb 確かに.
    if token.text() == "確か" && token.pos() == PartOfSpeechTag::Adv && token.lemma() != "確か"
    {
        fixes.push(format!(
            "Fixed adverb '確か' lemma from '{}' to '確か'",
            token.lemma()
        ));
        token.set_lemma("確か".to_string());
    }

    // A single token 別に is always the adverb — 別に払う "separately" and
    // 別に急いでない "(not) particularly" alike — never the noun 別 or the
    // na-adjective 別だ (those readings belong to the bare token 別).
    if token.text() == "別に" {
        if token.pos() != PartOfSpeechTag::Adv {
            fixes.push(format!("Fixed '別に' POS from {:?} to ADV", token.pos()));
            token.set_pos(PartOfSpeechTag::Adv);
        }
        if token.lemma() != "別に" {
            fixes.push(format!(
                "Fixed '別に' lemma from '{}' to '別に'",
                token.lemma()
            ));
            token.set_lemma("別に".to_string());
        }
    }

    // (Removed: rules that rewrote なさい→なさる and ください→くださる. They were
    // text-keyed, so くださった kept the analyzer's 下さる while ください became
    // くださる — one word split across two keys. The analyzer already lemmatizes the
    // whole paradigm consistently (下さる, 為さる); the lemma is an identifier, so
    // its spelling only has to be consistent, not kana.)

    // Honorific verbs: never AUX, lemma is the dictionary form.
    let honorific_verbs: &[(&str, &str)] = &[
        ("いらっしゃ", "いらっしゃる"),
        ("おっしゃ", "おっしゃる"),
        ("召し上が", "召し上がる"),
    ];
    for (prefix, dict_form) in honorific_verbs {
        if token.text().starts_with(prefix) {
            if token.lemma() != *dict_form {
                fixes.push(format!(
                    "Fixed '{}' lemma from '{}' to '{}'",
                    token.text(),
                    token.lemma(),
                    dict_form
                ));
                token.set_lemma(dict_form.to_string());
            }
            if token.pos() != PartOfSpeechTag::Verb {
                fixes.push(format!(
                    "Fixed '{}' POS from {:?} to VERB",
                    token.text(),
                    token.pos()
                ));
                token.set_pos(PartOfSpeechTag::Verb);
            }
            break;
        }
    }

    // ございます → ござる
    if (token.text() == "ございます"
        || token.text() == "ございました"
        || token.text() == "ございません")
        && token.lemma() != "ござる"
    {
        fixes.push(format!(
            "Fixed '{}' lemma from '{}' to 'ござる'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("ござる".to_string());
    }

    // そう lemma lockdown
    if token.text() == "そう" && token.pos() == PartOfSpeechTag::Aux && token.lemma() != "そう"
    {
        fixes.push(format!(
            "Fixed 'そう' (AUX) lemma from '{}' to 'そう'",
            token.lemma()
        ));
        token.set_lemma("そう".to_string());
    }

    // Fix capitalized lemmas
    if token.pos() != PartOfSpeechTag::Propn
        && token
            .lemma()
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase() && c.is_ascii())
    {
        let lower = token.lemma().to_lowercase();
        fixes.push(format!("Lowercased lemma '{}' to '{lower}'", token.lemma()));
        token.set_lemma(lower);
    }

    fixes
}

/// The negated copula must never be one token: じゃない fused whole can only carry
/// lemma だ, which erases the negation from the lemma entirely. The copula じゃ and
/// the negation ない are both words, so the boundary is real. Splits a fused
/// じゃない/じゃなかった/じゃなくて back apart, and retags a ない that already
/// stands after the copula as the standalone ADJ 無い.
fn fix_japanese_copula_negation<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let negation = match tokens[i].text() {
            "じゃない" => Some("ない"),
            "じゃなかった" => Some("なかった"),
            "じゃなくて" => Some("なくて"),
            _ => None,
        };
        if let Some(negation) = negation {
            fixes.push(format!(
                "Split '{}' into じゃ (AUX, lemma だ) + {negation} (ADJ, lemma 無い)",
                tokens[i].text()
            ));
            split_token(
                tokens,
                i,
                &[
                    (
                        "じゃ",
                        PartOfSpeechTag::Aux,
                        "だ",
                        PieceAttach::ToHead(PieceDep::Cop),
                    ),
                    (negation, PartOfSpeechTag::Adj, "無い", PieceAttach::Head),
                ],
            );
        } else if i > 0
            && matches!(tokens[i - 1].lemma(), "だ" | "です" | "じゃ")
            && matches!(tokens[i].lemma(), "ない" | "無い")
            && (tokens[i].pos() != PartOfSpeechTag::Adj || tokens[i].lemma() != "無い")
        {
            fixes.push(format!(
                "Retagged '{}' after the copula to ADJ, lemma 無い",
                tokens[i].text()
            ));
            tokens[i].set_pos(PartOfSpeechTag::Adj);
            tokens[i].set_lemma("無い".to_string());
        }
        i += 1;
    }
    fixes
}

/// Japanese canonical segmentation. Every entry enforces a rule the cleaning prompt
/// already states: plural たち/達 splits off its pronoun, noun+する verbs split
/// (お願い|します), でしょう and 大きな/小さな are single tokens whose "pieces"
/// (しょう, 大き) are not words, and そんなに/こんなに/あんなに resolve as DET + に.
/// かも carries a POS-independent reading only as the particle pair か+も (the bird
/// 鴨 would be NOUN, which never reaches this table because the splits key on exact
/// text — 鴨 is written in kanji when it is a token of its own).
const JA_SEGMENTATION: SegmentationRules = SegmentationRules {
    splits: &[
        (
            "私達",
            &[
                ("私", PartOfSpeechTag::Pron, "私", PieceAttach::Head),
                (
                    "達",
                    PartOfSpeechTag::Noun,
                    "達",
                    PieceAttach::ToHead(PieceDep::Compound),
                ),
            ],
        ),
        (
            "私たち",
            &[
                ("私", PartOfSpeechTag::Pron, "私", PieceAttach::Head),
                (
                    "たち",
                    PartOfSpeechTag::Noun,
                    "達",
                    PieceAttach::ToHead(PieceDep::Compound),
                ),
            ],
        ),
        (
            "お願いします",
            &[
                (
                    "お願い",
                    PartOfSpeechTag::Noun,
                    "御願い",
                    PieceAttach::ToHead(PieceDep::CompoundLvc),
                ),
                ("します", PartOfSpeechTag::Verb, "為る", PieceAttach::Head),
            ],
        ),
        (
            "どうやって",
            &[
                ("どう", PartOfSpeechTag::Adv, "どう", PieceAttach::Head),
                (
                    "やって",
                    PartOfSpeechTag::Verb,
                    "遣る",
                    PieceAttach::ToHead(PieceDep::Fixed),
                ),
            ],
        ),
        (
            "そんなに",
            &[
                ("そんな", PartOfSpeechTag::Det, "そんな", PieceAttach::Head),
                (
                    "に",
                    PartOfSpeechTag::Adp,
                    "に",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "こんなに",
            &[
                ("こんな", PartOfSpeechTag::Det, "こんな", PieceAttach::Head),
                (
                    "に",
                    PartOfSpeechTag::Adp,
                    "に",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        (
            "あんなに",
            &[
                ("あんな", PartOfSpeechTag::Det, "あんな", PieceAttach::Head),
                (
                    "に",
                    PartOfSpeechTag::Adp,
                    "に",
                    PieceAttach::ToHead(PieceDep::Case),
                ),
            ],
        ),
        // explicit contrast keeps the negation its own word: で|は|なかった. The は
        // marks what precedes the split, so it attaches backward.
        (
            "はなかった",
            &[
                (
                    "は",
                    PartOfSpeechTag::Adp,
                    "は",
                    PieceAttach::Prev(PieceDep::Case),
                ),
                ("なかった", PartOfSpeechTag::Adj, "無い", PieceAttach::Head),
            ],
        ),
        (
            "かも",
            &[
                ("か", PartOfSpeechTag::Part, "か", PieceAttach::Head),
                (
                    "も",
                    PartOfSpeechTag::Adp,
                    "も",
                    PieceAttach::Sibling(PieceDep::Advmod),
                ),
            ],
        ),
    ],
    merges: &[
        ("で", "しょう", PartOfSpeechTag::Aux, "だ"),
        ("大き", "な", PartOfSpeechTag::Adj, "大きい"),
        ("小さ", "な", PartOfSpeechTag::Adj, "小さい"),
    ],
};

/// Do the tokens from `i` onward continue as a form of なる? Guards the 本当に/確かに
/// merges: 本当+に+なる ("come true" — noun + particle) is a live reading there, and
/// only there, so before なる the LLM's choice stands.
fn ja_next_is_naru<T: TokenView>(tokens: &[T], i: usize) -> bool {
    tokens.get(i).is_some_and(|t| {
        ["なる", "なっ", "なり", "なれ", "なろ"]
            .iter()
            .any(|p| t.text().starts_with(p))
    })
}

/// Multiword proper nouns the teacher labels both ways — merged per the cleaning
/// prompt's "unjoined multiword proper nouns should be one token". These occur
/// almost entirely in the synthetic augmentation corpus (自由の女神 and 万里の長城
/// were written merged/split roughly 50:50 there), where the split pieces are all
/// real words, so only an explicit list can decide.
const JA_MWE_MERGES: &[(&[&str], &str)] = &[
    (&["自由", "の", "女神"], "自由の女神"),
    (&["万里", "の", "長城"], "万里の長城"),
];

fn fix_japanese_segmentation<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = apply_segmentation(tokens, &JA_SEGMENTATION);
    let mut i = 0;
    while i < tokens.len() {
        let matched = JA_MWE_MERGES.iter().find(|(seq, _)| {
            i + seq.len() <= tokens.len()
                && seq.iter().enumerate().all(|(j, piece)| {
                    let tok = &tokens[i + j];
                    let text = if j == 0 {
                        tok.text().trim()
                    } else {
                        tok.text()
                    };
                    text == *piece && (j + 1 == seq.len() || tok.whitespace().is_empty())
                })
        });
        if let Some((seq, whole)) = matched {
            fixes.push(format!("Merged multiword proper noun {whole}"));
            for _ in 1..seq.len() {
                merge_pair(tokens, i, PartOfSpeechTag::Propn, whole);
            }
        }
        i += 1;
    }
    // Lexicalized に-adverbs the prompt lists: 本当に, 確かに — merge unless a なる
    // follows (see ja_next_is_naru). (Conditional ば after an izenkei stem needs no
    // rule here: `absorbs_suffix` already pulls it onto the predicate.)
    let mut i = 0;
    while i < tokens.len() {
        if i + 1 < tokens.len()
            && tokens[i].whitespace().is_empty()
            && tokens[i + 1].text() == "に"
            && matches!(tokens[i].text().trim(), "本当" | "確か")
            && !ja_next_is_naru(tokens, i + 2)
        {
            let lemma = format!("{}に", tokens[i].text().trim());
            fixes.push(format!("Merged '{lemma}' — lexicalized に-adverb"));
            merge_pair(tokens, i, PartOfSpeechTag::Adv, &lemma);
            continue;
        }
        i += 1;
    }
    fixes
}

/// A token is a word. Japanese inflection is agglutinative, so a verb or adjective
/// and the auxiliary chain hanging off it are one word however many morphemes deep
/// it runs — 食べさせられたくなかった is a word the same way 食べた is. Splitting it
/// strands pieces (食べ, まし, だっ, られ) that are not words in any sense and that
/// nobody could be shown on their own. The internal structure is the morpheme
/// layer's job (generate-data's MorphemeCategory::Inflectional), where each piece
/// gets a gloss; that is also where a polysemous piece like られ belongs, since it
/// is only ambiguous in isolation — inside 食べさせられた the reading is forced.
///
/// This mirrors what the Korean prompt already mandates ("conjugated verb forms
/// should stay as one token... don't split the stem from its endings") and what the
/// Korean data does: 91% of its verbs are whole, and the splits that remain are
/// serial verbs (가져|가) whose halves are both real forms — the same category we
/// keep split here as て-form + auxiliary verb.
pub fn merge_japanese_inflection<T: TokenView>(tokens: &mut Vec<T>) {
    let mut i = 0;
    while i + 1 < tokens.len() {
        if absorbs_suffix(&tokens[i], &tokens[i + 1]) {
            // lemma and POS stay the head's: 食べました is a VERB with lemma 食べる
            let pos = tokens[i].pos();
            let lemma = tokens[i].lemma().to_string();
            merge_pair(tokens, i, pos, &lemma);
            // stay: the merged token may absorb the next piece of the chain too
        } else {
            i += 1;
        }
    }
}

/// Auxiliaries that are full verbs (or adjectives) in their own right. `食べて` +
/// `いる` are both showable words, so that boundary stays — it is the same split
/// Korean keeps for serial verbs. なさい is deliberately NOT here: it attaches to a
/// bare stem (見せ|なさい strands 見せ), so 見せなさい merges into one word.
// Each verb is listed in both the kana spelling (what the LLM tends to write) and the
// analyzer's normalized form (what the Sudachi proposal carries) — the carve-out must
// recognize either.
const JAPANESE_AUXILIARY_VERBS: &[&str] = &[
    "いる",
    "居る",
    "ある",
    "有る",
    "くる",
    "来る",
    "いく",
    "行く",
    "しまう",
    "仕舞う",
    "みる",
    "見る",
    "おく",
    "置く",
    "あげる",
    "上げる",
    "くれる",
    "呉れる",
    "もらう",
    "貰う",
    "くださる",
    "下さる",
    "やる",
    "遣る",
    "いただく",
    "頂く",
    "おる",
    "ほしい",
    "欲しい",
];

/// Is this token already a complete, showable form — the dictionary form itself, or a
/// finished past/negative? そう・らしい・よう・みたい after such a head are words of
/// their own (hearsay 降る|そう|だ, 降った|らしい); after a bare stem they are bound
/// (降りそう, 高そう).
fn is_japanese_complete_form(head: &impl TokenView) -> bool {
    head.text() == head.lemma()
        || head.text().ends_with('た')
        || head.text().ends_with('だ')
        || head.text().ends_with("ない")
}

/// Should `next` be absorbed into the preceding token to keep every token a word?
fn absorbs_suffix<T: TokenView>(head: &T, next: &T) -> bool {
    // Anything written apart stays apart — merging across a space would drop it and the
    // tokens would no longer reconstruct the sentence.
    if !head.whitespace().is_empty() {
        return false;
    }
    // Only a predicate has an inflectional tail. A noun keeps the copula separate
    // (学生|です), exactly as Korean keeps 학생|입니다.
    if !matches!(
        head.pos(),
        PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Aux
    ) {
        return false;
    }
    // Inflectional tails that are not words no matter how the labeller tagged them:
    // the て/で of a て-form, conditional ば/たら, listing たり, and stem-attaching
    // ながら/つつ. Pulling them onto the predicate stops the stem being stranded:
    // 食べ|て|いる → 食べて|いる, 食べれ|ば → 食べれば, 食べ|ながら → 食べながら.
    // The copula's て-form (で with lemma だ) is not a tail — 静か|で stays split,
    // like 静か|だ.
    if matches!(
        next.text(),
        "て" | "で" | "ば" | "たら" | "だら" | "たり" | "だり" | "ながら" | "つつ"
    ) && !matches!(next.lemma(), "だ" | "です")
        && matches!(
            next.pos(),
            PartOfSpeechTag::Sconj
                | PartOfSpeechTag::Part
                | PartOfSpeechTag::Adp
                | PartOfSpeechTag::Aux
        )
    {
        return true;
    }
    // そう/らしい/よう/みたい bind to a bare stem but stand alone after a complete
    // form: 降りそう and 実現しそう merge, 降る|そう|だ splits. Checked before the
    // AUX gate because Sudachi tags them 形状詞 (mapped ADJ), not AUX.
    if matches!(
        next.lemma(),
        "そう" | "そうだ" | "らしい" | "よう" | "ようだ" | "みたい" | "みたいだ"
    ) && matches!(next.pos(), PartOfSpeechTag::Aux | PartOfSpeechTag::Adj)
    {
        return !is_japanese_complete_form(head)
            && !head.text().ends_with('て')
            && !head.text().ends_with('で');
    }
    // なさい binds to the bare stem it inflects (し+なさい, 食べ+なさい), and Sudachi
    // tags it 動詞 — the AUX gate below would miss it.
    if next.text() == "なさい"
        && head.pos() == PartOfSpeechTag::Verb
        && !head.text().ends_with('て')
        && !head.text().ends_with('で')
    {
        return true;
    }
    if next.pos() != PartOfSpeechTag::Aux {
        return false;
    }
    // The copula family (だ, です, でした, だった, でしょう, なら, じゃ) is a word: it
    // stays split from nouns and from complete predicates alike — 学生|です,
    // 静か|だった, 美しい|です, 行く|だろう. Two bound exceptions: the voiced past
    // tense of a 撥音便 verb also arrives as だ with lemma だ (読ん+だ, 浮かん+だ) —
    // a verb stem ending in ん is never a complete form, so だ there is inflection,
    // not the copula — and でした continuing the ます chain (食べません+でした).
    if matches!(next.lemma(), "だ" | "です" | "じゃ") {
        return (head.pos() == PartOfSpeechTag::Verb && head.text().ends_with('ん'))
            || head.text().ends_with("ません");
    }
    // ない after the copula is the standalone negation — a word of its own (無い):
    // 学生|じゃ|ない, 静か|で|ない. Folding it in would produce a token whose lemma
    // is だ, erasing the negation from the lemma entirely. Checked before the て/で
    // contraction rule below, which would otherwise swallow ない after the copula で.
    if matches!(head.lemma(), "だ" | "です" | "じゃ") && matches!(next.lemma(), "ない" | "無い")
    {
        return false;
    }
    // After a て-form the auxiliary is a separate word: 食べて|いる, 読んで|しまう.
    // Except てた/てない/てます — contractions of ていた/ていない/ています (乗ってた,
    // 言ってない, 書いてます) — where the piece after て is bound.
    if head.text().ends_with('て') || head.text().ends_with('で') {
        return matches!(next.lemma(), "た" | "ない" | "無い" | "ます");
    }
    if JAPANESE_AUXILIARY_VERBS.contains(&next.lemma()) {
        return false;
    }
    // な on a na-adjective is a closed-class attributive marker attaching to any stem,
    // so it is treated like a particle: 綿密|な.
    if next.text() == "な" {
        return false;
    }
    true
}

/// The full deterministic pass for Japanese.
fn fix_japanese_once<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = Vec::new();
    for token in tokens.iter_mut() {
        fixes.extend(fix_japanese_token(token));
    }
    fixes.extend(fix_japanese_copula_negation(tokens));
    fixes.extend(fix_japanese_segmentation(tokens));
    let before = tokens.len();
    merge_japanese_inflection(tokens);
    if tokens.len() < before {
        fixes.push(format!(
            "Merged inflected predicate pieces: {before} tokens → {}",
            tokens.len()
        ));
    }
    fixes
}

// ---------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------
// Hindi
// ---------------------------------------------------------------------------------

/// Deterministic per-token lemma/POS fixes for Hindi, applied identically to the
/// NLP proposal (`correct`) and the LLM output (`post_corrections`). Returns
/// descriptions of what changed.
fn fix_hindi_token(token: &mut impl TokenView) -> Vec<String> {
    let mut fixes = Vec::new();

    // Lemma hygiene: no stray whitespace (a ',' token once got lemma ", ").
    let trimmed = token.lemma().trim();
    if trimmed != token.lemma() {
        fixes.push(format!("Trimmed whitespace from lemma '{}'", token.lemma()));
        let trimmed = trimmed.to_string();
        token.set_lemma(trimmed);
    }

    // A single-word token must not carry a multiword lemma (पास once got lemma
    // "के पास"). When the token itself is one of the lemma's words, use it.
    if !token.text().contains(' ')
        && token.lemma().contains(' ')
        && token.lemma().split_whitespace().any(|w| w == token.text())
    {
        fixes.push(format!(
            "Collapsed multiword lemma '{}' to '{}'",
            token.lemma(),
            token.text()
        ));
        token.set_lemma(token.text().to_string());
    }

    // Pronoun/possessive lemma normalization to base nominative pronoun.
    // Applies to both PRON and DET (possessives are often tagged DET).
    if token.pos() == PartOfSpeechTag::Pron || token.pos() == PartOfSpeechTag::Det {
        let expected = match token.text() {
            "मुझे" | "मुझको" | "मुझसे" | "मुझमें" | "मेरा" | "मेरी" | "मेरे" => {
                Some("मैं")
            }
            "तुझे" | "तुझको" | "तुझसे" | "तेरा" | "तेरी" | "तेरे" => {
                Some("तू")
            }
            "तुम्हें" | "तुम्हारा" | "तुम्हारी" | "तुम्हारे" => {
                Some("तुम")
            }
            "आपको" | "आपसे" | "आपका" | "आपकी" | "आपके" => {
                Some("आप")
            }
            "उसे" | "उसको" | "उससे" | "उसमें" | "उसका" | "उसकी" | "उसके" | "उसने" => {
                Some("वह")
            }
            "इसे" | "इसको" | "इससे" | "इसमें" | "इसका" | "इसकी" | "इसके" | "इसने" => {
                Some("यह")
            }
            "उन्हें" | "उनसे" | "उनका" | "उनकी" | "उनके" | "उन्होंने" | "वे" => {
                Some("वह")
            }
            "इन्हें" | "इनसे" | "इनका" | "इनकी" | "इनके" | "इन्होंने" | "ये" => {
                Some("यह")
            }
            "हमें" | "हमसे" | "हमारा" | "हमारी" | "हमारे" | "हमने" => {
                Some("हम")
            }
            _ => None,
        };

        if let Some(expected) = expected
            && token.lemma() != expected
        {
            fixes.push(format!(
                "Fixed pronoun/possessive '{}' lemma from '{}' to '{}'",
                token.text(),
                token.lemma(),
                expected
            ));
            token.set_lemma(expected.to_string());
        }
    }

    // Possessives are DET regardless of syntactic position.
    let possessive_forms = [
        "मेरा",
        "मेरी",
        "मेरे",
        "तेरा",
        "तेरी",
        "तेरे",
        "तुम्हारा",
        "तुम्हारी",
        "तुम्हारे",
        "आपका",
        "आपकी",
        "आपके",
        "उसका",
        "उसकी",
        "उसके",
        "इसका",
        "इसकी",
        "इसके",
        "उनका",
        "उनकी",
        "उनके",
        "इनका",
        "इनकी",
        "इनके",
        "हमारा",
        "हमारी",
        "हमारे",
    ];
    if possessive_forms.contains(&token.text()) && token.pos() == PartOfSpeechTag::Pron {
        fixes.push(format!(
            "Fixed possessive '{}' POS from PRON to DET",
            token.text()
        ));
        token.set_pos(PartOfSpeechTag::Det);
    }

    // चाहिए is its own lemma (dictionaries list it as its own entry, and it
    // means "is needed/should", not चाहना "to want"). Its POS is contextual and
    // set in fix_hindi_context.
    if token.text() == "चाहिए" && token.lemma() != "चाहिए" {
        fixes.push(format!(
            "Fixed 'चाहिए' lemma from '{}' to 'चाहिए'",
            token.lemma()
        ));
        token.set_lemma("चाहिए".to_string());
    }

    // लाइए is the honorific imperative of लाना (to bring), not लेना (to take).
    // This regresses repeatedly so it needs a deterministic fix.
    if token.text() == "लाइए" && token.lemma() == "लेना" {
        fixes.push("Fixed 'लाइए' lemma from 'लेना' to 'लाना'".to_string());
        token.set_lemma("लाना".to_string());
    }

    // जनता (NOUN "the public") vs जानता (VERB "knows", from जानना).
    if token.text() == "जनता" && token.pos() == PartOfSpeechTag::Verb {
        fixes.push(
            "Fixed 'जनता' from VERB to NOUN — the verb form is 'जानता' (lemma जानना), the noun is 'जनता' (the public)"
                .to_string(),
        );
        token.set_pos(PartOfSpeechTag::Noun);
        token.set_lemma("जनता".to_string());
    }

    // किसी… → कोई (oblique → base indefinite pronoun).
    if matches!(token.text(), "किसी" | "किसीने" | "किसीको" | "किसीसे") && token.lemma() != "कोई"
    {
        fixes.push(format!(
            "Fixed '{}' lemma from '{}' to 'कोई' (oblique → base form)",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("कोई".to_string());
    }

    // किस… → कौन (oblique of कौन, not क्या).
    if matches!(token.text(), "किस" | "किसे" | "किसने" | "किसको" | "किससे") && token.lemma() != "कौन"
    {
        fixes.push(format!(
            "Fixed '{}' lemma from '{}' to 'कौन' (किस is oblique of कौन, not क्या)",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("कौन".to_string());
    }

    // और is CCONJ ("and") or ADV ("more"), never ADJ.
    if token.text() == "और" && token.pos() == PartOfSpeechTag::Adj {
        fixes.push("Fixed 'और' POS from ADJ to CCONJ".to_string());
        token.set_pos(PartOfSpeechTag::Cconj);
    }

    // Simple postpositions lemmatize to themselves (के once got lemma का).
    if matches!(
        token.text(),
        "में" | "पर" | "को" | "से" | "के" | "का" | "की" | "ने" | "तक" | "द्वारा"
    ) && token.pos() == PartOfSpeechTag::Adp
        && token.lemma() != token.text()
    {
        fixes.push(format!(
            "Fixed postposition '{}' lemma from '{}' to itself",
            token.text(),
            token.lemma()
        ));
        token.set_lemma(token.text().to_string());
    }

    // पहले: as ADV ("earlier/first") it lemmatizes to पहला; as ADP
    // (compound postposition के पहले) it is its own lemma.
    if token.text() == "पहले" {
        if token.pos() == PartOfSpeechTag::Adv && token.lemma() != "पहला" {
            fixes.push(format!(
                "Fixed 'पहले' ADV lemma from '{}' to 'पहला'",
                token.lemma()
            ));
            token.set_lemma("पहला".to_string());
        }
        if token.pos() == PartOfSpeechTag::Adp && token.lemma() != "पहले" {
            fixes.push(format!(
                "Fixed 'पहले' ADP lemma from '{}' to 'पहले'",
                token.lemma()
            ));
            token.set_lemma("पहले".to_string());
        }
    }

    // कैसे as ADV ("how") is its own dictionary headword, not a form of कैसा.
    if token.text() == "कैसे" && token.pos() == PartOfSpeechTag::Adv && token.lemma() != "कैसे"
    {
        fixes.push(format!(
            "Fixed 'कैसे' ADV lemma from '{}' to 'कैसे'",
            token.lemma()
        ));
        token.set_lemma("कैसे".to_string());
    }

    // रहा/रही/रहे/रहीं are forms of रहना only — never करना or होना.
    if matches!(token.text(), "रहा" | "रही" | "रहे" | "रहीं")
        && matches!(token.lemma(), "करना" | "होना")
    {
        fixes.push(format!(
            "Fixed '{}' lemma from '{}' to 'रहना'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("रहना".to_string());
    }

    // Negation नहीं/न/मत → ADV.
    if matches!(token.text(), "नहीं" | "न" | "मत") && token.pos() != PartOfSpeechTag::Adv
    {
        fixes.push(format!(
            "Fixed '{}' POS from {:?} to ADV",
            token.text(),
            token.pos()
        ));
        token.set_pos(PartOfSpeechTag::Adv);
    }

    // Reflexive possessive अपना family: lemma अपना.
    if matches!(token.text(), "अपना" | "अपने" | "अपनी" | "अपनों") && token.lemma() != "अपना"
    {
        fixes.push(format!(
            "Fixed reflexive possessive '{}' lemma from '{}' to 'अपना'",
            token.text(),
            token.lemma()
        ));
        token.set_lemma("अपना".to_string());
    }

    // वह/यह tagged CCONJ → PRON.
    if matches!(token.text(), "वह" | "यह") && token.pos() == PartOfSpeechTag::Cconj {
        fixes.push(format!("Fixed '{}' POS from CCONJ to PRON", token.text()));
        token.set_pos(PartOfSpeechTag::Pron);
    }

    // Focus particles ही/भी → PART.
    if matches!(token.text(), "ही" | "भी") && token.pos() != PartOfSpeechTag::Part {
        fixes.push(format!(
            "Fixed '{}' POS from {:?} to Part",
            token.text(),
            token.pos()
        ));
        token.set_pos(PartOfSpeechTag::Part);
    }

    // Lowercase non-PROPN lemmas (only affects stray Latin text).
    if token.pos() != PartOfSpeechTag::Propn
        && token
            .lemma()
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase())
    {
        let lower = token.lemma().to_lowercase();
        fixes.push(format!(
            "Lowercased lemma '{}' to '{}'",
            token.lemma(),
            lower
        ));
        token.set_lemma(lower);
    }

    fixes
}

/// Tokens skipped when looking for the verbal host of a Hindi auxiliary:
/// focus particles, negation, and displaced interrogatives can all intervene
/// (करना ही नहीं चाहिए; तुम्हें हो क्या गया था?; कह कैसे सकती हो?).
pub fn is_hindi_aux_intervener(text: &str) -> bool {
    matches!(text, "ही" | "भी" | "तो" | "नहीं" | "न" | "मत" | "क्या" | "कैसे")
}

/// Deterministic context-dependent Hindi fixes (these need neighbor access),
/// applied identically in `correct` and `post_corrections`.
fn fix_hindi_context<T: TokenView>(tokens: &mut [T]) -> Vec<String> {
    let mut fixes = Vec::new();

    // "X जाने" where X ∈ {ईश्वर, खुदा, भगवान, कौन, अल्लाह} — जाने = subjunctive
    // of जानना (to know), not जाना (to go). Fixed idioms meaning "God/who knows."
    for i in 0..tokens.len().saturating_sub(1) {
        if matches!(tokens[i].text(), "ईश्वर" | "खुदा" | "भगवान" | "कौन" | "अल्लाह")
            && tokens[i + 1].text() == "जाने"
            && tokens[i + 1].lemma() == "जाना"
        {
            fixes.push(format!(
                "Fixed 'जाने' after '{}' — lemma 'जाना' (to go) → 'जानना' (to know). '{} जाने' = '{} knows'",
                tokens[i].text(),
                tokens[i].text(),
                tokens[i].text()
            ));
            tokens[i + 1].set_lemma("जानना".to_string());
            tokens[i + 1].set_pos(PartOfSpeechTag::Verb);
        }
    }

    // चाहिए POS is decided by what it follows. After an infinitive (-ना/-नी/-ने
    // verb form, possibly with particles/negation in between) it is the deontic
    // "should" → AUX (जाना चाहिए). Anywhere else it is the main predicate of
    // need, "X को Y चाहिए" → VERB (मुझे चाय चाहिए).
    for i in 0..tokens.len() {
        if tokens[i].text() != "चाहिए" {
            continue;
        }
        let mut host_idx = None;
        let mut j = i;
        while j > 0 {
            j -= 1;
            if is_hindi_aux_intervener(tokens[j].text()) {
                continue;
            }
            host_idx = Some(j);
            break;
        }
        let deontic = host_idx.is_some_and(|j| {
            matches!(
                tokens[j].pos(),
                PartOfSpeechTag::Verb | PartOfSpeechTag::Aux
            ) && (tokens[j].text().ends_with("ना")
                || tokens[j].text().ends_with("नी")
                || tokens[j].text().ends_with("ने"))
        });
        let want = if deontic {
            PartOfSpeechTag::Aux
        } else {
            PartOfSpeechTag::Verb
        };
        if tokens[i].pos() != want {
            fixes.push(format!(
                "Fixed 'चाहिए' POS from {:?} to {:?} — AUX after an infinitive (deontic 'should'), VERB otherwise (need: 'X को Y चाहिए')",
                tokens[i].pos(),
                want
            ));
            tokens[i].set_pos(want);
        }
    }

    // ठीक/अच्छा before लगना is a predicative complement ("seems fine", "feels
    // good") → ADJ, not ADV. Manner uses before other verbs stay ADV
    // (अच्छा खेलता है, ठीक कहा).
    for i in 0..tokens.len() {
        if !matches!(tokens[i].text(), "ठीक" | "अच्छा" | "अच्छी" | "अच्छे")
            || tokens[i].pos() != PartOfSpeechTag::Adv
        {
            continue;
        }
        let mut j = i + 1;
        while j < tokens.len() && is_hindi_aux_intervener(tokens[j].text()) {
            j += 1;
        }
        if j < tokens.len() && tokens[j].lemma() == "लगना" {
            fixes.push(format!(
                "Fixed '{}' POS from ADV to ADJ — predicative complement of लगना",
                tokens[i].text()
            ));
            tokens[i].set_pos(PartOfSpeechTag::Adj);
        }
    }

    // ADJ+करना conjunct verbs: after these unambiguous conjunct hosts, करना is
    // the verbalizer → AUX (खत्म कर दें, चुप करो, दूर कर देगी). Interrogatives
    // like कैसा कर रहा है are real main-verb uses and are not in this list.
    const CONJUNCT_ADJ_HOSTS: &[&str] = &[
        "खत्म",
        "ख़त्म",
        "दूर",
        "चुप",
        "बंद",
        "साफ",
        "साफ़",
        "शुरू",
        "शुरु",
        "पूरा",
        "पूरी",
        "पूरे",
        "माफ",
        "माफ़",
        "ठीक",
        "तैयार",
        "ख़ुश",
        "खुश",
        "अलग",
        "समाप्त",
        "कम",
        "तर",
    ];
    for i in 1..tokens.len() {
        if tokens[i].lemma() == "करना"
            && tokens[i].pos() == PartOfSpeechTag::Verb
            && CONJUNCT_ADJ_HOSTS.contains(&tokens[i - 1].text())
        {
            fixes.push(format!(
                "Fixed '{}' POS from VERB to AUX — verbalizer in the conjunct verb '{} {}'",
                tokens[i].text(),
                tokens[i - 1].text(),
                tokens[i].text()
            ));
            tokens[i].set_pos(PartOfSpeechTag::Aux);
        }
    }

    fixes
}

/// The full deterministic pass for Hindi. The context pass runs twice because its
/// rules can be enabled by per-token normalizations (चहिए → चाहिए).
fn fix_hindi_once<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    let mut fixes = fix_hindi_context(tokens.as_mut_slice());
    for token in tokens.iter_mut() {
        fixes.extend(fix_hindi_token(token));
    }
    fixes.extend(fix_hindi_context(tokens.as_mut_slice()));
    fixes
}

/// Hindi spelling normalizations that REWRITE TOKEN TEXT (लिये → लिए, the
/// misspelling चहिए → चाहिए). Correct for the LLM-cleaning pipeline, whose
/// downstream reconstructs the sentence from the tokens — but never for the silver
/// tokenization store, where each entry's tokens must reconstruct the source
/// sentence it is keyed by. Hence not part of `fix_hindi`; clean-nlp-data's
/// corrector applies it separately.
pub fn normalize_hindi_spelling(token: &mut impl TokenView) -> Vec<String> {
    let mut fixes = Vec::new();
    if token.text() == "लिये" {
        fixes.push("Normalized 'लिये' to 'लिए'".to_string());
        token.set_text("लिए".to_string());
        token.set_lemma("लिए".to_string());
    }
    if token.text() == "चहिए" {
        fixes.push("Normalized 'चहिए' to 'चाहिए'".to_string());
        token.set_text("चाहिए".to_string());
    }
    fixes
}

/// Run one language pass to a fixpoint. A fix can enable another fix that already
/// ran earlier in the same pass — e.g. a Japanese inflection merge completes a stem
/// that the い-adjective lemma rule keys on — so a single pass is not always enough.
/// Convergence takes two passes in practice; the cap only guards against a future
/// rule that reports fixes without converging.
fn fixpoint<T>(
    tokens: &mut Vec<T>,
    mut pass: impl FnMut(&mut Vec<T>) -> Vec<String>,
) -> Vec<String> {
    let mut all = Vec::new();
    for _ in 0..4 {
        let fixes = pass(tokens);
        if fixes.is_empty() {
            break;
        }
        all.extend(fixes);
    }
    all
}

/// The full deterministic pass for Chinese, run to a fixpoint.
pub fn fix_chinese<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    fixpoint(tokens, fix_chinese_once)
}

/// The full deterministic pass for Thai, run to a fixpoint.
pub fn fix_thai<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    fixpoint(tokens, fix_thai_once)
}

/// The full deterministic pass for Korean, run to a fixpoint.
pub fn fix_korean<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    fixpoint(tokens, fix_korean_once)
}

/// The full deterministic pass for Japanese, run to a fixpoint.
pub fn fix_japanese<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    fixpoint(tokens, fix_japanese_once)
}

/// The full deterministic pass for Hindi, run to a fixpoint.
pub fn fix_hindi<T: TokenView + Clone>(tokens: &mut Vec<T>) -> Vec<String> {
    fixpoint(tokens, fix_hindi_once)
}

/// The full deterministic correction pass for a language's tokens. Languages without
/// deterministic rules pass through unchanged. Applied at LOAD time to the silver
/// tokenization stores (`nlp::load_canonicalized`) — the stores keep the model's raw
/// output so these rules stay revisable — and at generation time in clean-nlp-data's
/// correctors. Idempotent: applying it to already-corrected tokens changes nothing.
/// The French definite and indefinite articles, including the elided form.
/// Only these: the rule below relabels by syntax, and `de`/`du`/`des` reach a
/// `det` slot as contractions with their own analysis, so they stay out.
const FRENCH_ARTICLES: &[&str] = &["le", "la", "les", "l'", "un", "une"];

/// French: an article the tagger called a pronoun while attaching it as a
/// determiner.
///
/// The teacher model routinely emits `les/le/PRON` with `dep=det` — 3.1k tokens
/// of a French corpus, 1k of them `les` — and the gold cleaner reproduces the
/// same error, so neither dataset can be trusted to settle it. The `det`
/// relation does settle it: a word attached as a determiner is a determiner,
/// whatever the POS field says, so this is a self-inconsistency to repair
/// rather than a judgment call about context. That distinction is why the rule
/// belongs here at all — `les` really is ambiguous between article and object
/// clitic ("les livres" vs "je les vois"), and a surface-keyed table entry
/// would be exactly the context-blind fix this module warns against. Keying on
/// the parse instead means the ambiguous cases never reach the rule.
///
/// Left uncorrected: `tout`/`tous`/`ce` (another ~3.2k `det`-attached PRONs)
/// and the `un peu`/`beaucoup` ADVs, which need their own audit.
///
/// Only affects token types that carry a dependency — [`TokenView::dep_label`]
/// is `None` for spaCy-side proposals, so this is a no-op there.
pub fn fix_french<T: TokenView + Clone>(tokens: &mut [T]) -> Vec<String> {
    let mut notes = Vec::new();
    for token in tokens.iter_mut() {
        if token.pos() != PartOfSpeechTag::Pron || token.dep_label() != Some(PieceDep::Det) {
            continue;
        }
        let surface = token
            .text()
            .to_lowercase()
            .replace(['\u{2019}', '\u{02BC}'], "'");
        if !FRENCH_ARTICLES.contains(&surface.as_str()) {
            continue;
        }
        token.set_pos(PartOfSpeechTag::Det);
        notes.push(format!(
            "article {:?} attached as det: PRON -> DET",
            token.text()
        ));
    }
    notes
}

pub fn fix_tokens<T: TokenView + Clone>(language: Language, tokens: &mut Vec<T>) -> Vec<String> {
    match language {
        Language::ChineseSimplified | Language::ChineseTraditional => fix_chinese(tokens),
        Language::Japanese => fix_japanese(tokens),
        Language::Hindi => fix_hindi(tokens),
        Language::Korean => fix_korean(tokens),
        Language::Thai => fix_thai(tokens),
        Language::French => fix_french(tokens),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PartOfSpeechTag::*;

    /// A lexide token with dep/head, for exercising the head bookkeeping.
    fn ltok(
        text: &str,
        pos: PartOfSpeechTag,
        dep: lexide::DependencyRelation,
        head: i32,
    ) -> lexide::Token {
        lexide::Token {
            text: lexide::Text {
                text: text.to_string(),
            },
            whitespace: String::new(),
            pos: tag_to_lexide_pos(pos),
            lemma: lexide::Lemma {
                lemma: text.to_string(),
            },
            dep,
            head,
        }
    }

    fn texts(tokens: &[lexide::Token]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.text.as_str()).collect()
    }

    fn reconstruct(tokens: &[lexide::Token]) -> String {
        tokens
            .iter()
            .map(|t| format!("{}{}", t.text.text, t.whitespace))
            .collect()
    }

    #[test]
    fn chinese_split_renumbers_heads() {
        use lexide::DependencyRelation as D;
        // 你(nsubj→要) 不要(root) 走(xcomp→要)
        let mut tokens = vec![
            ltok("你", Pron, D::Nsubj, 2),
            ltok("不要", Verb, D::Root, 0),
            ltok("走", Verb, D::Xcomp, 2),
        ];
        let before = reconstruct(&tokens);
        fix_chinese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["你", "不", "要", "走"]);
        assert_eq!(reconstruct(&tokens), before);
        // 要 is the head piece at position 3: 你 and 走 follow it, 不 hangs off it
        assert_eq!(tokens[0].head, 3);
        assert_eq!((tokens[1].dep, tokens[1].head), (D::Advmod, 3));
        assert_eq!((tokens[2].dep, tokens[2].head), (D::Root, 0));
        assert_eq!(tokens[3].head, 3);
    }

    #[test]
    fn chinese_merge_takes_external_attachment() {
        use lexide::DependencyRelation as D;
        // 我(nsubj→有) 没(advmod→有) 有(root) 钱(obj→有): merging 没+有 must keep
        // 有's root attachment even though 没 (the left piece) hung off 有.
        let mut tokens = vec![
            ltok("我", Pron, D::Nsubj, 3),
            ltok("没", Adv, D::Advmod, 3),
            ltok("有", Verb, D::Root, 0),
            ltok("钱", Noun, D::Obj, 3),
        ];
        fix_chinese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["我", "没有", "钱"]);
        assert_eq!((tokens[1].dep, tokens[1].head), (D::Root, 0));
        assert_eq!(tokens[0].head, 2);
        assert_eq!(tokens[2].head, 2);
    }

    #[test]
    fn korean_sibling_attachment() {
        use lexide::DependencyRelation as D;
        // 이름(root) 은요(case→이름) ?(punct→이름)
        let mut tokens = vec![
            ltok("이름", Noun, D::Root, 0),
            ltok("은요", Adp, D::Case, 1),
            ltok("?", Punct, D::Punct, 1),
        ];
        fix_korean(&mut tokens);
        assert_eq!(texts(&tokens), vec!["이름", "은", "요", "?"]);
        // 은 keeps the case attachment to 이름; 요 attaches to the same external
        // head with its own discourse label
        assert_eq!((tokens[1].dep, tokens[1].head), (D::Case, 1));
        assert_eq!((tokens[2].dep, tokens[2].head), (D::Discourse, 1));
        assert_eq!(tokens[3].head, 1);
    }

    #[test]
    fn japanese_prev_attachment_and_inflection_merge() {
        use lexide::DependencyRelation as D;
        // わけ(nsubj-ish) で(cop) はなかった(root): the は from the split must attach
        // backward to で's nominal side, and なかった keeps root.
        let mut tokens = vec![
            ltok("わけ", Noun, D::Nsubj, 3),
            ltok("で", Aux, D::Cop, 3),
            ltok("はなかった", Adj, D::Root, 0),
        ];
        fix_japanese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["わけ", "で", "は", "なかった"]);
        assert_eq!((tokens[2].dep, tokens[2].head), (D::Case, 2));
        assert_eq!((tokens[3].dep, tokens[3].head), (D::Root, 0));
        assert_eq!(tokens[0].head, 4);

        // すれ(root) ば(mark→すれ): the conditional rejoins via absorbs_suffix,
        // keeping the stem's attachment.
        let mut tokens = vec![ltok("すれ", Verb, D::Root, 0), {
            let mut t = ltok("ば", Sconj, D::Mark, 1);
            t.lemma.lemma = "ば".to_string();
            t
        }];
        tokens[0].lemma.lemma = "為る".to_string();
        fix_japanese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["すれば"]);
        assert_eq!(tokens[0].lemma.lemma, "為る");
        assert_eq!((tokens[0].dep, tokens[0].head), (D::Root, 0));
    }

    #[test]
    fn japanese_ni_adverb_guard() {
        use lexide::DependencyRelation as D;
        // 本当|に merges as the lexicalized adverb…
        let mut tokens = vec![
            ltok("本当", Adv, D::Advmod, 3),
            ltok("に", Adp, D::Case, 1),
            ltok("好き", Adj, D::Root, 0),
        ];
        fix_japanese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["本当に", "好き"]);
        assert_eq!(tokens[0].lemma.lemma, "本当に");
        assert_eq!((tokens[0].dep, tokens[0].head), (D::Advmod, 2));

        // …but not before なる, where 本当+に+なる is a live reading
        let mut tokens = vec![
            ltok("本当", Noun, D::Obl, 3),
            ltok("に", Adp, D::Case, 1),
            ltok("なった", Verb, D::Root, 0),
        ];
        fix_japanese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["本当", "に", "なった"]);
    }

    #[test]
    fn japanese_mwe_merge() {
        use lexide::DependencyRelation as D;
        // 自由(nmod→女神) の(case→自由) 女神(nsubj→ext) は(case→女神) …
        let mut tokens = vec![
            ltok("自由", Noun, D::Nmod, 3),
            ltok("の", Adp, D::Case, 1),
            ltok("女神", Noun, D::Nsubj, 5),
            ltok("は", Adp, D::Case, 3),
            ltok("ある", Verb, D::Root, 0),
        ];
        fix_japanese(&mut tokens);
        assert_eq!(texts(&tokens), vec!["自由の女神", "は", "ある"]);
        // the merged PROPN keeps the head noun's external attachment
        assert_eq!(tokens[0].pos, tag_to_lexide_pos(Propn));
        assert_eq!(tokens[0].lemma.lemma, "自由の女神");
        assert_eq!((tokens[0].dep, tokens[0].head), (D::Nsubj, 3));
        assert_eq!(tokens[1].head, 1);
        assert_eq!(tokens[2].head, 0);
    }

    #[test]
    fn thai_split_and_merge() {
        use lexide::DependencyRelation as D;
        let mut tokens = vec![ltok("ควรจะ", Aux, D::Aux, 2), ltok("ไป", Verb, D::Root, 0)];
        fix_thai(&mut tokens);
        assert_eq!(texts(&tokens), vec!["ควร", "จะ", "ไป"]);
        // both markers attach to the external verb (now at position 3)
        assert_eq!((tokens[0].dep, tokens[0].head), (D::Aux, 3));
        assert_eq!((tokens[1].dep, tokens[1].head), (D::Aux, 3));

        // no merge across a phrase-boundary space
        let mut tokens = vec![
            {
                let mut t = ltok("พวก", Noun, D::Nsubj, 0);
                t.whitespace = " ".to_string();
                t
            },
            ltok("มัน", Pron, D::Nsubj, 0),
        ];
        fix_thai(&mut tokens);
        assert_eq!(texts(&tokens), vec!["พวก", "มัน"]);
    }

    #[test]
    fn fix_tokens_dispatch_and_idempotence() {
        use lexide::DependencyRelation as D;
        let mut tokens = vec![
            ltok("你", Pron, D::Nsubj, 2),
            ltok("有没有", Verb, D::Root, 0),
            ltok("水", Noun, D::Obj, 2),
        ];
        let fixed = fix_tokens(Language::ChineseSimplified, &mut tokens);
        assert!(!fixed.is_empty());
        assert_eq!(texts(&tokens), vec!["你", "有", "没有", "水"]);
        let again = fix_tokens(Language::ChineseSimplified, &mut tokens);
        assert!(again.is_empty(), "second pass changed tokens: {again:?}");
    }

    #[test]
    fn french_article_under_det_becomes_det() {
        use lexide::DependencyRelation as D;
        // "À tous les deux." — the tagger's own `det` attachment contradicts
        // the PRON tag it gave the article.
        let mut toks = vec![
            ltok("tous", Pron, D::Det, 4),
            ltok("les", Pron, D::Det, 4),
            ltok("deux", Num, D::Nmod, 1),
        ];
        let notes = fix_french(&mut toks);
        assert_eq!(toks[1].pos, tag_to_lexide_pos(Det));
        assert_eq!(notes.len(), 1, "only the article is in scope, not `tous`");
        // Idempotent: a second pass has nothing left to say.
        assert!(fix_french(&mut toks).is_empty());
    }

    #[test]
    fn french_object_clitic_les_is_left_alone() {
        use lexide::DependencyRelation as D;
        // "Nous allons les encercler." — same surface, object relation, so the
        // PRON tag is right and the rule must not touch it.
        let mut toks = vec![ltok("les", Pron, D::Obj, 3)];
        assert!(fix_french(&mut toks).is_empty());
        assert_eq!(toks[0].pos, tag_to_lexide_pos(Pron));
        // A dep-less token type reports no label, so the rule can't fire.
        let mut doc = vec![language_utils::DocToken {
            text: "les".to_string(),
            whitespace: " ".to_string(),
            pos: Pron,
            lemma: "le".to_string(),
            morph: Default::default(),
        }];
        assert!(fix_french(&mut doc).is_empty());
    }
}
