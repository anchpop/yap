//! In-memory placement workflow. Completing it still appends the existing event.
use crate::{Deck, placement_test::PlacementTestWord};
use std::collections::BTreeSet;
use wasm_bindgen::prelude::*;

const ROUNDS: u8 = 3;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PlacementSession {
    pub round: u8,
    pub words: Vec<PlacementTestWord>,
    pub selected_words: BTreeSet<String>,
    pub known_words: Vec<String>,
    pub unknown_words: Vec<String>,
}

impl Default for PlacementSession {
    fn default() -> Self {
        Self {
            round: 1,
            words: vec![],
            selected_words: BTreeSet::new(),
            known_words: vec![],
            unknown_words: vec![],
        }
    }
}

impl PlacementSession {
    fn finished(&self) -> bool {
        self.round > ROUNDS
    }
    fn refresh(&mut self, words: Vec<PlacementTestWord>) -> bool {
        if self.finished() {
            return false;
        }
        if words.is_empty() {
            self.round = ROUNDS + 1;
            return true;
        }
        if self
            .words
            .iter()
            .map(|w| &w.word)
            .eq(words.iter().map(|w| &w.word))
        {
            return false;
        }
        self.words = words;
        self.selected_words.clear();
        true
    }
    fn submit(&mut self) {
        if self.finished() || self.words.is_empty() {
            return;
        }
        for word in &self.words {
            if self.selected_words.contains(&word.word) {
                self.known_words.push(word.word.clone());
            } else {
                self.unknown_words.push(word.word.clone());
            }
        }
        self.round += 1;
        self.selected_words.clear();
    }
}

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct PlacementSessionInfo {
    pub finished: bool,
    pub total_rounds: u8,
    pub progress_percent: f64,
    pub can_restart: bool,
    pub too_advanced: bool,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_placement_session_info(session: PlacementSession) -> PlacementSessionInfo {
    let answers = session.known_words.len() + session.unknown_words.len();
    PlacementSessionInfo {
        finished: session.finished(),
        total_rounds: ROUNDS,
        progress_percent: f64::from(session.round) / f64::from(ROUNDS + 1) * 100.0,
        can_restart: session.round > 1 && !session.finished(),
        too_advanced: answers > 0 && session.known_words.len() as f64 / answers as f64 > 0.85,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn toggle_placement_word(mut session: PlacementSession, word: String) -> PlacementSession {
    if !session.finished()
        && session.words.iter().any(|w| w.word == word)
        && !session.selected_words.remove(&word)
    {
        session.selected_words.insert(word);
    }
    session
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    pub fn start_placement_session(&self) -> PlacementSession {
        let mut session = PlacementSession::default();
        session.refresh(self.get_placement_test(vec![], vec![]));
        session
    }
    /// None means that a rebuilt deck produced the same round: preserve the
    /// host's state object and selected words during background pack loading.
    pub fn refresh_placement_session(
        &self,
        mut session: PlacementSession,
    ) -> Option<PlacementSession> {
        if session.finished() {
            return None;
        }
        let words =
            self.get_placement_test(session.known_words.clone(), session.unknown_words.clone());
        session.refresh(words).then_some(session)
    }
    pub fn advance_placement_session(&self, mut session: PlacementSession) -> PlacementSession {
        session.submit();
        self.refresh_placement_session(session.clone())
            .unwrap_or(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn words(names: &[&str]) -> Vec<PlacementTestWord> {
        names
            .iter()
            .map(|w| PlacementTestWord {
                word: (*w).into(),
                definition: (*w).into(),
            })
            .collect()
    }
    #[test]
    fn rebuilt_deck_preserves_selection_but_new_round_resets_it() {
        let mut s = PlacementSession::default();
        s.refresh(words(&["a", "b"]));
        s = toggle_placement_word(s, "a".into());
        assert!(!s.refresh(words(&["a", "b"])));
        assert!(s.selected_words.contains("a"));
        s.submit();
        assert_eq!(s.known_words, ["a"]);
        assert_eq!(s.unknown_words, ["b"]);
        assert!(s.selected_words.is_empty());
        assert_eq!(s.round, 2);
        assert!(get_placement_session_info(s.clone()).can_restart);
        s.refresh(words(&["c"]));
        s.submit();
        s.refresh(words(&["d"]));
        s.submit();
        assert!(s.finished());
        let count = s.unknown_words.len();
        s.submit();
        assert_eq!(s.unknown_words.len(), count);
        assert_eq!(get_placement_session_info(s).progress_percent, 100.0);
    }
    #[test]
    fn no_words_finishes_early_and_empty_results_are_not_advanced() {
        let mut s = PlacementSession::default();
        s.refresh(vec![]);
        let info = get_placement_session_info(s);
        assert!(info.finished);
        assert!(!info.too_advanced);
        let restarted = PlacementSession::default();
        assert_eq!(restarted.round, 1);
        assert!(restarted.known_words.is_empty());
    }
    #[test]
    fn advanced_cutoff_is_strict() {
        for (known, expected) in [(17, false), (18, true)] {
            let s = PlacementSession {
                known_words: vec!["known".into(); known],
                unknown_words: vec!["unknown".into(); 20 - known],
                ..PlacementSession::default()
            };
            assert_eq!(get_placement_session_info(s).too_advanced, expected);
        }
    }
}
