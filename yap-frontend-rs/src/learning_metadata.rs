use crate::deck_selection::DailyReviewTarget;
use language_utils::Language;
use wasm_bindgen::prelude::*;

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "lowercase")]
pub enum CourseMaturity {
    Stable,
    Beta,
    Alpha,
}

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct LanguageLearningMetadata {
    pub iso_code: String,
    pub iso6391: String,
    pub status: CourseMaturity,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_language_learning_metadata(language: Language) -> LanguageLearningMetadata {
    use Language::*;
    LanguageLearningMetadata {
        iso_code: language.code().into(),
        iso6391: language.iso_639_1().into(),
        status: match language {
            English | French | Spanish | German => CourseMaturity::Stable,
            Italian | Portuguese => CourseMaturity::Beta,
            Korean | Japanese | Russian | ChineseSimplified | ChineseTraditional | Hindi | Thai => {
                CourseMaturity::Alpha
            }
        },
    }
}

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct DailyGoalOption {
    pub value: DailyReviewTarget,
    pub minutes: u32,
    /// Existing onboarding estimate, not a prediction from the scheduler.
    pub estimated_first_week_words: u32,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_daily_goal_options() -> Vec<DailyGoalOption> {
    use DailyReviewTarget::*;
    [Casual, Regular, Serious, Intense]
        .into_iter()
        .map(|value| {
            let minutes = value.target_seconds() / 60;
            DailyGoalOption {
                value,
                minutes,
                estimated_first_week_words: minutes * 5,
            }
        })
        .collect()
}
