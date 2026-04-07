// =============================================================================
// VERB LISTS
// =============================================================================

/// Common ichidan verbs (一段動詞). Lemma always ends in る, and the mora
/// before る is from the え-row or い-row. This list is used for volitional
/// う→よう correction and other ichidan-specific checks.
///
/// NOT exhaustive — this is a high-confidence starter set of verbs that are
/// unambiguously ichidan. Godan-る verbs (走る, 帰る, 切る, etc.) are
/// deliberately excluded.
const ICHIDAN_VERBS: &[&str] = &[
    // --- High frequency ---
    "いる",     // to exist (animate)
    "見る",     // to see
    "出る",     // to exit
    "食べる",   // to eat
    "考える",   // to think
    "教える",   // to teach
    "覚える",   // to remember
    "変える",   // to change (trans.)
    "変わる",   // NOTE: godan — DO NOT include. here as a reminder.
    "始める",   // to begin (trans.)
    "決める",   // to decide
    "止める",   // to stop (trans.)
    "開ける",   // to open (trans.)
    "閉める",   // to close (trans.)
    "つける",   // to attach/turn on
    "受ける",   // to receive
    "上げる",   // to raise
    "下げる",   // to lower
    "見せる",   // to show
    "伝える",   // to convey
    "答える",   // to answer
    "調べる",   // to investigate
    "比べる",   // to compare
    "並べる",   // to line up
    "育てる",   // to raise/grow
    "建てる",   // to build
    "立てる",   // to stand (trans.)
    "当てる",   // to hit/guess
    "捨てる",   // to throw away
    "慣れる",   // to get used to
    "疲れる",   // to get tired
    "生まれる", // to be born
    "倒れる",   // to collapse
    "壊れる",   // to break (intrans.)
    "離れる",   // to separate
    "逃げる",   // to flee
    "投げる",   // to throw
    "混ぜる",   // to mix
    "見つける", // to find
    "続ける",   // to continue (trans.)
    "届ける",   // to deliver
    "助ける",   // to help/save
    "分ける",   // to divide
    "負ける",   // to lose
    "迎える",   // to welcome
    "加える",   // to add
    "与える",   // to give/grant
    "抑える",   // to suppress
    "支える",   // to support
    "備える",   // to prepare
    "構える",   // to set up
    "据える",   // to install
    "唱える",   // to recite/advocate
    "訴える",   // to sue/appeal

    // --- Perception/cognition ---
    "感じる",   // to feel
    "信じる",   // to believe
    "応じる",   // to respond
    "生じる",   // to arise
    "通じる",   // to communicate
    "禁じる",   // to prohibit
    "命じる",   // to order
    "論じる",   // to discuss
    "案じる",   // to worry about
    "報じる",   // to report

    // --- Movement/position ---
    "寝る",     // to sleep
    "起きる",   // to wake up
    "降りる",   // to get off
    "乗せる",   // to give a ride
    "寄せる",   // to bring near
    "落ちる",   // to fall
    "過ぎる",   // to pass/exceed
    "すぎる",   // to pass/exceed (kana)

    // --- Communication ---
    "知らせる", // to inform
    "褒める",   // to praise
    "認める",   // to recognize
    "求める",   // to seek
    "進める",   // to advance (trans.)
    "勧める",   // to recommend
    "務める",   // to serve as
    "努める",   // to endeavor

    // --- Creation/destruction ---
    "入れる",   // to put in
    "出かける", // to go out
    "片付ける", // to tidy up
    "取り付ける", // to install
    "組み立てる", // to assemble
    "作り上げる", // to complete

    // --- State change ---
    "増える",   // to increase (intrans.)
    "減る",     // NOTE: godan — DO NOT include
    "冷える",   // to get cold
    "温める",   // to warm up
    "固める",   // to harden (trans.)
    "広げる",   // to spread (trans.)
    "狭める",   // to narrow
    "深める",   // to deepen
    "高める",   // to heighten
    "強める",   // to strengthen
    "弱める",   // to weaken
    "早める",   // to hasten
    "遅れる",   // to be late
    "枯れる",   // to wither
    "腐れる",   // to rot
    "汚れる",   // to get dirty
    "晴れる",   // to clear up

    // --- Everyday actions ---
    "着る",     // to wear (upper body)
    "浴びる",   // to bathe in
    "足りる",   // to suffice
    "飽きる",   // to get bored
    "できる",   // to be able
    "似る",     // to resemble
    "煮る",     // to simmer
    "干る",     // to dry (NOTE: uncommon, usually 干す godan)

    // --- Compound/derived ---
    "見える",   // to be visible
    "聞こえる", // to be audible
    "消える",   // to disappear
    "現れる",   // to appear
    "表れる",   // to manifest
    "溢れる",   // to overflow
    "恐れる",   // to fear
    "訪れる",   // to visit

    // --- させる/られる auxiliaries (lemma forms) ---
    // These are ichidan too, relevant for the volitional check
    "させる",   // causative
    "られる",   // passive/potential
];

/// Godan verbs that end in る — these are NOT ichidan despite ending in る.
/// Used as a negative filter: if a verb's lemma is in this list, don't apply
/// ichidan-specific rules.
const GODAN_RU_VERBS: &[&str] = &[
    "走る",     // to run
    "帰る",     // to return
    "切る",     // to cut
    "知る",     // to know
    "入る",     // to enter
    "座る",     // to sit
    "通る",     // to pass through
    "取る",     // to take
    "送る",     // to send
    "作る",     // to make
    "売る",     // to sell
    "乗る",     // to ride
    "残る",     // to remain
    "登る",     // to climb
    "渡る",     // to cross
    "戻る",     // to return
    "回る",     // to go around
    "上る",     // to go up
    "下る",     // to go down
    "太る",     // to get fat
    "参る",     // to go/come (humble)
    "なる",     // to become
    "ある",     // to exist (inanimate)
    "やる",     // to do
    "くれる",   // NOTE: ichidan! here as a reminder NOT to include
    "要る",     // to need
    "釣る",     // to fish
    "塗る",     // to paint
    "握る",     // to grip
    "練る",     // to knead
    "蹴る",     // to kick
    "散る",     // to scatter
    "照る",     // to shine
    "減る",     // to decrease
    "滑る",     // to slip
    "喋る",     // to chat
    "焦る",     // to be impatient
    "限る",     // to limit
    "頼る",     // to rely on
    "怒る",     // to get angry
    "祈る",     // to pray
    "眠る",     // to sleep
    "異なる",   // to differ
    "至る",     // to reach
    "被る",     // to suffer/wear (hat)
    "遮る",     // to block
    "罵る",     // to curse
];

fn is_ichidan(lemma: &str) -> bool {
    // Quick check: must end in る
    if !lemma.ends_with("る") {
        return false;
    }
    // する/くる are irregular, not ichidan
    if lemma == "する" || lemma == "くる" || lemma == "来る" {
        return false;
    }
    // Check explicit lists first
    if ICHIDAN_VERBS.contains(&lemma) {
        return true;
    }
    if GODAN_RU_VERBS.contains(&lemma) {
        return false;
    }
    // Heuristic fallback for verbs not in either list:
    // If the mora before る is え-row or い-row kana, likely ichidan.
    // This is wrong for some verbs (帰る, 切る) but those should be in
    // the godan list above. For unknown verbs, ichidan is the safer guess
    // since most る-ending verbs with え/い-row pre-final mora are ichidan.
    let without_ru = &lemma[..lemma.len() - "る".len()];
    let last_char = without_ru.chars().last();
    match last_char {
        Some(c) => {
            let e_row = ['え', 'け', 'せ', 'て', 'ね', 'べ', 'め', 'れ', 'げ', 'ぜ', 'で', 'ぺ'];
            let i_row = ['い', 'き', 'し', 'ち', 'に', 'び', 'み', 'り', 'ぎ', 'じ', 'ぢ', 'ぴ'];
            e_row.contains(&c) || i_row.contains(&c)
        }
        None => false,
    }
}


// =============================================================================
// CLASSIFIER ADDITIONS (add these to JapaneseClassifier::classify)
// =============================================================================

// --- Contracted form in lemma ---
// Place this inside the token loop in classify()
{
    let contracted_forms = ["ちゃう", "ちゃった", "じゃう", "じゃった", "とく", "とった", "とけ", "とける"];
    for form in &contracted_forms {
        if token.lemma.contains(form) && token.lemma != *form {
            reasons.push(format!(
                "'{}' (lemma '{}') — lemma contains contracted form '{}'. \
                 Verify this is the actual dictionary form. \
                 Contractions: てしまう→ちゃう, でしまう→じゃう, ておく→とく.",
                text, token.lemma, form
            ));
            break;
        }
    }
}

// --- Short れる/られる as VERB might be AUX suffix ---
// Place this inside the token loop in classify()
{
    if token.pos == PartOfSpeechTag::Verb
        && (text == "れる" || text == "られる" || text == "れた" || text == "られた"
            || text == "れて" || text == "られて" || text == "れない" || text == "られない")
    {
        reasons.push(format!(
            "'{}' tagged as VERB — verify: when れる/られる is a passive/potential suffix, \
             it should be AUX. VERB is correct only for standalone use (rare).",
            text
        ));
    }
}


// =============================================================================
// CORRECTOR ADDITIONS (add these to JapaneseCorrector::correct)
// =============================================================================

// --- ございます → ござる ---
// Same pattern as every other verb: strip ます, lemma is plain form.
// ござる is the dictionary form even though the bare form is archaic in
// modern Japanese. We normalize to ござる for consistency with all other
// verb lemmas (食べます→食べる, 行きます→行く, etc.)
{
    if (token.text == "ございます" || token.text == "ございました"
        || token.text == "ございません")
        && token.lemma != "ござる"
    {
        corrections.push(format!(
            "Fixed '{}' lemma from '{}' to 'ござる'",
            token.text, token.lemma
        ));
        token.lemma = "ござる".to_string();
        corrected = true;
    }
}

// --- そう lemma lockdown ---
{
    if token.text == "そう" && token.pos == PartOfSpeechTag::Aux && token.lemma != "そう" {
        corrections.push(format!(
            "Fixed 'そう' (AUX) lemma from '{}' to 'そう'",
            token.lemma
        ));
        token.lemma = "そう".to_string();
        corrected = true;
    }
}


// =============================================================================
// DOUBLE-CHECK ADDITIONS (add these to JapaneseClassifier::needs_double_check)
// =============================================================================

// --- Volitional う after ichidan/する → should be よう ---
{
    if token.pos == PartOfSpeechTag::Aux
        && token.lemma == "う"
        && idx > 0
    {
        let prev = &tokens[idx - 1];
        let prev_is_ichidan = prev.pos == PartOfSpeechTag::Verb
            && is_ichidan(&prev.lemma);
        let prev_is_suru = prev.pos == PartOfSpeechTag::Verb
            && (prev.lemma == "する" || prev.lemma.ends_with("する"));
        let prev_is_kuru = prev.pos == PartOfSpeechTag::Verb
            && (prev.lemma == "くる" || prev.lemma == "来る");

        if prev_is_ichidan || prev_is_suru || prev_is_kuru {
            reasons.push(format!(
                "'{}' (AUX, lemma 'う') after '{}' (lemma '{}') — \
                 volitional after ichidan/する/くる should have lemma 'よう', not 'う'. \
                 う is the godan volitional suffix.",
                token.text, prev.text, prev.lemma
            ));
        }
    }
}

// --- VERB text containing 達/たち → should split ---
{
    if token.pos == PartOfSpeechTag::Verb
        && (token.text.ends_with("達") || token.text.ends_with("たち"))
        && token.text.chars().count() > 1
    {
        reasons.push(format!(
            "'{}' (VERB) ends with 達/たち — verify: \
             this may be a noun+plural merged into the verb token. \
             Should split if 達/たち is a plural suffix.",
            token.text
        ));
    }
}

// --- ADJ with だ-lemma not on known na-adjective list ---
{
    let common_na_adjectives = [
        "きれい", "静か", "大切", "大変", "元気", "有名", "便利", "不便",
        "親切", "丁寧", "簡単", "複雑", "重要", "特別", "自由", "安全",
        "危険", "可能", "不可能", "素敵", "立派", "無理", "大丈夫", "心配",
        "好き", "嫌い", "上手", "下手",
    ];
    if token.pos == PartOfSpeechTag::Adj
        && token.lemma.ends_with("だ")
    {
        let stem = &token.lemma[..token.lemma.len() - "だ".len()];
        if !common_na_adjectives.contains(&stem) {
            // Check context: is な or だ/です nearby?
            let next_is_na = tokens.get(idx + 1).is_some_and(|n| n.text == "な");
            let next_is_copula = tokens.get(idx + 1).is_some_and(|n| {
                matches!(n.text.as_str(), "だ" | "です" | "でした" | "だった")
            });
            if !next_is_na && !next_is_copula {
                reasons.push(format!(
                    "'{}' tagged ADJ with lemma '{}' — this word isn't on the known \
                     na-adjective list, and it doesn't appear before な or copula. \
                     Verify this is actually a na-adjective and not a noun used predicatively \
                     (e.g., '絶品' is a noun, not a na-adjective).",
                    token.text, token.lemma
                ));
            }
        }
    }
}


// =============================================================================
// POST-CORRECTOR ADDITIONS (add to JapaneseCorrector::post_corrections)
// =============================================================================

// --- Volitional う → よう after ichidan/する/くる ---
// Deterministic: ichidan and する/くる NEVER use う for volitional.
{
    for i in 1..tokens.len() {
        if tokens[i].pos == PartOfSpeechTag::Aux && tokens[i].lemma == "う" {
            let prev = &tokens[i - 1];
            let prev_is_ichidan = prev.pos == PartOfSpeechTag::Verb
                && is_ichidan(&prev.lemma);
            let prev_is_suru = prev.pos == PartOfSpeechTag::Verb
                && (prev.lemma == "する" || prev.lemma.ends_with("する"));
            let prev_is_kuru = prev.pos == PartOfSpeechTag::Verb
                && (prev.lemma == "くる" || prev.lemma == "来る");

            if prev_is_ichidan || prev_is_suru || prev_is_kuru {
                tokens[i].lemma = "よう".to_string();
            }
        }
    }
}

// --- ございます lemma in post-correction (same as corrector, safety net) ---
{
    for token in tokens.iter_mut() {
        if (token.text == "ございます" || token.text == "ございました"
            || token.text == "ございません")
            && token.lemma != "ござる"
        {
            token.lemma = "ござる".to_string();
        }

        // そう AUX lemma
        if token.text == "そう" && token.pos == PartOfSpeechTag::Aux && token.lemma != "そう" {
            token.lemma = "そう".to_string();
        }
    }
}


// =============================================================================
// LOGGING-ONLY: contracted lemma detection
// =============================================================================

/// Call this in post_corrections or a separate audit pass. Does not modify
/// tokens — just collects warnings for bulk review.
fn log_contracted_lemmas(tokens: &[SimplifiedTokenPrime]) -> Vec<String> {
    let mut warnings = Vec::new();
    let contracted_markers = ["ちゃ", "じゃ", "とく", "とっ"];

    for token in tokens {
        if token.pos == PartOfSpeechTag::Verb {
            for marker in &contracted_markers {
                if token.lemma.contains(marker) {
                    // TODO: check against a known-verb dictionary if you build one
                    warnings.push(format!(
                        "CONTRACTED_LEMMA: '{}' (VERB, lemma '{}') contains '{}' — \
                         may be a contraction left in the lemma. \
                         Expected dictionary forms: ちゃう→てしまう, じゃう→でしまう, とく→ておく.",
                        token.text, token.lemma, marker
                    ));
                }
            }
        }
    }

    warnings
}