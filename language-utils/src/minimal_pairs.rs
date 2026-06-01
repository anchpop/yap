//! Minimal-pair detection and indexing.
//!
//! A *minimal pair* is two words whose IPA pronunciation differs by exactly
//! one phoneme at exactly one position.
//!
//! Two stages live here:
//!
//! * [`find_minimal_pairs`] runs the wildcard-pattern detection algorithm
//!   over a list of word/IPA strings and produces a `Vec<MinimalPairGroup>`
//!   in the rich pre-interning form. `generate-data` calls this once,
//!   writes the JSONL for inspection, and stores the same `Vec` on
//!   `ConsolidatedLanguageData`.
//! * [`build_minimal_pairs_index`] takes those pre-computed groups and the
//!   pack's `RodeoReader` and produces the interned [`MinimalPairs`]
//!   stored on the `LanguagePack` — plus the inverse `word → 1-off words`
//!   map derived from the same data.
//!
//! ## Algorithm
//!
//! For each word with phonemes `[p_0, ..., p_{n-1}]`, generate `n` bucket
//! keys by replacing each position in turn with a wildcard. Two words share
//! a bucket key iff they differ only at that one position. Each unordered
//! pair of words can collide in at most one bucket, so no cross-bucket
//! dedup is needed. Homophones (same IPA, different spelling) end up in
//! the same buckets with identical phonemes at the wildcard position and
//! are filtered out.

use lasso::{RodeoReader, Spur};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single minimal pair: two words whose IPA differs by exactly one
/// phoneme. `position` is the 0-based phoneme index at which they differ.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MinimalPair {
    pub word_a: String,
    pub ipa_a: String,
    pub word_b: String,
    pub ipa_b: String,
    pub position: usize,
}

/// All minimal pairs distinguished by a particular pair of phonemes.
/// `phonemes` is sorted alphabetically so `(a, b)` and `(b, a)` collapse to
/// one entry, and within each `MinimalPair`, `word_a` is the word whose IPA
/// contains `phonemes[0]`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MinimalPairGroup {
    pub phonemes: [String; 2],
    pub pairs: Vec<MinimalPair>,
}

/// Find all minimal pairs across `word_to_pronunciation` and group them by
/// the unordered pair of distinguishing tokens.
///
/// Every whitespace-separated IPA token counts as a distinguishing unit,
/// including suprasegmentals like `.` (syllable boundary) and stress-marked
/// tokens like `ˈa`. Stress and syllable contrasts are pedagogically useful
/// to drill, so we deliberately do not strip them.
///
/// Groups are sorted by pair count descending. Pairs within a group are
/// sorted by `max(freq_a, freq_b)` descending — the most useful pairs for a
/// learner appear first.
pub fn find_minimal_pairs(
    word_to_pronunciation: &[(String, String)],
    word_frequency: &HashMap<&str, u32>,
) -> Vec<MinimalPairGroup> {
    // Tokenize once. IPA tokens are whitespace-separated; one phoneme may be
    // multi-codepoint (e.g. "sʲ").
    let entries: Vec<(String, String, Vec<String>)> = word_to_pronunciation
        .iter()
        .filter_map(|(word, ipa)| {
            let phonemes: Vec<String> = ipa.split_whitespace().map(str::to_string).collect();
            (!phonemes.is_empty()).then(|| (word.clone(), ipa.clone(), phonemes))
        })
        .collect();

    // Bucket key = (position, prefix, suffix). Different total lengths can't
    // collide because (prefix, suffix) won't match. Bucket value = (entry
    // index, phoneme at wildcard position) — the phoneme is what
    // distinguishes one occupant from another.
    type BucketKey = (usize, Vec<String>, Vec<String>);
    let mut buckets: HashMap<BucketKey, Vec<(usize, String)>> = HashMap::new();
    for (idx, (_, _, phs)) in entries.iter().enumerate() {
        for pos in 0..phs.len() {
            buckets
                .entry((pos, phs[..pos].to_vec(), phs[pos + 1..].to_vec()))
                .or_default()
                .push((idx, phs[pos].clone()));
        }
    }

    let mut pair_groups: HashMap<(String, String), Vec<MinimalPair>> = HashMap::new();
    for ((pos, _, _), bucket) in &buckets {
        if bucket.len() < 2 {
            continue;
        }
        for i in 0..bucket.len() {
            for j in (i + 1)..bucket.len() {
                let (idx_a, ph_a) = (&bucket[i].0, &bucket[i].1);
                let (idx_b, ph_b) = (&bucket[j].0, &bucket[j].1);
                if ph_a == ph_b {
                    // Same phoneme at the wildcard position → identical IPA;
                    // these are homophones, not a minimal pair.
                    continue;
                }
                let (word_a, ipa_a, _) = &entries[*idx_a];
                let (word_b, ipa_b, _) = &entries[*idx_b];
                if word_a == word_b {
                    continue;
                }
                // word_a always carries the alphabetically lower phoneme,
                // matching `phonemes[0]` in the group header.
                let (lo_ph, hi_ph, w_a, i_a, w_b, i_b) = if ph_a < ph_b {
                    (ph_a.clone(), ph_b.clone(), word_a, ipa_a, word_b, ipa_b)
                } else {
                    (ph_b.clone(), ph_a.clone(), word_b, ipa_b, word_a, ipa_a)
                };
                pair_groups
                    .entry((lo_ph, hi_ph))
                    .or_default()
                    .push(MinimalPair {
                        word_a: w_a.clone(),
                        ipa_a: i_a.clone(),
                        word_b: w_b.clone(),
                        ipa_b: i_b.clone(),
                        position: *pos,
                    });
            }
        }
    }

    let pair_freq = |pair: &MinimalPair| -> u32 {
        let fa = word_frequency
            .get(pair.word_a.as_str())
            .copied()
            .unwrap_or(0);
        let fb = word_frequency
            .get(pair.word_b.as_str())
            .copied()
            .unwrap_or(0);
        fa.max(fb)
    };

    let mut groups: Vec<MinimalPairGroup> = pair_groups
        .into_iter()
        .map(|((lo, hi), mut pairs)| {
            pairs.sort_by(|a, b| {
                pair_freq(b)
                    .cmp(&pair_freq(a))
                    .then_with(|| a.word_a.cmp(&b.word_a))
                    .then_with(|| a.word_b.cmp(&b.word_b))
            });
            MinimalPairGroup {
                phonemes: [lo, hi],
                pairs,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.pairs
            .len()
            .cmp(&a.pairs.len())
            .then_with(|| a.phonemes.cmp(&b.phonemes))
    });

    groups
}

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MinimalPairs {
    /// Key is the unordered phoneme pair, canonicalized so the resolved
    /// string of `key.0` sorts before `key.1`. Within each `(word_a, word_b)`
    /// value, `word_a` carries `key.0` at the distinguishing position and
    /// `word_b` carries `key.1`.
    pub by_phoneme_pair: FxHashMap<(Spur, Spur), Vec<(Spur, Spur)>>,
    /// For each word that's part of any minimal pair, the list of words one
    /// phoneme substitution away. Sorted by `word_frequency` desc (most
    /// common standalone-usage neighbors first), with Spur identity as a
    /// tiebreaker for a stable archive.
    pub by_word: FxHashMap<Spur, Vec<Spur>>,
}

/// Translate pre-computed `MinimalPairGroup`s into the interned, archive
/// form. The `rodeo` must already contain every word and every phoneme
/// referenced by the groups (handled by `ConsolidatedLanguageData::intern`).
///
/// `word_frequency` is used to sort each `by_word` neighbor list so the most
/// common standalone-usage neighbors appear first. A word missing from the
/// map sorts to the end (treated as frequency 0).
pub fn build_minimal_pairs_index(
    groups: &[MinimalPairGroup],
    rodeo: &RodeoReader,
    word_frequency: &FxHashMap<Spur, u32>,
) -> MinimalPairs {
    let mut by_phoneme_pair: FxHashMap<(Spur, Spur), Vec<(Spur, Spur)>> = FxHashMap::default();
    let mut by_word: FxHashMap<Spur, Vec<Spur>> = FxHashMap::default();

    for group in groups {
        let [lo_ph_s, hi_ph_s] = &group.phonemes;
        let lo_ph = rodeo
            .get(lo_ph_s)
            .unwrap_or_else(|| panic!("phoneme {lo_ph_s:?} not interned"));
        let hi_ph = rodeo
            .get(hi_ph_s)
            .unwrap_or_else(|| panic!("phoneme {hi_ph_s:?} not interned"));
        let pair_key = (lo_ph, hi_ph);

        let pair_list = by_phoneme_pair.entry(pair_key).or_default();
        pair_list.reserve(group.pairs.len());

        for pair in &group.pairs {
            let word_a = rodeo
                .get(&pair.word_a)
                .unwrap_or_else(|| panic!("word {:?} not interned", pair.word_a));
            let word_b = rodeo
                .get(&pair.word_b)
                .unwrap_or_else(|| panic!("word {:?} not interned", pair.word_b));
            pair_list.push((word_a, word_b));
            by_word.entry(word_a).or_default().push(word_b);
            by_word.entry(word_b).or_default().push(word_a);
        }
    }

    // Dedup + sort each neighbor list by descending standalone-usage frequency
    // (tiebreak by Spur ID for stable archive output). Dedup first because
    // sort_by is unstable across duplicate keys; with one IPA per word a
    // duplicate shouldn't occur, but dedup defensively in case that invariant
    // changes.
    for neighbors in by_word.values_mut() {
        neighbors.sort_by_key(|s| s.into_inner());
        neighbors.dedup();
        neighbors.sort_by(|a, b| {
            let fa = word_frequency.get(a).copied().unwrap_or(0);
            let fb = word_frequency.get(b).copied().unwrap_or(0);
            fb.cmp(&fa)
                .then_with(|| a.into_inner().cmp(&b.into_inner()))
        });
    }

    MinimalPairs {
        by_phoneme_pair,
        by_word,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lasso::Rodeo;

    fn freqs<'a>(pairs: &[(&'a str, u32)]) -> HashMap<&'a str, u32> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn finds_simple_minimal_pair() {
        let data = vec![
            ("ship".to_string(), "ʃ ɪ p".to_string()),
            ("sip".to_string(), "s ɪ p".to_string()),
        ];
        let groups = find_minimal_pairs(&data, &freqs(&[("ship", 10), ("sip", 5)]));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].phonemes, ["s".to_string(), "ʃ".to_string()]);
        assert_eq!(groups[0].pairs.len(), 1);
        assert_eq!(groups[0].pairs[0].position, 0);
    }

    #[test]
    fn word_a_carries_first_phoneme() {
        // word_a must be the word whose IPA contains phonemes[0], regardless
        // of input order.
        for data in [
            vec![
                ("ship".to_string(), "ʃ ɪ p".to_string()),
                ("sip".to_string(), "s ɪ p".to_string()),
            ],
            vec![
                ("sip".to_string(), "s ɪ p".to_string()),
                ("ship".to_string(), "ʃ ɪ p".to_string()),
            ],
        ] {
            let groups = find_minimal_pairs(&data, &HashMap::new());
            let g = &groups[0];
            assert_eq!(g.phonemes, ["s".to_string(), "ʃ".to_string()]);
            assert_eq!(g.pairs[0].word_a, "sip");
            assert_eq!(g.pairs[0].word_b, "ship");
        }
    }

    #[test]
    fn skips_homophones() {
        let data = vec![
            ("their".to_string(), "ð ɛ r".to_string()),
            ("there".to_string(), "ð ɛ r".to_string()),
        ];
        let groups = find_minimal_pairs(&data, &HashMap::new());
        assert!(groups.is_empty());
    }

    #[test]
    fn groups_by_distinguishing_phoneme() {
        let data = vec![
            ("bat".to_string(), "b a t".to_string()),
            ("pat".to_string(), "p a t".to_string()),
            ("bit".to_string(), "b ɪ t".to_string()),
            ("pit".to_string(), "p ɪ t".to_string()),
        ];
        let f = freqs(&[("bat", 1), ("pat", 1), ("bit", 1), ("pit", 1)]);
        let groups = find_minimal_pairs(&data, &f);
        let bp = groups
            .iter()
            .find(|g| g.phonemes == ["b".to_string(), "p".to_string()])
            .expect("expected a b/p group");
        assert_eq!(bp.pairs.len(), 2);
        let ai = groups
            .iter()
            .find(|g| g.phonemes == ["a".to_string(), "ɪ".to_string()])
            .expect("expected an a/ɪ group");
        assert_eq!(ai.pairs.len(), 2);
    }

    #[test]
    fn ignores_different_length_words() {
        let data = vec![
            ("a".to_string(), "a".to_string()),
            ("ab".to_string(), "a b".to_string()),
        ];
        let groups = find_minimal_pairs(&data, &HashMap::new());
        assert!(groups.is_empty());
    }

    #[test]
    fn index_interns_and_builds_inverse() {
        let data = vec![
            ("ship".to_string(), "ʃ ɪ p".to_string()),
            ("sip".to_string(), "s ɪ p".to_string()),
        ];
        let groups = find_minimal_pairs(&data, &HashMap::new());

        // Build a rodeo containing words + IPA + phonemes, like
        // ConsolidatedLanguageData::intern does.
        let mut rodeo = Rodeo::new();
        for (word, ipa) in &data {
            rodeo.get_or_intern(word);
            rodeo.get_or_intern(ipa);
            for phoneme in ipa.split_whitespace() {
                rodeo.get_or_intern(phoneme);
            }
        }
        let rodeo = rodeo.into_reader();

        let idx = build_minimal_pairs_index(&groups, &rodeo, &FxHashMap::default());

        let s_phoneme = rodeo.get("s").unwrap();
        let sh_phoneme = rodeo.get("ʃ").unwrap();
        let sip = rodeo.get("sip").unwrap();
        let ship = rodeo.get("ship").unwrap();

        assert_eq!(
            idx.by_phoneme_pair.get(&(s_phoneme, sh_phoneme)).unwrap(),
            &vec![(sip, ship)]
        );
        assert_eq!(idx.by_word.get(&sip).unwrap(), &vec![ship]);
        assert_eq!(idx.by_word.get(&ship).unwrap(), &vec![sip]);
    }

    #[test]
    fn by_word_sorted_by_frequency_desc() {
        // "cat" minimal-pairs with "bat" (b/k), "cot" (a/o), "can" (n/t).
        // Their frequencies: bat=100, cot=5, can=50 → expect [bat, can, cot].
        let data = vec![
            ("cat".to_string(), "k a t".to_string()),
            ("bat".to_string(), "b a t".to_string()),
            ("cot".to_string(), "k o t".to_string()),
            ("can".to_string(), "k a n".to_string()),
        ];
        let groups = find_minimal_pairs(&data, &HashMap::new());

        let mut rodeo = Rodeo::new();
        for (word, ipa) in &data {
            rodeo.get_or_intern(word);
            rodeo.get_or_intern(ipa);
            for phoneme in ipa.split_whitespace() {
                rodeo.get_or_intern(phoneme);
            }
        }
        let rodeo = rodeo.into_reader();

        let bat = rodeo.get("bat").unwrap();
        let cat = rodeo.get("cat").unwrap();
        let cot = rodeo.get("cot").unwrap();
        let can = rodeo.get("can").unwrap();
        let mut freq = FxHashMap::default();
        freq.insert(bat, 100);
        freq.insert(cot, 5);
        freq.insert(can, 50);

        let idx = build_minimal_pairs_index(&groups, &rodeo, &freq);
        assert_eq!(idx.by_word.get(&cat).unwrap(), &vec![bat, can, cot]);
    }
}
