use crate::{ReviewDefinition, TranscribeComprehensibleSentence};
use language_utils::transcription_challenge::{Part, PartGraded, PartSubmitted, WordGrade};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TranscriptionInput {
    pub index: usize,
    pub text: String,
}

#[derive(serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct TranscriptionSubmission {
    pub request: Vec<PartSubmitted>,
    pub all_blanks_filled: bool,
}

// Match the previous JavaScript trim operation, including BOM whitespace and
// excluding the Unicode NEXT LINE character (which Rust's trim would remove).
fn trim_submission(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{0085}'))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn prepare_transcription_submission(
    parts: Vec<Part>,
    inputs: Vec<TranscriptionInput>,
) -> TranscriptionSubmission {
    let inputs: BTreeMap<_, _> = inputs.into_iter().map(|i| (i.index, i.text)).collect();
    let mut all_blanks_filled = true;
    let request = parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| match part {
            Part::Provided { part } => PartSubmitted::Provided { part },
            Part::AskedToTranscribe { parts } => {
                let submission =
                    trim_submission(inputs.get(&i).map_or("", String::as_str)).to_string();
                all_blanks_filled &= !submission.is_empty();
                PartSubmitted::AskedToTranscribe { parts, submission }
            }
        })
        .collect();
    TranscriptionSubmission {
        request,
        all_blanks_filled,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn transcription_is_perfect(results: Vec<PartGraded>) -> bool {
    results.iter().all(|result| match result {
        PartGraded::Provided { .. } => true,
        PartGraded::AskedToTranscribe { parts, .. } => parts
            .iter()
            .all(|p| matches!(p.grade, WordGrade::Perfect { .. })),
    })
}

fn wrong_gram_groups(results: &[PartGraded], indices: &[Vec<usize>]) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    let mut groups = vec![];
    for (part_index, result) in results.iter().enumerate() {
        if let PartGraded::AskedToTranscribe { parts, .. } = result {
            for (word_index, part) in parts.iter().enumerate() {
                if matches!(
                    part.grade,
                    WordGrade::Perfect { .. } | WordGrade::CorrectWithTypo { .. }
                ) {
                    continue;
                }
                if let Some(&group) = indices.get(part_index).and_then(|p| p.get(word_index))
                    && seen.insert(group)
                {
                    groups.push(group);
                }
            }
        }
    }
    groups
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_transcription_review_definitions(
    challenge: TranscribeComprehensibleSentence,
    results: Vec<PartGraded>,
) -> Vec<ReviewDefinition> {
    wrong_gram_groups(&results, &challenge.part_gram_indices)
        .into_iter()
        .filter_map(|group| {
            challenge
                .gram_definitions_for_lookup
                .get(group)?
                .as_ref()
                .map(|definition| ReviewDefinition {
                    definition: definition.clone(),
                    breakdown: challenge
                        .gram_breakdowns_for_lookup
                        .get(group)
                        .cloned()
                        .flatten(),
                })
        })
        .collect()
}

/// Operates on an owned snapshot: changing a grade must not mutate earlier
/// React state objects or a saved draft through a shared nested reference.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn apply_transcription_grade(
    mut results: Vec<PartGraded>,
    part_index: usize,
    word_index: usize,
    grade: WordGrade,
) -> Vec<PartGraded> {
    if let Some(PartGraded::AskedToTranscribe { parts, .. }) = results.get_mut(part_index)
        && let Some(part) = parts.get_mut(word_index)
    {
        part.grade = grade;
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use language_utils::{Literal, transcription_challenge::PartGradedPart};
    fn literal() -> Literal<String> {
        serde_json::from_value(serde_json::json!({"word":{"text":"chat","word_type":{"type":"Heteronym","word":"chat","lemma":"chat","pos":"NOUN"}},"whitespace":""})).unwrap()
    }
    fn result(grades: Vec<WordGrade>) -> Vec<PartGraded> {
        vec![PartGraded::AskedToTranscribe {
            parts: grades
                .into_iter()
                .map(|grade| PartGradedPart {
                    heard: literal(),
                    grade,
                })
                .collect(),
            submission: "answer".into(),
        }]
    }
    #[test]
    fn only_requested_parts_need_answers_and_whitespace_matches_web() {
        let parts = vec![
            Part::Provided { part: literal() },
            Part::AskedToTranscribe {
                parts: vec![literal()],
            },
        ];
        assert!(!prepare_transcription_submission(parts.clone(), vec![]).all_blanks_filled);
        let input = prepare_transcription_submission(
            parts,
            vec![TranscriptionInput {
                index: 1,
                text: "\u{feff} chat \u{a0}".into(),
            }],
        );
        assert!(input.all_blanks_filled);
        assert!(
            matches!(&input.request[1], PartSubmitted::AskedToTranscribe { submission, .. } if submission == "chat")
        );
        assert_eq!(
            trim_submission("\u{0085}chat\u{0085}"),
            "\u{0085}chat\u{0085}"
        );
    }
    #[test]
    fn typo_is_not_perfect_but_does_not_request_a_definition() {
        let grades = result(vec![
            WordGrade::CorrectWithTypo {
                wrote: Some("caht".into()),
            },
            WordGrade::Incorrect { wrote: None },
            WordGrade::Missed {},
        ]);
        assert!(!transcription_is_perfect(grades.clone()));
        assert_eq!(wrong_gram_groups(&grades, &[vec![0, 2, 2]]), vec![2]);
        assert_eq!(wrong_gram_groups(&grades, &[vec![0, 2, 1]]), vec![2, 1]);
    }
    #[test]
    fn corrections_leave_prior_snapshot_unchanged_and_ignore_invalid_indices() {
        let before = result(vec![WordGrade::Incorrect {
            wrote: Some("chien".into()),
        }]);
        let after =
            apply_transcription_grade(before.clone(), 0, 0, WordGrade::Perfect { wrote: None });
        assert!(!transcription_is_perfect(before.clone()));
        assert!(transcription_is_perfect(after));
        assert_eq!(
            apply_transcription_grade(before.clone(), 2, 0, WordGrade::Missed {}),
            before
        );
    }
}
