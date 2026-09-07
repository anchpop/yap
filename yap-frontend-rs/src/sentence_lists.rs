use crate::{Deck, SentenceListSelection};

#[bridgerton::bridge(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SentenceListCategory {
    Essential,
    Movie,
    Pimsleur,
}

#[bridgerton::bridge(transparent)]
#[derive(serde::Serialize)]
pub struct SentenceListNavigation {
    pub categories: Vec<SentenceListCategory>,
    pub selection: Option<SentenceListSelection>,
    pub selected_index: usize,
}

#[bridgerton::bridge]
pub fn get_sentence_list_navigation(
    selection: Option<SentenceListSelection>,
    has_movies: bool,
    has_pimsleur: bool,
) -> SentenceListNavigation {
    use SentenceListCategory::*;
    let mut categories = vec![Essential];
    if has_movies {
        categories.push(Movie);
    }
    if has_pimsleur {
        categories.push(Pimsleur);
    }
    let category = match &selection {
        None => Essential,
        Some(SentenceListSelection::Movie { .. }) => Movie,
        Some(SentenceListSelection::PimsleurLesson { .. }) => Pimsleur,
    };
    let selected_index = categories.iter().position(|c| *c == category);
    SentenceListNavigation {
        categories,
        selection: selected_index.and(selection),
        selected_index: selected_index.unwrap_or(0),
    }
}

#[bridgerton::bridge(transparent)]
#[derive(serde::Serialize)]
pub struct SentenceListProgress {
    pub percent_known: f64,
    pub all_available_learned: bool,
}

#[bridgerton::bridge]
impl Deck {
    /// Called only when switching categories, so choosing the best movie stays lazy.
    pub fn get_sentence_list_for_category(
        &self,
        category: SentenceListCategory,
        fallback_movie_id: Option<String>,
    ) -> Option<SentenceListSelection> {
        match category {
            SentenceListCategory::Essential => None,
            SentenceListCategory::Movie => self
                .get_best_movie_sentence_list()
                .or_else(|| fallback_movie_id.map(|id| SentenceListSelection::Movie { id })),
            SentenceListCategory::Pimsleur => self.get_best_pimsleur_sentence_list(),
        }
    }
    /// Essential progress refers to the tier chosen by smart-add, which may
    /// differ from the first incomplete tier when the proposed cards finish it.
    pub fn get_sentence_list_progress(
        &self,
        selection: Option<SentenceListSelection>,
        essential_percent_known: f64,
    ) -> SentenceListProgress {
        let source = match &selection {
            None => {
                return SentenceListProgress {
                    percent_known: essential_percent_known,
                    all_available_learned: false,
                };
            }
            Some(SentenceListSelection::Movie { id }) => {
                language_utils::FrequencySourceId::Movie(id.clone())
            }
            Some(SentenceListSelection::PimsleurLesson { level, lesson }) => {
                language_utils::FrequencySourceId::PimsleurLesson(language_utils::PimsleurLesson {
                    level: *level,
                    lesson: *lesson,
                })
            }
        };
        let available = self
            .context
            .language_pack
            .source_gram_frequencies
            .get(&source)
            .is_some_and(|f| !f.entries.is_empty() && f.total_count > 0);
        if !available {
            return SentenceListProgress {
                percent_known: 0.0,
                all_available_learned: false,
            };
        }
        let score = self.sentence_list_percent_known(&selection);
        SentenceListProgress {
            percent_known: score.percent_known,
            all_available_learned: score.all_available_learned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_category_falls_back_without_losing_available_selection() {
        let selection = Some(SentenceListSelection::Movie { id: "film".into() });
        let fallback = get_sentence_list_navigation(selection.clone(), false, true);
        assert_eq!(
            fallback.categories,
            [
                SentenceListCategory::Essential,
                SentenceListCategory::Pimsleur
            ]
        );
        assert!(fallback.selection.is_none());
        assert_eq!(fallback.selected_index, 0);
        let available = get_sentence_list_navigation(selection.clone(), true, false);
        assert_eq!(available.selection, selection);
        assert_eq!(available.selected_index, 1);
        assert_eq!(
            get_sentence_list_navigation(
                Some(SentenceListSelection::PimsleurLesson {
                    level: 1,
                    lesson: 1
                }),
                false,
                true
            )
            .selected_index,
            1
        );
    }
}
