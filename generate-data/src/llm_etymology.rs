//! LLM-based morpheme splits, plus loading of the hand-curated
//! `golden_morphemes.jsonl` seed file.
//!
//! Each morpheme is carried as a `MorphemeSegment { surface, canonical }`.
//! The canonical is the dictionary / lemma form (see `llm_etymology.rs`
//! prompt); the surface is the literal substring of the word.
//!
//! tysm handles caching per-call, so we don't persist anything ourselves.

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Language, MorphemeSegment};
use sentence_sampler::sample_to_target;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

use crate::etymology::AlignedEntry;

static CHAT_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    crate::apply_cache_only(
        ChatClient::from_env("gpt-5.4-mini")
            .unwrap()
            .with_cache_directory("./.cache")
            .with_reasoning_effort("low")
            .with_service_tier("flex"),
    )
});

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct EtymologyResponse {
    /// Brief reasoning about the etymology / productive morphemes of the word.
    #[serde(rename = "1. thoughts")]
    thoughts: String,
    /// One entry per morpheme, in left-to-right order, each formatted as
    /// `"surface (canonical)"`. Concatenating every `surface` must exactly
    /// reproduce the input word. If the word is a simple monomorphemic root,
    /// return a single entry with surface == canonical == the whole word.
    #[serde(rename = "2. morphemes")]
    morphemes: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenLine {
    word: String,
    morphemes: Vec<String>,
}

/// Render a word as space-separated characters, e.g. `destabilize` -> `d e s t a b i l i z e`.
fn spell_out(word: &str) -> String {
    let mut out = String::new();
    for (i, ch) in word.chars().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Parse `"surface (canonical)"` or `"surface (canonical) [tag]"` into a
/// `MorphemeSegment`. The trailing `[tag]` is optional and used to
/// disambiguate grammatical morphemes with the same (surface, canonical) but
/// different functions (e.g. French `-s` as plural vs 1sg verb ending).
fn parse_morpheme_pair(s: &str) -> Option<MorphemeSegment<String>> {
    let s = s.trim();

    // Extract optional trailing `[tag]`.
    let (main, tag) = if s.ends_with(']')
        && let Some(open) = s.rfind(" [")
    {
        let tag_str = s[open + 2..s.len() - 1].trim();
        if tag_str.is_empty() {
            (s, None)
        } else {
            (&s[..open], Some(tag_str.to_string()))
        }
    } else {
        (s, None)
    };

    if !main.ends_with(')') {
        return None;
    }
    let open = main.rfind(" (")?;
    let surface = main[..open].to_string();
    let canonical = main[open + 2..main.len() - 1].to_string();
    if surface.is_empty() || canonical.is_empty() {
        return None;
    }
    Some(MorphemeSegment {
        surface,
        canonical,
        tag,
    })
}

/// Load `golden_morphemes.jsonl` (one JSON object per line matching
/// [`GoldenLine`]). Lines that fail to parse or whose surfaces don't
/// concatenate back to the word are silently skipped.
pub fn load_golden_morphemes(path: &Path) -> Vec<AlignedEntry> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);

    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let parsed: GoldenLine = serde_json::from_str(trimmed).ok()?;
            if parsed.morphemes.len() < 2 {
                return None;
            }
            let segments: Vec<MorphemeSegment<String>> = parsed
                .morphemes
                .iter()
                .map(|s| parse_morpheme_pair(s))
                .collect::<Option<Vec<_>>>()?;
            if segments
                .iter()
                .map(|s| s.surface.as_str())
                .collect::<String>()
                != parsed.word
            {
                return None;
            }
            Some(AlignedEntry {
                word: parsed.word,
                segments,
            })
        })
        .collect()
}

/// Language-specific notes for the etymology prompt. Covers both splitting
/// guidance (which morphemes to break off) and tag guidance (how to
/// disambiguate same-surface morphemes with different grammatical functions).
/// Exhaustive match so the compiler flags any `Language` variant that doesn't
/// have coverage.
fn language_specific_notes(language: Language) -> &'static str {
    match language {
        Language::Korean => {
            "\n\nExtra Korean-specific guidance:
- `요` is a separable politeness particle, not part of `아요`/`어요`/`지요`/`네요`/etc. Always split `요` off by itself when it appears at the end of a polite form. Example: `먹어요 → 먹 + 어 + 요` (not `먹 + 어요`), `먹었어요 → 먹 + 었 + 어 + 요`, `가지요 → 가 + 지 + 요`.
- Past-tense `-았/-었` is its own morpheme, separate from the politeness particle and any following vowel. `먹었어요 → 먹 + 었 + 어 + 요`.
- Dictionary-form infinitive `-다` is its own morpheme: `먹다 → 먹 + 다`, `좋다 → 좋 + 다`.
- Particles (조사) always detach from their noun: `학생은 → 학생 + 은`, `친구와 → 친구 + 와`, `학교에서 → 학교 + 에서`. Common particles: `은/는, 이/가, 을/를, 에, 에서, 와/과, 의, 도, 만, 까지, 부터`.
- Honorific `-님`, plural `-들`, verbalizer `-하다`, `-화`, `-적`, `-자`, `-성` are all separable productive suffixes.
- Keep the literal-concatenation rule above in mind: if splitting a productive morpheme would require vowel fusion / jamo attachment / irregular stem change (e.g. `해요`, `갔어요`, `예쁜`, `도와요`), leave the word as a single morpheme rather than forcing a broken split.
- Tag disambiguation for genuinely ambiguous endings/particles:
  - `-아`/`-어`: `[conn]` (connective) vs `[imp]` (imperative)
  - `-은`/`-는`: `[top]` topic marker on nouns (학생은) vs `[attr]` adjective-attributive (좋은) vs `[past.attr]` past-attributive on verb stems (먹은)
  - `-(으)로`: `[inst]` (연필로 with a pencil) vs `[dir]` (학교로 to school) vs `[rslt]` (의사로 되다 become)
  - `-에`: `[loc]` (집에 있다) vs `[dir]` (집에 간다) vs `[tmp]` (3시에 at 3)
  - `-게`: `[adv]` adverbializer (빠르게) vs `[caus]` causative complementizer (-게 하다)
  - `-겠-`: `[fut]` (내일 가겠어요) vs `[conj]` conjectural (비가 오겠어요) vs `[vol]` volition (제가 하겠습니다)
  - `-지`: `[conn]` negative connective (가지 않다) vs `[sfp]` sentence-final seeking agreement (가지요 — though usually split 지+요)
  - Most other Korean endings are already unambiguous and don't need tags."
        }
        Language::Japanese => {
            "\n\nExtra Japanese-specific guidance:
- Always split productive verb inflection endings from the stem: `食べます → 食べ + ます`, `食べた → 食べ + た`, `食べない → 食べ + ない`, `食べて → 食べ + て`, `食べられる → 食べ + られる`, `食べさせる → 食べ + させる`.
- For ichidan (一段) verbs in dictionary form, split the `る`: `食べる → 食べ + る`, `見る → 見 + る`.
- i-adjective inflections are separable: `大きかった → 大き + かった`, `大きくない → 大き + く + ない`, `大きくて → 大き + く + て`, `大きければ → 大き + ければ`.
- Productive derivational suffixes: `-化` (-ization), `-的` (-ic), `-者` (agent), `-性` (-ness), `-さ` (nominalizer), `-感` (feeling), `-性`, `-風`.
- Kanji compounds: only split if each kanji is productively used across many modern words with a consistent meaning. Lexicalized compounds like `全然`, `大半`, `無理` stay as single morphemes.
- Tag disambiguation for multi-function particles and endings:
  - `で`: `[loc]` (東京で) vs `[inst]` (ペンで) vs `[cop]` (copula te-form: 学生で)
  - `に`: `[dat]` (友達に) vs `[loc]` (部屋に) vs `[adv]` (na-adj adverbial: 静かに) vs `[purp]`
  - `の`: `[gen]` (私の) vs `[nmlz]` (読むの) vs `[attr]`
  - `と`: `[quot]` (〜と言う) vs `[com]` (友達と) vs `[cond]` (雨が降ると)
  - `から`: `[abl]` source (学校から) vs `[cause]` reason (寒いから)
  - `が`: `[nom]` (私が) vs `[advers]` conjunction (〜が、…)
  - `も`: `[add]` also (私も) vs `[emph]` even (100円も)
  - `は`: `[top]` vs `[contr]` contrastive
  - `な`: `[adn]` na-adj attributive (きれいな花) vs `[sfp]` sentence-final (すごいな)
  - `-させる`: `[caus]` vs `[perm]` permissive (let X do)"
        }
        Language::Russian => {
            "\n\nExtra Russian-specific guidance:
- Russian case/gender/number endings are heavily syncretic — the same surface often serves many roles. **Tag aggressively** when the canonical alone can't disambiguate. Examples:
  - `а (а)`: `[nom.sg.f]` (книга) vs `[gen.sg.m]` (стола) vs `[nom.pl.n]` (окна) vs `[sht.f]` short-form adjective feminine (красива)
  - `у (у)`: `[dat.sg.m]` (столу) vs `[acc.sg.f]` (книгу) vs `[1sg.prs]` (говорю)
  - `и (и)`: `[nom.pl]` (столы — actually ы) / `[gen.sg.f]` (книги) / `[imp.2sg]` (говори)
  - `е (е)`: `[prp.sg]` (столе) vs `[dat.sg.f]` (книге)
  - `ом (ом)`: noun `[ins.sg.m]`/`[ins.sg.n]` (столом) vs adjective `[prp.sg.m]`/`[prp.sg.n]` (о красивом)
  - `ой (ой)`: noun `[ins.sg.f]` (женой) vs adjective `[gen.sg.f]` / `[dat.sg.f]` / `[ins.sg.f]` / `[prp.sg.f]` — same ending for all four oblique feminine-singular adjective cases
  - `ого (ого)`: adjective `[gen.sg.m]`/`[gen.sg.n]` (красивого) or `[acc.sg.m.anim]` (animate accusative)
- Verb person endings: `у (у) [1sg.prs]`, `ешь (ешь) [2sg.prs]`, `ет (ет) [3sg.prs]`, `ем (ем) [1pl.prs]`, `ете (ете) [2pl.prs]`, `ут (ут) [3pl.prs]`.
- Past tense `л/ла/ло/ли` already encodes gender/number distinctly; tag with `[pst.sg.m]`, `[pst.sg.f]`, `[pst.sg.n]`, `[pst.pl]`.
- Past adverbial participles: `в (в) [adv.pst]`, `вши (вши) [adv.pst]` (сказав, сделавши).
- Reflexive `-ся` is tagged `[refl]` to distinguish it from the (rare) non-reflexive contexts."
        }
        Language::Hindi => {
            "\n\nExtra Hindi-specific guidance:
- Hindi verb/adjective agreement endings collapse gender, number, and aspect onto a few vowel suffixes. Tag to disambiguate:
  - Perfective vs adjective: `ा (ा) [pfv.m.sg]` (खाया) vs `ा (ा) [m.sg.dir]` (बड़ा); `ी (ी) [pfv.f]` (खाई) vs `ी (ी) [f]` (बड़ी); `े (े) [pfv.m.pl]` (खाए) vs `े (े) [m.pl]`/`[m.obl]` (बड़े)
  - Habitual: `ता (ता) [hab.m.sg]` vs `ती (ती) [hab.f]` vs `ते (ते) [hab.m.pl]`
  - Case: `ों (ों) [obl.pl]`, `एं (एं) [f.pl]`, `यों (यों) [f.obl.pl]`
  - `-ना`: `[inf]` (करना) vs `[proh]` prohibitive (मत जाना don't go) vs `[inf.obl]` oblique infinitive as verbal noun (करने से)
  - `-कर`: `[conj.ptcp]` conjunctive participle (खाकर having eaten)
  - `-वाला`: `[agent]` (चायवाला tea seller, दिल्लीवाला from Delhi) vs `[immin]` imminent future (जाने वाला है about to go)
  - Causatives: `ा (ा) [caus1]` (चलाना from चलना) vs `वा (वा) [caus2]` (चलवाना)
  - Copula forms already encode person/number/gender: `हूँ [1sg.prs]`, `है [3sg.prs]`, `हैं [3pl.prs]`, `था [pst.m.sg]`, `थी [pst.f.sg]`, `थे [pst.m.pl]`, `थीं [pst.f.pl]` — tag these even though their canonicals are distinct forms, since a learner will want to know the grammatical role.
- Don't split Hindi postpositions (को, से, में, पर, तक, etc.) — they're separate words, not suffixes."
        }
        Language::German => {
            "\n\nExtra German-specific guidance:
- German suffixes are famously syncretic across case/gender/number and part-of-speech. **Tag aggressively.**
  - `-t` is the most overloaded ending in German: `[3sg.prs]` (er geht) vs `[2pl.prs]` (ihr geht) vs `[ptcp]` (gemacht) vs `[sbjv.i.3sg]` quotative subjunctive (er gehe — well, that's -e; for -t use: er habe getan)
  - `-st`: `[2sg.prs]` (du machst) vs `[sup]` superlative marker (schönst-)
  - `-nd`: `[prs.ptcp]` (laufend)
  - `-en`: `[inf]` (gehen) vs `[pl.n]` (dat.pl like Büchern) vs `[dat.pl]` vs `[3pl.prs]` (they go) vs `[wk.acc.m.sg]` vs `[sbjv.i.pl]`
  - `-e`: `[1sg.prs]` (ich gehe) vs `[pl]` (Tage) vs `[wk.nom]` (weak-declension adjective nom)
  - `-er`: `[agent]` (Lehrer) vs `[comp]` (schöner) vs `[pl.m/n]` (Kinder) vs `[st.nom.m.sg]` (guter)
  - `-es`: `[gen.sg.m/n]` (des Hauses) vs `[st.nom.n]`
  - `ge-` prefix: `[ptcp]` (past participle: gemacht) vs `[coll]` collective-noun derivation (Gebirge, Gerede)
- Separable prefixes like auf-, aus-, ein-, mit- are productive morphemes and don't usually need tags."
        }
        Language::Spanish => {
            "\n\nExtra Spanish-specific guidance:
- Plural / gender / verb endings share surfaces. Tag to disambiguate:
  - `a (a)`: `[f.sg]` (casa) vs `[3sg.prs]` for -ar verbs (habla) vs `[3sg.sbjv]` for -er/-ir verbs (coma, viva) vs `[impv.2sg]` for -ar verbs (habla! command)
  - `o (o)`: `[m.sg]` (año, libro) vs `[1sg.prs]` (hablo)
  - `e (e)`: `[3sg.prs]` for -er/-ir verbs (come) vs `[3sg.sbjv]`/`[1sg.sbjv]` for -ar verbs (hable) vs `[impv.2sg.formal]` (hable usted)
  - `s (s)`: `[pl]` (casas) vs `[2sg.prs]` (hablas)
  - `n (n)`: tag 3pl across tenses — `[3pl.prs]`, `[3pl.ipfv]`, `[3pl.pret]`, `[3pl.sbjv]` depending on the stem+thematic-vowel
  - `ado`/`ido`: `[ptcp]` past participle (he hablado) vs `[adj]` lexicalized adjective (cansado, aburrido)
  - `se (se)`: `[refl]` reflexive (lavarse) vs `[impers]` impersonal (se habla español) vs `[pass]` passive-se vs `[rcp]` reciprocal
  - `ra (ra)`: `[sbjv.ipfv]` imperfect subjunctive (hablara) vs `[plpf]` archaic pluperfect indicative
- Fused tense-person endings like `amos`, `aste`, `ado`, `iste` can stay as one tagged morpheme (`amos (amos) [1pl.prs]`) when further splitting isn't productive."
        }
        Language::Italian => {
            "\n\nExtra Italian-specific guidance:
- Plural/gender markers overlap with verb endings. Tag:
  - `i (i)`: `[m.pl]` (libri) vs `[2sg.prs]` (parli) vs `[3sg.sbjv]` for -are verbs (che tu parli, che lui parli) vs `[impv.2sg]` for -ere/-ire (prendi!)
  - `e (e)`: `[f.pl]` (case) vs `[3sg.prs]` for -ere/-ire verbs (vive, dorme) vs `[2sg.sbjv]` for -are verbs
  - `a (a)`: `[f.sg]` (casa) vs `[3sg.prs]` for -are verbs (parla)
  - `o (o)`: `[m.sg]` (libro) vs `[1sg.prs]` (parlo)
  - `to`/`ta`/`ti`/`te`: past participle with agreement — `[ptcp.m.sg]`/`[ptcp.f.sg]`/`[ptcp.m.pl]`/`[ptcp.f.pl]` (distinct from bare nouns/adjectives with the same ending)
  - `ssi (ssi)`: `[sbjv.ipfv.1sg]`/`[sbjv.ipfv.2sg]` imperfect subjunctive (parlassi)
  - `va (va)`: `[3sg.ipfv]` imperfect (parlava — distinct from the suppletive verb root va- of andare)
- `no (no) [3pl.prs]` in verbs. `mente (mente) [adv]` for adverbializer."
        }
        Language::Portuguese => {
            "\n\nExtra Portuguese-specific guidance:
- Plural/gender markers overlap with verb endings. Tag:
  - `a (a)`: `[f.sg]` (casa) vs `[3sg.prs]` (fala)
  - `o (o)`: `[m.sg]` (carro) vs `[1sg.prs]` (falo)
  - `s (s)`: `[pl]` (casas) vs `[2sg.prs]` (falas)
  - `m (m)`: `[3pl.prs]` (falam)
  - `ia (ia)`: `[ipfv]` imperfect for -er/-ir verbs (comia, vivia) vs `[cond]` conditional for all verbs (falaria, comeria) — same surface, completely different TAM. This is the headline Portuguese ambiguity.
  - `ra (ra)`: `[plpf]` pluperfect (falara) vs `[3sg.fut]` (falará — the accent usually disambiguates orthographically, but the bare morph doesn't)
  - `sse (sse)`: `[sbjv.ipfv]` imperfect subjunctive (falasse)
  - `ado`/`ido`: `[ptcp]` past participle vs `[adj]` lexicalized adjective
  - `ão (ão)` has many functions: `[3pl.fut]` (falarão), `[3pl.prs]` for irregulars (são, têm, vão), `[aug]` augmentative (carrão), `[nmlz]` in `-ação`/`-ção` nominalizations (though those have distinct canonicals)
- `mente (mente) [adv]` for adverbializer."
        }
        Language::Chinese => {
            "\n\nExtra Chinese-specific guidance:
- Chinese has minimal grammatical morphology, but several characters serve multiple grammatical roles on the same surface form. Tag aggressively for these:
  - `了`: `[pfv]` perfective aspect (吃了 ate) vs `[cos]` change-of-state sentence-final
  - `的`: `[attr]` attributive (我的书) vs `[nmlz]` nominalizer (红的 the red one)
  - `过 (过) [exp]` experiential aspect
  - `着 (着) [dur]` durative aspect
  - `们 (们) [pl]` animate plural
  - `在`: `[loc]` locative preposition (在学校) vs `[prog]` progressive aspect (在吃) vs `[exist]` existence verb (to be located)
  - `给`: `[give]` main verb vs `[dat]` dative preposition (给我写信) vs `[pass]` colloquial passive marker
  - `要`: `[want]` desire/necessity vs `[fut]` future/prospective (要下雨了) vs `[must]` modal obligation
  - `会`: `[abil]` learned ability (会说中文) vs `[fut]` probabilistic future (会下雨) vs `[prob]` epistemic
  - `让`: `[caus]` causative (\"make\") vs `[pass]` passive marker
  - `得`: different tones/functions share the character — `[obtain]` (dé as full verb) vs `[compl]` verb-complement marker (de — 跑得快) vs `[must]` (děi obligation: 得走了)
  - `地`: `[adv]` adverbializer de (慢慢地) vs noun \"ground\" dì. Different tones, same character."
        }
        Language::English | Language::French => "",
    }
}

/// Deterministically pick up to 20 multi-morpheme example entries from the
/// golden set to show the LLM.
fn build_examples(entries: &[AlignedEntry]) -> Vec<&AlignedEntry> {
    let multi: Vec<&AlignedEntry> = entries.iter().filter(|e| e.segments.len() >= 2).collect();

    let mut sampled = sample_to_target(multi, 30, |e| e.word.clone());
    sampled.sort_by(|a, b| a.word.cmp(&b.word));
    sampled.truncate(20);
    sampled
}

/// Render one morpheme the way the LLM is asked to render its output:
/// `"surface (canonical)"` or `"surface (canonical) [tag]"` when the tag is present.
fn format_morpheme(seg: &MorphemeSegment<String>) -> String {
    match &seg.tag {
        Some(tag) => format!("{} ({}) [{}]", seg.surface, seg.canonical, tag),
        None => format!("{} ({})", seg.surface, seg.canonical),
    }
}

/// Render one example the way the LLM is asked to render its output.
fn format_example(entry: &AlignedEntry) -> String {
    let pairs: Vec<String> = entry.segments.iter().map(format_morpheme).collect();
    format!(
        "word: {word}\nspelled: {spelled}\nmorphemes: {json}",
        word = entry.word,
        spelled = spell_out(&entry.word),
        json = serde_json::to_string(&pairs).unwrap(),
    )
}

/// Run the LLM on `candidate_words`, returning one `AlignedEntry` per word
/// that was successfully split into at least two morphemes whose surfaces
/// concatenate back to the original word.
pub async fn generate_llm_aligned_entries(
    course: Course,
    candidate_words: &[String],
    golden_entries: &[AlignedEntry],
    word_morphology: &std::collections::BTreeMap<String, String>,
) -> Vec<AlignedEntry> {
    if candidate_words.is_empty() {
        return Vec::new();
    }

    let language = course.target_language;
    let examples = build_examples(golden_entries);
    let examples_block = examples
        .iter()
        .map(|e| format_example(e))
        .collect::<Vec<_>>()
        .join("\n\n");

    let language_notes = language_specific_notes(language);
    let system_prompt = format!(
        r#"You are an expert {language} morphologist. Please split the given {language} word into its morphemes.

For each morpheme, return a single string of the form `"surface (canonical)"`, optionally followed by a `[tag]` in square brackets:
- `surface`  — the literal substring of the word covered by this morpheme, verbatim. Concatenating every `surface` in order MUST reproduce the input word, character for character.
- `canonical` — the dictionary / lemma form the morpheme refers to. For verb stems use the infinitive / dictionary form (English `stabl (stable)`; French `mang (manger)`; Korean `먹 (먹다)`). For nouns and adjectives use the bare dictionary headword. For purely grammatical morphemes that don't have a dictionary entry of their own, `canonical` is just the morpheme itself (e.g. `ed (ed)`, `요 (요)`).
- `[tag]` (optional) — a short grammatical descriptor in square brackets, added **only when the same (surface, canonical) pair could represent different underlying morphemes** and disambiguation is needed. Use a short UniMorph-style tag like `[pl]`, `[1sg]`, `[3sg]`, `[past]`, `[ptcp]`, `[prog]`, `[poss]`, `[prs.ind.1sg]`. Examples:
  - English plural vs. verb 3sg: `"s (s) [pl]"` (cats) vs. `"s (s) [3sg]"` (runs)
  - English past vs. past-participle: `"ed (ed) [past]"` (walked) vs. `"ed (ed) [ptcp]"` (has walked)
  - French plural vs. verb 1sg: `"s (s) [pl]"` (chats) vs. `"s (s) [1sg]"` (je fais)
  - Simply omit `[tag]` when the canonical alone uniquely identifies the morpheme (e.g. `"ly (ly)"` for English adverbializer has no ambiguity).
- **Use combined tags for genuinely syncretic forms.** When a single surface form covers multiple grammatical functions at once (not just "could mean different things in different words" but actually IS both readings in this exact word), join them with `/` rather than arbitrarily picking one:
  - French `-e` on -er verbs is both 1sg and 3sg present indicative (je parle / il parle both end in `-e`). Tag as `[prs.ind.1sg/3sg]` or `[1sg/3sg]`, not just `[3sg]`.
  - French `-s` on most -re/-ir verbs is both 1sg and 2sg present indicative (je fais / tu fais, je finis / tu finis). Tag as `[prs.ind.1sg/2sg]` or `[1sg/2sg]`.
  - Italian `-i` on -are verbs is both 2sg indicative (parli "you speak") and 2sg/3sg subjunctive. Tag as appropriate, using a combined tag when the form is genuinely ambiguous for a learner.
  - Picking just one reading when both are correct produces a misleading gloss downstream. Combined tags tell the gloss generator to describe all the readings.
- The user message may include a `known readings:` line listing the grammatical analyses of the word (Leipzig-style, e.g. `VERB rester [1.SG.PRS.IND / 3.SG.PRS.IND]`). **Treat this as authoritative** — base your tags on those readings so you capture all of the actual grammatical functions of the word, not just the most common one.

Rules:
- Split only along real morpheme boundaries: productive prefixes, roots/stems, derivational suffixes, inflectional endings.
- **Only split when each piece is a productive morpheme** — i.e. appears as a meaningful component across many other words in the language. If a compound is lexicalized and its pieces aren't productive in modern usage (like English `window` would break into `win + dow`, or Japanese `全然` into `全 + 然` — neither `dow` nor `然` is productive), leave it as a single morpheme. Err on the side of NOT splitting CJK compounds into individual characters unless each character is clearly productive.
- Preserve the left-to-right order the morphemes appear in the word.
- If the word is a simple monomorphemic root, return a single entry with `surface` and `canonical` both equal to the whole word; I will filter those out.
- Do not split proper nouns into letters or syllables.
- Do not include the spaces I insert in `spelled` — that's a reading aid for you, not part of the word.
- **Literal concatenation rule (especially for Korean)**: the surfaces must concatenate byte-for-byte to reproduce the word. Do NOT split across boundaries that involve phonological contraction, vowel fusion, jamo-level endings, or irregular-verb alternations. Examples where you must leave the word as a single morpheme instead of splitting:
  - Korean `해요` — cannot be split as `하 + 아 + 요` because `하` + `아` would not concatenate to `해` (vowel fusion)
  - Korean `갔어요` — cannot be split as `가 + 았 + 어요` because `가` + `았` ≠ `갔`
  - Korean `예쁜` — cannot be split as `예쁘 + ㄴ` because `예쁘` + `ㄴ` ≠ `예쁜` (jamo-level attachment)
  - Korean `도와요` (from 돕다) — cannot be split as `돕 + 아 + 요` (ㅂ-irregular alternation)
  Only split Korean verbs/adjectives on clean consonant-final stem + ending boundaries (e.g. `먹어요 → 먹 + 어 + 요`, `읽었어요 → 읽 + 었 + 어 + 요`).{language_notes}

Here are {n} real examples:

{examples_block}
"#,
        n = examples.len(),
    );

    let pb = ProgressBar::new(candidate_words.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} etymology splits ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let results: Vec<AlignedEntry> = futures::stream::iter(candidate_words.iter())
        .map(|word| {
            let pb = pb.clone();
            let system_prompt = system_prompt.clone();
            async move {
                let morphology_line = word_morphology
                    .get(word.as_str())
                    .map(|m| format!("\nknown readings: {m}"))
                    .unwrap_or_default();
                let user = format!(
                    "word: {word}\nspelled: {spelled}{morphology_line}",
                    spelled = spell_out(word),
                );
                let response: Result<EtymologyResponse, _> = CHAT_CLIENT
                    .chat_with_system_prompt(system_prompt, user)
                    .await;

                pb.set_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));
                pb.inc(1);

                let response = response.ok()?;
                if response.morphemes.is_empty() {
                    return None;
                }

                let segments: Vec<MorphemeSegment<String>> = response
                    .morphemes
                    .iter()
                    .map(|m| parse_morpheme_pair(m))
                    .collect::<Option<Vec<_>>>()?;

                // Surfaces must literally concatenate back to the word.
                if segments
                    .iter()
                    .map(|s| s.surface.as_str())
                    .collect::<String>()
                    != *word
                    || segments.iter().any(|s| s.surface.is_empty())
                {
                    return None;
                }

                Some(AlignedEntry {
                    word: word.clone(),
                    segments,
                })
            }
        })
        .buffer_unordered(50)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect();

    pb.finish_with_message(format!("{:.2}", CHAT_CLIENT.cost().unwrap_or(0.0)));

    results
}

/// Load the golden JSONL, ask the LLM to split any `candidate_words` that
/// aren't covered, and return a merged list of aligned entries.
pub async fn augment_with_llm_aligned(
    course: Course,
    golden_path: &Path,
    candidate_words: &[String],
    word_morphology: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<Vec<AlignedEntry>> {
    let golden_entries = load_golden_morphemes(golden_path);

    let covered: std::collections::HashSet<&str> =
        golden_entries.iter().map(|e| e.word.as_str()).collect();

    let uncovered: Vec<String> = candidate_words
        .iter()
        .filter(|w| !covered.contains(w.as_str()))
        .cloned()
        .collect();

    println!(
        "  {} candidate words, {} golden entries, {} uncovered (querying LLM)",
        candidate_words.len(),
        golden_entries.len(),
        uncovered.len(),
    );

    let llm_entries =
        generate_llm_aligned_entries(course, &uncovered, &golden_entries, word_morphology).await;
    println!("  LLM produced {} aligned entries", llm_entries.len());

    let mut aligned = golden_entries;
    aligned.extend(llm_entries);
    Ok(aligned)
}
