//! Translation review decisions. Browser request cancellation and draft storage
//! remain host adapters; this module never writes or modifies deck events.
use crate::TranslateComprehensibleSentence;
use language_utils::{
    Gram, GramDefinition, Heteronym, Language, Literal,
    autograde::{AutoGradeTranslationResponse, Remembered},
};
use std::collections::BTreeSet;
use wasm_bindgen::prelude::*;

// Preserve the field names already stored in pending translation drafts.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ManualTranslationGrade {
    pub literal_grades: Vec<Option<Remembered>>,
    pub phrases_remembered: Vec<Gram<String>>,
    pub phrases_forgot: Vec<Gram<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encouragement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autograding_error: Option<String>,
}

#[derive(serde::Serialize, tsify::Tsify)]
#[serde(tag = "type")]
#[tsify(into_wasm_abi)]
pub enum TranslationReviewResult {
    Perfect {
        encouragement: Option<String>,
        explanation: Option<String>,
    },
    Manual {
        grade: ManualTranslationGrade,
    },
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn prepare_translation_review(
    literals: Vec<Literal<String>>,
    response: AutoGradeTranslationResponse,
) -> TranslationReviewResult {
    if crate::translation_is_perfect(literals, response.clone()) {
        TranslationReviewResult::Perfect {
            encouragement: response.encouragement,
            explanation: response.explanation,
        }
    } else {
        TranslationReviewResult::Manual {
            grade: ManualTranslationGrade {
                literal_grades: response.literal_grades,
                phrases_remembered: response.phrases_remembered,
                phrases_forgot: response.phrases_forgot,
                encouragement: response.encouragement,
                explanation: response.explanation,
                autograding_error: response.autograding_error,
            },
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn failed_translation_review(literal_count: usize, error: String) -> ManualTranslationGrade {
    ManualTranslationGrade {
        literal_grades: vec![None; literal_count],
        phrases_remembered: vec![],
        phrases_forgot: vec![],
        encouragement: None,
        explanation: None,
        autograding_error: Some(error),
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum TranslationGradeItem {
    Literal {
        #[serde(rename = "literalIndex")]
        literal_index: usize,
        display: String,
        status: Option<bool>,
    },
    Phrase {
        gram: Gram<String>,
        display: String,
        status: Option<bool>,
    },
}
impl TranslationGradeItem {
    fn has_grade(&self) -> bool {
        match self {
            Self::Literal { status, .. } | Self::Phrase { status, .. } => status.is_some(),
        }
    }
}

fn grade_items(
    literals: &[Literal<String>],
    phrases: &[Gram<String>],
    grade: &ManualTranslationGrade,
    language: Language,
) -> Vec<TranslationGradeItem> {
    let mut items = vec![];
    let failed = grade
        .autograding_error
        .as_ref()
        .is_some_and(|e| !e.is_empty());
    for (i, literal) in literals.iter().enumerate() {
        if literal.word.heteronym().is_none() {
            continue;
        }
        let status = grade
            .literal_grades
            .get(i)
            .and_then(|g| g.as_ref())
            .map(|g| *g == Remembered::Remembered);
        if status.is_none() && !failed {
            continue;
        }
        items.push(TranslationGradeItem::Literal {
            literal_index: i,
            display: literal.word.text.clone(),
            status,
        });
    }
    for gram in phrases {
        let status = if grade.phrases_remembered.contains(gram) {
            Some(true)
        } else if grade.phrases_forgot.contains(gram) {
            Some(false)
        } else {
            None
        };
        items.push(TranslationGradeItem::Phrase {
            gram: gram.clone(),
            display: gram.to_display_string(language),
            status,
        });
    }
    items
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn apply_translation_grade(
    mut grade: ManualTranslationGrade,
    item: TranslationGradeItem,
    remembered: bool,
    literal_count: usize,
) -> ManualTranslationGrade {
    match item {
        TranslationGradeItem::Phrase { gram, .. } => {
            grade.phrases_remembered.retain(|g| *g != gram);
            grade.phrases_forgot.retain(|g| *g != gram);
            if remembered {
                grade.phrases_remembered.push(gram);
            } else {
                grade.phrases_forgot.push(gram);
            }
        }
        TranslationGradeItem::Literal { literal_index, .. } => {
            if literal_index < literal_count {
                if grade.literal_grades.len() <= literal_index {
                    grade.literal_grades.resize(literal_index + 1, None);
                }
                grade.literal_grades[literal_index] = Some(if remembered {
                    Remembered::Remembered
                } else {
                    Remembered::Forgot
                });
            }
        }
    }
    grade
}

#[derive(Clone, Debug, serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct ReviewDefinition {
    pub definition: GramDefinition,
    #[allow(clippy::type_complexity)]
    pub breakdown: Option<Vec<(String, Option<String>, Option<String>)>>,
}

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct TranslationReviewFeedback {
    pub grade_items: Vec<TranslationGradeItem>,
    pub can_continue: bool,
    pub definitions: Vec<ReviewDefinition>,
    pub tapped_gram_groups: Vec<usize>,
    pub heteronyms_tapped: Vec<Heteronym<String>>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_translation_review_feedback(
    sentence: TranslateComprehensibleSentence,
    grade: Option<ManualTranslationGrade>,
    is_perfect: bool,
    tapped_words: Vec<usize>,
    language: Language,
) -> TranslationReviewFeedback {
    let items = grade
        .as_ref()
        .map(|g| {
            grade_items(
                &sentence.target_language_literals,
                &sentence.unique_target_language_phrases,
                g,
                language,
            )
        })
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut tapped_gram_groups = vec![];
    let mut heteronyms_tapped = vec![];
    for index in tapped_words {
        if let Some(heteronym) = sentence
            .target_language_literals
            .get(index)
            .and_then(|l| l.word.heteronym())
        {
            heteronyms_tapped.push(heteronym.clone());
        }
        if let Some(&group) = sentence.literal_gram_indices.get(index)
            && seen.insert(group)
        {
            tapped_gram_groups.push(group);
        }
    }
    let mut groups = tapped_gram_groups.clone();
    let mut definitions = vec![];
    if let Some(grade) = &grade {
        for (i, value) in grade.literal_grades.iter().enumerate() {
            if value == &Some(Remembered::Forgot)
                && let Some(&group) = sentence.literal_gram_indices.get(i)
                && seen.insert(group)
            {
                groups.push(group);
            }
        }
    }
    for group in groups {
        if let Some(Some(definition)) = sentence.gram_definitions_for_lookup.get(group) {
            definitions.push(ReviewDefinition {
                definition: definition.clone(),
                breakdown: sentence
                    .gram_breakdowns_for_lookup
                    .get(group)
                    .cloned()
                    .flatten(),
            });
        }
    }
    if let Some(grade) = &grade {
        for phrase in &grade.phrases_forgot {
            if let Some(i) = sentence
                .unique_target_language_phrases
                .iter()
                .position(|g| g == phrase)
                && let Some(Some(definition)) = sentence.phrase_definitions.get(i)
            {
                definitions.push(ReviewDefinition {
                    definition: definition.clone(),
                    breakdown: sentence.phrase_breakdowns.get(i).cloned().flatten(),
                });
            }
        }
    }
    TranslationReviewFeedback {
        can_continue: is_perfect || items.iter().any(TranslationGradeItem::has_grade),
        grade_items: items,
        definitions,
        tapped_gram_groups,
        heteronyms_tapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn literal() -> Literal<String> {
        serde_json::from_value(serde_json::json!({"word":{"text":"chat","word_type":{"type":"Heteronym","word":"chat","lemma":"chat","pos":"NOUN"}},"whitespace":""})).unwrap()
    }
    #[test]
    fn failure_requires_manual_input_and_real_ungradable_words_stay_hidden() {
        let mut grade = failed_translation_review(1, "offline".into());
        let items = grade_items(&[literal()], &[], &grade, Language::French);
        assert_eq!(items.len(), 1);
        assert!(!items.iter().any(TranslationGradeItem::has_grade));
        grade = apply_translation_grade(grade, items[0].clone(), false, 1);
        assert_eq!(grade.literal_grades, [Some(Remembered::Forgot)]);
        assert!(
            grade_items(&[literal()], &[], &grade, Language::French)
                .iter()
                .any(TranslationGradeItem::has_grade)
        );
        grade.literal_grades[0] = None;
        grade.autograding_error = None;
        assert!(grade_items(&[literal()], &[], &grade, Language::French).is_empty());
    }
    #[test]
    fn corrections_are_idempotent_and_support_missing_literal_grades() {
        let phrase = Gram::new(vec![language_utils::Atom::Tok(literal().word)]);
        let item = TranslationGradeItem::Phrase {
            gram: phrase.clone(),
            display: "chat".into(),
            status: None,
        };
        let mut grade = failed_translation_review(0, "offline".into());
        grade = apply_translation_grade(grade, item.clone(), true, 1);
        grade = apply_translation_grade(grade, item.clone(), true, 1);
        assert_eq!(grade.phrases_remembered.len(), 1);
        grade = apply_translation_grade(grade, item, false, 1);
        assert!(grade.phrases_remembered.is_empty());
        assert_eq!(grade.phrases_forgot, [phrase]);
        grade = apply_translation_grade(
            grade,
            TranslationGradeItem::Literal {
                literal_index: 0,
                display: "chat".into(),
                status: None,
            },
            true,
            1,
        );
        assert_eq!(grade.literal_grades, [Some(Remembered::Remembered)]);
        grade = apply_translation_grade(
            grade,
            TranslationGradeItem::Literal {
                literal_index: usize::MAX,
                display: "invalid".into(),
                status: None,
            },
            false,
            1,
        );
        assert_eq!(grade.literal_grades.len(), 1);
    }

    #[test]
    fn manual_grade_round_trips_existing_draft_shape() {
        let old = serde_json::json!({"literalGrades":[null,"Forgot"],"phrasesRemembered":[],"phrasesForgot":[],"autogradingError":"offline"});
        let grade: ManualTranslationGrade = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(grade).unwrap(), old);
    }
    #[test]
    fn heuristic_success_is_never_promoted_to_perfect() {
        let mut response = AutoGradeTranslationResponse {
            literal_grades: vec![Some(Remembered::Remembered)],
            phrases_remembered: vec![],
            phrases_forgot: vec![],
            encouragement: None,
            explanation: None,
            autograding_error: Some("offline".into()),
        };
        assert!(matches!(
            prepare_translation_review(vec![literal()], response.clone()),
            TranslationReviewResult::Manual { .. }
        ));
        response.autograding_error = None;
        assert!(matches!(
            prepare_translation_review(vec![literal()], response),
            TranslationReviewResult::Perfect { .. }
        ));
    }
}
