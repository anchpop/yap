"""spaCy sentence analysis for the gold-labeling pipeline (clean-nlp-data).

Parses sentences into tokens (text/whitespace/lemma/pos/morph) as NlpAnalyzedSentence
JSONL. Multiword terms are not detected here: they are downloaded from Wiktionary and
fed through this same pipeline as if they were sentences, so `multiword_terms` on
each record is always emitted empty.
"""
import json
import sys

from tqdm import tqdm

# spacy is imported lazily inside load_model: Japanese goes through Sudachi and has no
# reason to pay for spacy's import (or to fail when its numpy stack is unhappy).

# Model mapping for different languages
MODEL_MAPPING = {
    "fra": {
        "small": "fr_core_news_sm",
        "large": "fr_dep_news_trf"
    },
    "spa": {
        "small": "es_core_news_sm",
        "large": "es_dep_news_trf"
    },
    "kor": {
        "small": "ko_core_news_sm",
        "large": "ko_core_news_lg"
    },
    "eng": {
        "small": "en_core_web_sm",
        "large": "en_core_web_trf"
    },
    "deu": {
        "small": "de_core_news_sm",
        "large": "de_dep_news_trf"
    },
    # Chinese does not go through spaCy — see the HanLP section below.
    "ita": {
        "small": "it_core_news_lg",
        "large": "it_core_news_lg"
    },
    "por": {
        "small": "pt_core_news_lg",
        "large": "pt_core_news_lg"
    },
    "jpn": {
        "small": "ja_core_news_trf",
        "large": "ja_core_news_trf"
    },
    "rus": {
        "small": "ru_core_news_lg",
        "large": "ru_core_news_lg"
    },
}

use_big_model = True


# ---------------------------------------------------------------------------------------
# Japanese: Sudachi, not spaCy
#
# Japanese used to be analyzed by an LLM from scratch, which made it the only language
# without a deterministic proposer — and it showed: gold disagreed with itself on 1.39% of
# tokens against German's 0.02%, and carved the same construction four different ways
# (ノック|し|て|いる, 到着|し|た, 終了|し|ました, 作って|い|ます).
#
# Split mode C is deliberate. Mode A strands bound morphemes that are not words — お|金,
# 図書|館, フランス|語 — which violates the rule that every token must be showable to a
# learner with a definition. C keeps those whole. Measured against gold's self-consistent
# subset, C scores 95.0 boundary F1 against UniDic/mode-A's 94.2.
#
# What Sudachi does NOT do is merge a predicate with its auxiliary chain — 食べ|まし|た stays
# split in every mode. That is applied downstream by JapaneseCorrector, and stated in the
# tokenization policy the LLM is given.
SUDACHI_SPLIT_MODE = "C"

# Sudachi's tagset is Japanese-specific and hierarchical; UPOS is the 17-tag cross-lingual
# set. The correspondence is nearly mechanical — 32 distinct keys cover our whole corpus,
# and this table agrees with gold on 94.9% of tokens. The residual is mostly cases the LLM
# is *supposed* to change (今日 as NOUN vs ADV depending on use), which is the point of
# having it review the proposal.
_SUDACHI_UPOS = {
    "動詞": "VERB",
    "助動詞": "AUX",
    "形容詞": "ADJ",
    "形状詞": "ADJ",
    "副詞": "ADV",
    "接続詞": "CCONJ",
    "感動詞": "INTJ",
    "連体詞": "DET",
    "代名詞": "PRON",
    "接頭辞": "NOUN",
    "接尾辞": "NOUN",
    "補助記号": "PUNCT",
    "記号": "SYM",
    "空白": "SPACE",
    "フィラー": "INTJ",
}
_SUDACHI_UPOS_2 = {
    ("名詞", "普通名詞"): "NOUN",
    ("名詞", "固有名詞"): "PROPN",
    ("名詞", "数詞"): "NUM",
    ("名詞", "助動詞語幹"): "AUX",
    ("助詞", "格助詞"): "ADP",
    ("助詞", "接続助詞"): "SCONJ",
    ("助詞", "係助詞"): "ADP",
    ("助詞", "副助詞"): "ADP",
    ("助詞", "終助詞"): "PART",
    ("助詞", "準体助詞"): "SCONJ",
}


def _sudachi_upos(pos, surface, prev_surface):
    """Map a Sudachi POS tuple to UPOS.

    非自立可能 verbs and adjectives (いる, ある, くる, しまう, ない) are auxiliaries when they
    follow a て-form and ordinary predicates otherwise — 食べて|いる vs 家に|いる. That single
    context rule is worth ~1.8 points of agreement with gold, so it is applied here rather
    than left for the LLM.

    Three more context rules the cleaning LLM otherwise applies by hand on every pass
    (21×/14×/13× in a 448-sentence sample):
    - adverbial に (助動詞, the copula's 連用形 in ように/静かに) is a particle: ADP
    - explanatory ん (contracted の in んです) is PART; the full の stays SCONJ
    - question/or か (副助詞) is PART, matching the 終助詞 か it alternates with
    """
    p0, p1 = pos[0], pos[1]
    if p1 == "非自立可能" and prev_surface in ("て", "で"):
        return "AUX"
    if surface == "に" and p0 == "助動詞":
        return "ADP"
    if surface == "ん" and (p0, p1) == ("助詞", "準体助詞"):
        return "PART"
    if surface == "か" and (p0, p1) == ("助詞", "副助詞"):
        return "PART"
    if (p0, p1) in _SUDACHI_UPOS_2:
        return _SUDACHI_UPOS_2[(p0, p1)]
    if p0 in _SUDACHI_UPOS:
        return _SUDACHI_UPOS[p0]
    if p0 == "名詞":
        return "NOUN"
    if p0 == "助詞":
        return "ADP"
    return "X"


# Suffixes to pull off the stem mode C merged them onto. Everything else mode C merges
# stays merged.
#
# The test is the one that governs tokenization generally: after the split, is *every* piece
# a word a learner could be shown? These pass because the stem survives intact (子供, 私,
# 田中) and the suffix is purely grammatical — pluralising or a title.
#
# Mode C merges 91 distinct suffixes in our corpus and almost none of them qualify. 翻訳者
# would leave 者, 明るさ would leave 明る, 図書館 would leave 館 — bound pieces that are not
# words, the same failure as splitting 食べ|まし|た. An earlier version of this split on any
# 接尾辞 tail and fired on 713 tokens across all 91, of which these were under 200; the rest
# reverted mode C to mode A, undoing the reason mode C was chosen.
#
# 的 (37 distinct stems) and 者 (32) are the arguable omissions — both are highly productive,
# but both strand a non-word, so they stay merged.

# Pluralising たち/達 ranges over the open class of person nouns (子供たち, 学生たち,
# 私たち): merged, every combination would need its own dictionary entry, so it always
# comes off. ら is deliberately NOT here — its real hosts are a small closed set that
# dictionaries list as single words (彼ら, 僕ら, それら, 奴ら); see _fuse_presplit_suffixes,
# which re-attaches the ら that mode C emits pre-split.
_PLURAL_SUFFIXES = {"たち", "達"}

# Title suffixes come off proper names only (田中|さん, 花子|ちゃん): titles over the open
# class of names is the same combinatorial argument as たち. On a common noun the
# combination is one lexicalized word and stays whole: 皆さん, お客さん, 神様, 赤ちゃん.
_TITLE_SUFFIXES = {"さん", "様", "くん", "ちゃん", "君"}

# Honorific prefixes are never words on their own — お|誕生日 strands お the same way
# 食べ|た strands 食べ. Mode C emits them detached; the fuse pass reattaches them
# forward. Deliberately only the honorifics: other 接頭辞 either stay fused in mode C
# already (真っ赤, 不可能) or are words in their own right (約).
_HONORIFIC_PREFIXES = {"お", "ご", "御"}


class _Stem:
    """Several A-parts reassembled into one token, quacking like a Morpheme.

    Needed because a detachable suffix may sit on a stem that is itself multi-part
    (生徒会長たち A-splits to 生徒+会長+たち — only the たち comes off), and because
    _fuse_presplit_suffixes rebuilds one token from a stem and its suffix (彼+ら).
    `head` names the part whose POS represents the whole: the last part for a
    prefix+noun stem (お+客 -> 客, a noun), the first for a stem+suffix fusion
    (彼+ら -> 彼, a pronoun).
    """

    def __init__(self, parts, head=None):
        self._parts = parts
        self._head = head if head is not None else parts[-1]

    def begin(self):
        return self._parts[0].begin()

    def end(self):
        return self._parts[-1].end()

    def surface(self):
        return "".join(p.surface() for p in self._parts)

    def normalized_form(self):
        return "".join(p.normalized_form() for p in self._parts)

    def part_of_speech(self):
        return self._head.part_of_speech()


def _detach_bound_suffix(morpheme):
    """Undo mode C's merge of a detachable suffix onto its stem: 子供達 -> 子供 + 達.

    たち attaches to any animate noun — 53 distinct stems in our corpus — so merging it would
    demand a dictionary entry for 私たち, 子供たち, 学生たち, 生徒たち and so on. That is the
    reasoning that keeps Korean particles separate: split what attaches freely, merge what
    has fused. A title comes off only when the stem is a proper name (田中|さん): on a
    common noun the whole is one word (お客さん), so it stays put.

    Whether a given 達 is the suffix or part of a word is still Sudachi's call, not ours: the
    morpheme's own A-split decomposes 子供達 but returns 友達, 配達, 到達 and 発達 unchanged,
    because those are single lexemes. So the suffix list needs no companion list of
    exceptions — 友達 (61 occurrences here) cannot split even though it ends in 達.
    """
    from sudachipy import SplitMode

    parts = list(morpheme.split(SplitMode.A))
    if len(parts) < 2:
        return [morpheme]
    tail = parts[-1]
    if tail.part_of_speech()[0] != "接尾辞":
        return [morpheme]
    stem = parts[0] if len(parts) == 2 else _Stem(parts[:-1])
    if tail.surface() in _PLURAL_SUFFIXES:
        return [stem, tail]
    if tail.surface() in _TITLE_SUFFIXES and stem.part_of_speech()[1] == "固有名詞":
        return [stem, tail]
    return [morpheme]


def _fuse_presplit_pieces(morphemes):
    """The inverse repair: mode C sometimes emits a policy-fused word in pieces.

    C is inconsistent here — it splits 神|様 but merges 王様, splits 彼|ら but merges
    私たち, splits 一|つ but merges お客様 — so the policy has to be ours, applied in
    both directions. Four fusions, each keyed to a piece that is not a word on its own:
      ら onto a nominal stem       彼ら, 奴ら — single dictionary words
      title onto a common noun     皆さん, 神様 — proper names stay split (田中|さん),
                                   mirroring _detach_bound_suffix so the passes can't
                                   ping-pong
      counter onto a numeral       一つ, 三人, 一週間 — the prompt's 三人 rule
      noun/verb onto お/ご/御      お誕生日, ご予定, お呼び — the prefix is stranded
    """
    fused = []
    for m in morphemes:
        head = _fuse_head(fused[-1], m) if fused else None
        if head is not None:
            prev = fused.pop()
            parts = (prev._parts if isinstance(prev, _Stem) else [prev]) + [m]
            fused.append(_Stem(parts, head=head))
        else:
            fused.append(m)
    return fused


def _fuse_head(prev, m):
    """If `m` belongs inside the same token as `prev`, return the part whose POS
    represents the fused whole; otherwise None."""
    if isinstance(m, _Stem):
        return None
    # written apart stays apart — fusing across whitespace would drop it
    if prev.end() != m.begin():
        return None
    prev_pos = prev.part_of_speech()
    m_pos = m.part_of_speech()
    # A stranded honorific prefix attaches to whatever it modifies — noun, verb stem
    # (お呼び), or na-adjective — never to punctuation or another prefix.
    if (
        prev_pos[0] == "接頭辞"
        and prev.surface() in _HONORIFIC_PREFIXES
        and m_pos[0] not in ("補助記号", "空白", "接頭辞")
    ):
        return m
    # Counters after a numeral. Sudachi spells counters three ways — 接尾辞/助数詞
    # (つ, 本), 接尾辞/一般 (人, 冊), and 普通名詞/助数詞可能 (週間, 泊) — so key on
    # the numeral and accept any of them.
    if prev_pos[:2] == ("名詞", "数詞") and (
        m_pos[:2] == ("接尾辞", "名詞的") or m_pos[2] in ("助数詞", "助数詞可能")
    ):
        return m
    if m_pos[0] != "接尾辞":
        return None
    if m.surface() == "ら" and prev_pos[0] in ("名詞", "代名詞"):
        return prev
    if m.surface() in _TITLE_SUFFIXES and prev_pos[0] == "名詞" and prev_pos[1] == "普通名詞":
        return prev
    return None


def japanese_records(sentences):
    """Analyze Japanese with Sudachi, yielding the same record shape spaCy produces.

    Whitespace is reconstructed from character offsets rather than taken from Sudachi's own
    空白 tokens: the pipeline requires that concatenating text + whitespace reproduces the
    sentence exactly, and a whitespace *token* would break that invariant downstream.
    """
    from sudachipy import Dictionary, SplitMode

    mode = {"A": SplitMode.A, "B": SplitMode.B, "C": SplitMode.C}[SUDACHI_SPLIT_MODE]
    tokenizer = Dictionary().create()

    for sentence in tqdm(sentences, desc="Processing (sudachi)", unit="sent"):
        morphemes = [x for m in tokenizer.tokenize(sentence, mode)
                     for x in _detach_bound_suffix(m)
                     if sentence[x.begin():x.end()].strip()]
        morphemes = _fuse_presplit_pieces(morphemes)
        doc = []
        for i, m in enumerate(morphemes):
            begin, end = m.begin(), m.end()
            # everything up to the next token is this token's trailing whitespace
            nxt = morphemes[i + 1].begin() if i + 1 < len(morphemes) else len(sentence)
            prev = sentence[morphemes[i - 1].begin():morphemes[i - 1].end()] if i else ""
            text = sentence[begin:end]
            # normalized_form unifies orthographic variants — やっぱり/やはり/やっぱ all
            # give 矢張り, わかった/分かった give 分かる. dictionary_form does not.
            lemma = m.normalized_form()
            pos = m.part_of_speech()
            # Na-adjective lemmas include だ, paralleling verb dictionary forms
            # (静か → 静かだ). Only true 形状詞 — the 助動詞語幹 subclass (よう, みたい)
            # is grammar, not vocabulary, and keeps its bare lemma.
            if pos[0] == "形状詞" and pos[1] != "助動詞語幹":
                lemma += "だ"
            upos = _sudachi_upos(pos, text, prev)
            # The adverbial に retagged to ADP keeps Sudachi's copula lemma だ; the gold
            # convention (182:2 in a sampled regen) folds it into the case-particle key.
            if text == "に" and upos == "ADP":
                lemma = "に"
            doc.append({
                "text": text,
                "whitespace": sentence[end:nxt],
                "lemma": lemma,
                "pos": upos,
                "morph": {},
            })
        # leading whitespace has nowhere to attach; prepend it to the first token's text
        if doc and morphemes and morphemes[0].begin() > 0:
            doc[0]["text"] = sentence[:morphemes[0].end()]
        yield {
            "sentence": sentence,
            "multiword_terms": {"high_confidence": [], "low_confidence": []},
            "doc": doc,
            "entities": [],
        }


# ---------------------------------------------------------------------------------------
# Chinese: HanLP, not spaCy
#
# Same reasoning as the Sudachi section above: the LLM reviews a deterministic proposal,
# and the better the proposal, the cheaper and more consistent the review. spaCy's zh
# models segment with pkuseg (statistical, 2019, news-domain, ~95 F1); HanLP's ELECTRA
# models are neural, trained on CTB9 (which includes broadcast conversation — much closer
# to our subtitle corpus), and score ~98 F1. The multi-task base model gives fine-grained
# tokenization and CTB POS in one pass, on GPU when available.
#
# Fine-grained (CTB standard) over coarse is deliberate — the mirror image of Japanese
# choosing mode C. Japanese merges because splitting strands bound morphemes that are not
# words; Chinese fine-grained splits because coarse merges strand nothing: 都是 → 都|是 and
# 很快 → 很|快 leave two words a learner can look up, and neither compound is a dictionary
# entry. Where fine keeps a compound (没有, 放下, 为什么), it is because the compound IS the
# dictionary word.
#
# The tokenizer preserves surface text exactly (fullwidth chars, punctuation), so token
# offsets can be recovered by scanning — verified against the whitespace-reconstruction
# invariant the pipeline requires.
HANLP_ZH_MODEL = "CLOSE_TOK_POS_NER_SRL_DEP_SDP_CON_ELECTRA_BASE_ZH"

# CTB → UPOS. Nearly mechanical, like the Sudachi table: the residual ambiguities (modal
# VV vs AUX, 是...的 focus 是) are context rules applied downstream by ChineseCorrector
# and the cleaning LLM, not guessed here.
_CTB_UPOS = {
    "AD": "ADV",     # adverb (也, 不, 很)
    "AS": "PART",    # aspect particle (了, 着, 过)
    "BA": "ADP",     # 把 disposal marker
    "CC": "CCONJ",   # coordinating conjunction (和, 或者)
    "CD": "NUM",     # cardinal number
    "CS": "SCONJ",   # subordinating conjunction (如果, 虽然)
    "DEC": "PART",   # 的 relativizer/nominalizer
    "DEG": "PART",   # 的 genitive
    "DER": "PART",   # 得 complement marker
    "DEV": "PART",   # 地 adverbializer
    "DT": "DET",     # determiner (这, 那, 每)
    "EM": "SYM",     # emoticon
    "ETC": "PART",   # 等/等等
    "FW": "X",       # foreign word
    "IC": "X",       # incomplete word
    "IJ": "INTJ",    # interjection
    "JJ": "ADJ",     # attributive adjective
    "LB": "ADP",     # 被 long passive
    "LC": "ADP",     # localizer (上, 里, 中)
    "M": "NOUN",     # measure word — the pipeline's policy tags classifiers NOUN
    "MSP": "PART",   # 所
    "NN": "NOUN",
    "NOI": "X",      # noise
    "NR": "PROPN",   # proper noun
    "NT": "NOUN",    # temporal noun (今天, 明年)
    "OD": "NUM",     # ordinal (第一)
    "ON": "ADV",     # onomatopoeia (adverbial in CTB)
    "P": "ADP",      # preposition
    "PN": "PRON",
    "PU": "PUNCT",
    "SB": "ADP",     # 被 short passive
    "SP": "PART",    # sentence-final particle (吗, 呢, 吧)
    "URL": "SYM",
    "VA": "ADJ",     # predicative adjective (快 in 很快)
    "VC": "VERB",    # copula (是, 为)
    "VE": "VERB",    # existential 有
    "VV": "VERB",
    "X": "X",
}


def chinese_records(sentences):
    """Analyze Simplified Chinese with HanLP, yielding the spaCy record shape.

    Chinese words do not inflect, so the lemma is always the surface form; there is no
    normalization layer to apply. Whitespace between tokens is reconstructed from a
    surface scan, keeping the text+whitespace == sentence invariant.
    """
    import hanlp

    mtl = hanlp.load(getattr(hanlp.pretrained.mtl, HANLP_ZH_MODEL))

    batch_size = 128
    for i in tqdm(range(0, len(sentences), batch_size), desc="Processing (hanlp)", unit="batch"):
        chunk = sentences[i : i + batch_size]
        out = mtl(chunk, tasks=["tok/fine", "pos/ctb"])
        for sentence, words, tags in zip(chunk, out["tok/fine"], out["pos/ctb"]):
            spans = []
            pos_cursor = 0
            for word in words:
                found = sentence.find(word, pos_cursor)
                if found < 0:
                    break
                spans.append((found, found + len(word)))
                pos_cursor = found + len(word)
            if len(spans) != len(words) or not words:
                # The tokenizer altered the surface somewhere (never observed, but the
                # invariant is load-bearing). Emit the sentence as one X token — the
                # cleaning LLM is allowed to retokenize it from scratch.
                print(f"WARNING: hanlp surface mismatch, emitting unsegmented: {sentence!r}")
                doc = [{
                    "text": sentence,
                    "whitespace": "",
                    "lemma": sentence,
                    "pos": "X",
                    "morph": {},
                }]
            else:
                doc = []
                for j, (word, tag) in enumerate(zip(words, tags)):
                    end = spans[j][1]
                    nxt = spans[j + 1][0] if j + 1 < len(spans) else len(sentence)
                    doc.append({
                        "text": word,
                        "whitespace": sentence[end:nxt],
                        "lemma": word,
                        "pos": _CTB_UPOS.get(tag, "X"),
                        "morph": {},
                    })
                # leading whitespace has nowhere to attach; prepend it to the first
                # token's text (the lemma stays the clean word)
                if spans[0][0] > 0:
                    doc[0]["text"] = sentence[: spans[0][1]]
            yield {
                "sentence": sentence,
                "multiword_terms": {"high_confidence": [], "low_confidence": []},
                "doc": doc,
                "entities": [],
            }


# UPOS tags the Rust side accepts; anything else (a tagger glitch) becomes X.
_UPOS_VALID = {
    "ADJ", "ADP", "ADV", "AUX", "CCONJ", "DET", "INTJ", "NOUN", "NUM",
    "PART", "PRON", "PROPN", "PUNCT", "SCONJ", "SYM", "VERB", "X",
}


def thai_records(sentences):
    """Analyze Thai with PyThaiNLP: newmm segmentation + perceptron POS on the
    TUD treebank (native UPOS).

    newmm is dictionary-based, so every token it emits is a dictionary word by
    construction; its failure mode is unknown words (transliterated names)
    shredding into single-character fragments, which the cleaning LLM merges
    back per the style guide. Thai words don't inflect, so the lemma is always
    the surface form. Thai's real spaces (phrase separators, not word breaks)
    are reconstructed into the whitespace field by the surface scan.
    """
    from pythainlp.tag import pos_tag
    from pythainlp.tokenize import word_tokenize

    for sentence in tqdm(sentences, desc="Processing (pythainlp)", unit="sentence"):
        words = [w for w in word_tokenize(sentence, engine="newmm") if w.strip()]
        spans = []
        pos_cursor = 0
        for word in words:
            found = sentence.find(word, pos_cursor)
            if found < 0:
                break
            spans.append((found, found + len(word)))
            pos_cursor = found + len(word)
        if len(spans) != len(words) or not words:
            # The tokenizer altered the surface somewhere (never observed, but the
            # invariant is load-bearing). Emit the sentence as one X token — the
            # cleaning LLM is allowed to retokenize it from scratch.
            print(f"WARNING: pythainlp surface mismatch, emitting unsegmented: {sentence!r}")
            doc = [{
                "text": sentence,
                "whitespace": "",
                "lemma": sentence,
                "pos": "X",
                "morph": {},
            }]
        else:
            doc = []
            tags = pos_tag(words, engine="perceptron", corpus="tud")
            for j, (word, (_, tag)) in enumerate(zip(words, tags)):
                end = spans[j][1]
                nxt = spans[j + 1][0] if j + 1 < len(spans) else len(sentence)
                doc.append({
                    "text": word,
                    "whitespace": sentence[end:nxt],
                    "lemma": word,
                    "pos": tag if tag in _UPOS_VALID else "X",
                    "morph": {},
                })
            # leading whitespace has nowhere to attach; prepend it to the first
            # token's text (the lemma stays the clean word)
            if spans[0][0] > 0:
                doc[0]["text"] = sentence[: spans[0][1]]
        yield {
            "sentence": sentence,
            "multiword_terms": {"high_confidence": [], "low_confidence": []},
            "doc": doc,
            "entities": [],
        }


def load_model(language_code: str):
    import spacy

    models = MODEL_MAPPING.get(language_code)
    if not models:
        raise ValueError(f"Unsupported language code: {language_code}")
    model_name = models["large"] if use_big_model else models["small"]
    nlp = spacy.load(model_name)
    print(f"Pipeline components: {nlp.pipe_names}")
    return nlp


def process_sentences(sentences_file: str, output_file: str, language_code: str):
    """Parse sentences from JSONL and write analyzed records."""
    with open(sentences_file, "r", encoding="utf-8") as f:
        sentences = [json.loads(line) for line in f if line.strip()]
    print(f"Found {len(sentences)} sentences to process")

    analyzers = {"jpn": japanese_records, "zho-hans": chinese_records, "tha": thai_records}
    if language_code in analyzers:
        with open(output_file, "w", encoding="utf-8") as outfile:
            for record in analyzers[language_code](sentences):
                outfile.write(json.dumps(record, ensure_ascii=False) + "\n")
        print(f"\nProcessing complete! Output written to {output_file}")
        return

    nlp = load_model(language_code)

    batch_size = 1000
    with open(output_file, "w", encoding="utf-8") as outfile:
        for i in tqdm(range(0, len(sentences), batch_size), desc="Processing", unit="batch"):
            batch = sentences[i : i + batch_size]
            for sentence, doc in zip(batch, nlp.pipe(batch, batch_size=100)):
                record = {
                    "sentence": sentence,
                    # Always empty — see module docstring.
                    "multiword_terms": {"high_confidence": [], "low_confidence": []},
                    "doc": [
                        {
                            "text": token.text,
                            "whitespace": token.whitespace_,
                            "lemma": token.lemma_,
                            "pos": token.pos_,
                            "morph": token.morph.to_dict(),
                            "dep": token.dep_,
                            "head": token.head.i,
                        }
                        for token in doc
                    ],
                    "entities": [(ent.text, ent.label_) for ent in doc.ents],
                }
                outfile.write(json.dumps(record, ensure_ascii=False) + "\n")

    print(f"\nProcessing complete! Output written to {output_file}")


def main():
    if len(sys.argv) != 4:
        print("Usage: python main.py <language_code> <sentences.jsonl> <output.jsonl>")
        print("Language code should be ISO 639-3 (e.g., 'fra' for French, 'spa' for Spanish)")
        sys.exit(1)

    language_code = sys.argv[1]
    sentences_file = sys.argv[2]
    output_file = sys.argv[3]

    process_sentences(sentences_file, output_file, language_code)


if __name__ == "__main__":
    main()
