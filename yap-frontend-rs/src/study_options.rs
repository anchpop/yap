//! Decisions for the idle review screen, independent of its layout.
use wasm_bindgen::prelude::*;

pub(crate) fn recommend_more_cards(
    added: u32,
    average: f64,
    upcoming: u32,
    max_per_day: u32,
    available: usize,
) -> bool {
    added < 20
        && (f64::from(upcoming) < average * 21.0 || max_per_day < 10)
        && max_per_day <= 50
        && available > 0
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn next_progress_milestone(current: f64, projected: f64) -> Option<f64> {
    let after = (projected / 5.0).floor() * 5.0;
    (after > (current / 5.0).floor() * 5.0).then_some(after)
}

#[derive(Clone, Debug, serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct IdleStudyState {
    pub no_schedulable_cards: bool,
    pub nothing_to_do: bool,
    pub has_never_studied: bool,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_idle_study_state(
    has_future_card: bool,
    cards_added: usize,
    smart_add_count: u32,
) -> IdleStudyState {
    let nothing_to_do = !has_future_card && smart_add_count == 0;
    IdleStudyState {
        no_schedulable_cards: !has_future_card,
        nothing_to_do,
        has_never_studied: cards_added == 0 && !nothing_to_do,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workload_limits_and_recent_activity() {
        assert!(recommend_more_cards(19, 1.0, 20, 10, 1));
        assert!(!recommend_more_cards(20, 1.0, 20, 10, 1));
        assert!(!recommend_more_cards(0, 1.0, 21, 10, 1));
        assert!(recommend_more_cards(0, 0.0, 21, 9, 1));
        assert!(recommend_more_cards(0, 10.0, 100, 50, 1));
        assert!(!recommend_more_cards(0, 10.0, 100, 51, 1));
        assert!(!recommend_more_cards(0, 10.0, 100, 50, 0));
    }
    #[test]
    fn milestone_requires_crossing_a_five_percent_boundary() {
        assert_eq!(next_progress_milestone(4.9, 5.0), Some(5.0));
        assert_eq!(next_progress_milestone(5.0, 9.9), None);
        assert_eq!(next_progress_milestone(5.0, 15.1), Some(15.0));
        assert_eq!(next_progress_milestone(99.9, 100.0), Some(100.0));
    }
    #[test]
    fn empty_and_exhausted_decks_are_distinct() {
        assert!(get_idle_study_state(false, 0, 1).has_never_studied);
        let exhausted = get_idle_study_state(false, 0, 0);
        assert!(exhausted.nothing_to_do);
        assert!(!exhausted.has_never_studied);
        assert!(!get_idle_study_state(true, 5, 0).nothing_to_do);
    }
}
