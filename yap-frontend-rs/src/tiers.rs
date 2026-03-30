use std::collections::BTreeSet;

use language_utils::SpurGram;
use serde::{Deserialize, Serialize};

/// Tier definitions for the frequency list. Each tier covers a range of the most frequent grams.
/// The first element is the tier name, the second is how many new words this tier adds.
const TIERS: &[(&str, usize)] = &[
    ("Elementary", 200),
    ("Core", 300),
    ("Basic", 400),
    ("Essential", 500),
    ("Functional", 600),
    ("Intermediate", 700),
    ("Proficient", 800),
    ("Advanced", 900),
    ("Expert", 1000),
    ("Specialized", 1100),
    ("Esoteric", usize::MAX), // all remaining
];

pub(crate) const WORDS_PER_LEVEL: usize = 31;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TierInfo {
    /// 1-indexed tier number
    pub tier: u32,
    /// Display name for this tier
    pub name: String,
    /// 1-indexed level within this tier
    pub level: u32,
    /// Total number of levels in this tier
    pub total_levels: u32,
    /// Percent known within the current level (0-100)
    pub percent_known: f64,
}

pub(crate) struct TierLevelSlice<'a> {
    tier_idx: usize,
    tier_name: &'static str,
    level_idx: usize,
    total_levels_in_tier: usize,
    grams: &'a [SpurGram],
    total_freq: u64,
}

impl TierLevelSlice<'_> {
    pub(crate) fn known_pct(
        &self,
        freq_list: &language_utils::language_pack::FrequencyList,
        written: &BTreeSet<SpurGram>,
        listening: &BTreeSet<SpurGram>,
    ) -> f64 {
        if self.total_freq == 0 {
            return 100.0;
        }
        let known: f64 = self
            .grams
            .iter()
            .map(|gram| {
                let count = freq_list
                    .entries
                    .get(gram)
                    .map(|f| f.count as f64)
                    .unwrap_or(0.0);
                match (written.contains(gram), listening.contains(gram)) {
                    (true, true) => count,
                    (true, false) | (false, true) => count * 0.5,
                    (false, false) => 0.0,
                }
            })
            .sum();
        known / self.total_freq as f64 * 100.0
    }

    pub(crate) fn to_tier_info(&self, percent_known: f64) -> TierInfo {
        TierInfo {
            tier: (self.tier_idx + 1) as u32,
            name: self.tier_name.to_string(),
            level: (self.level_idx + 1) as u32,
            total_levels: self.total_levels_in_tier as u32,
            percent_known,
        }
    }
}

/// Build all tier level slices with pre-summed total frequencies.
pub(crate) fn tier_level_slices<'a>(
    all_grams: &'a [SpurGram],
    freq_list: &language_utils::language_pack::FrequencyList,
) -> Vec<TierLevelSlice<'a>> {
    let mut levels = Vec::new();
    let mut offset = 0;
    for (tier_idx, &(name, size)) in TIERS.iter().enumerate() {
        let end = if size == usize::MAX {
            all_grams.len()
        } else {
            (offset + size).min(all_grams.len())
        };
        if offset >= all_grams.len() {
            break;
        }
        let tier_grams = &all_grams[offset..end];
        let total_levels = tier_grams.len().div_ceil(WORDS_PER_LEVEL);
        for level_idx in 0..total_levels {
            let start = level_idx * WORDS_PER_LEVEL;
            let level_end = ((level_idx + 1) * WORDS_PER_LEVEL).min(tier_grams.len());
            let level_grams = &tier_grams[start..level_end];
            let total_freq: u64 = level_grams
                .iter()
                .filter_map(|g| freq_list.entries.get(g))
                .map(|f| f.count as u64)
                .sum();
            levels.push(TierLevelSlice {
                tier_idx,
                tier_name: name,
                level_idx,
                total_levels_in_tier: total_levels,
                grams: level_grams,
                total_freq,
            });
        }
        offset = end;
    }
    levels
}

/// Find the tier level where projected grams show the most improvement over current grams.
/// Falls back to the first incomplete level if no improvement anywhere.
pub(crate) fn best_tier_level<'a>(
    levels: &'a [TierLevelSlice<'_>],
    freq_list: &language_utils::language_pack::FrequencyList,
    current_written: &BTreeSet<SpurGram>,
    current_listening: &BTreeSet<SpurGram>,
    projected_written: &BTreeSet<SpurGram>,
    projected_listening: &BTreeSet<SpurGram>,
) -> &'a TierLevelSlice<'a> {
    // Find the earliest level where adding cards makes any progress
    let idx = levels
        .iter()
        .position(|level| {
            let current_pct = level.known_pct(freq_list, current_written, current_listening);
            let projected_pct =
                level.known_pct(freq_list, projected_written, projected_listening);
            projected_pct > current_pct
        })
        // Fall back to first incomplete level
        .unwrap_or_else(|| {
            levels
                .iter()
                .position(|level| {
                    level.known_pct(freq_list, current_written, current_listening) < 100.0
                })
                .unwrap_or(levels.len().saturating_sub(1))
        });

    &levels[idx]
}

/// Compute the percent known within the first tier level that isn't at 100%.
/// Used for the "essential" (no goal) path when we only have one set of grams.
pub(crate) fn first_incomplete_level_pct(
    frequency_list: &language_utils::language_pack::FrequencyList,
    known_written: &BTreeSet<SpurGram>,
    known_listening: &BTreeSet<SpurGram>,
) -> f64 {
    let all_grams: Vec<SpurGram> = frequency_list.entries.keys().copied().collect();
    let levels = tier_level_slices(&all_grams, frequency_list);
    for level in &levels {
        let pct = level.known_pct(frequency_list, known_written, known_listening);
        if pct < 100.0 {
            return pct;
        }
    }
    100.0
}
