//! Product disclosure rules shared by web and native frontends.
//! Hosts supply account/connectivity state; rendering and persistence stay with them.

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FlashcardDisclosure {
    pub require_answer_reveal: bool,
    pub show_tutorial: bool,
}

/// `total_card_count` is the number of due + future cards, not lifetime reviews.
#[bridgerton::bridge]
pub fn get_flashcard_disclosure(
    total_card_count: usize,
    times_type_seen: u32,
) -> FlashcardDisclosure {
    FlashcardDisclosure {
        require_answer_reveal: total_card_count < 50 || times_type_seen < 10,
        show_tutorial: should_show_challenge_tutorial(times_type_seen),
    }
}

#[bridgerton::bridge]
pub fn should_show_challenge_tutorial(times_type_seen: u32) -> bool {
    times_type_seen < 2
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ReviewPromptContext {
    /// No due challenges and no current challenge.
    pub is_idle: bool,
    pub is_online: bool,
    pub is_signed_in: bool,
    /// The host has loaded a profile with no display name.
    pub needs_display_name: bool,
    pub display_name_dismissed: bool,
    pub has_access_token: bool,
}

#[bridgerton::bridge(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ReviewPrompts {
    pub offer_display_name: bool,
    /// Eligibility for the idle screen's engagement prompts. The host still
    /// checks installation, notification support, and prior dismissals.
    pub offer_engagement: bool,
}

#[bridgerton::bridge]
pub fn get_review_prompts(
    total_reviews_completed: u64,
    total_card_count: usize,
    context: ReviewPromptContext,
) -> ReviewPrompts {
    ReviewPrompts {
        offer_display_name: context.is_idle
            && total_reviews_completed >= 25
            && context.needs_display_name
            && context.is_online
            && !context.display_name_dismissed
            && context.has_access_token,
        offer_engagement: context.is_idle
            && total_card_count > 5
            && context.is_online
            && context.is_signed_in,
    }
}

pub(crate) fn should_offer_placement_test(
    starting_fresh: Option<bool>,
    has_taken_test: bool,
    cards_added: usize,
) -> bool {
    starting_fresh == Some(false) && !has_taken_test && cards_added < 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_reveal_requires_both_experience_thresholds() {
        for (cards, seen, required) in [
            (49, 10, true),
            (50, 9, true),
            (50, 10, false),
            (51, 11, false),
        ] {
            assert_eq!(
                get_flashcard_disclosure(cards, seen).require_answer_reveal,
                required
            );
        }
        for (seen, show) in [(0, true), (1, true), (2, false), (3, false)] {
            assert_eq!(should_show_challenge_tutorial(seen), show);
            assert_eq!(get_flashcard_disclosure(50, seen).show_tutorial, show);
        }
    }

    fn eligible_context() -> ReviewPromptContext {
        ReviewPromptContext {
            is_idle: true,
            is_online: true,
            is_signed_in: true,
            needs_display_name: true,
            display_name_dismissed: false,
            has_access_token: true,
        }
    }

    #[test]
    fn prompts_use_distinct_review_and_card_counts() {
        assert_eq!(
            get_review_prompts(24, 6, eligible_context()),
            ReviewPrompts {
                offer_display_name: false,
                offer_engagement: true
            }
        );
        assert_eq!(
            get_review_prompts(25, 5, eligible_context()),
            ReviewPrompts {
                offer_display_name: true,
                offer_engagement: false
            }
        );
        assert_eq!(
            get_review_prompts(25, 6, eligible_context()),
            ReviewPrompts {
                offer_display_name: true,
                offer_engagement: true
            }
        );
    }

    #[test]
    fn prompts_respect_host_state() {
        for context in [
            ReviewPromptContext {
                is_idle: false,
                ..eligible_context()
            },
            ReviewPromptContext {
                is_online: false,
                ..eligible_context()
            },
        ] {
            assert_eq!(
                get_review_prompts(25, 6, context),
                ReviewPrompts {
                    offer_display_name: false,
                    offer_engagement: false,
                }
            );
        }
        for context in [
            ReviewPromptContext {
                needs_display_name: false,
                ..eligible_context()
            },
            ReviewPromptContext {
                display_name_dismissed: true,
                ..eligible_context()
            },
            ReviewPromptContext {
                has_access_token: false,
                ..eligible_context()
            },
        ] {
            assert_eq!(
                get_review_prompts(25, 6, context),
                ReviewPrompts {
                    offer_display_name: false,
                    offer_engagement: true,
                }
            );
        }
        let signed_out = ReviewPromptContext {
            is_signed_in: false,
            needs_display_name: false,
            has_access_token: false,
            ..eligible_context()
        };
        assert_eq!(
            get_review_prompts(25, 6, signed_out),
            ReviewPrompts {
                offer_display_name: false,
                offer_engagement: false,
            }
        );
    }

    #[test]
    fn placement_is_only_offered_to_existing_learners_before_three_added_cards() {
        for (preference, taken, cards, expected) in [
            (None, false, 0, false),
            (Some(true), false, 0, false),
            (Some(false), false, 0, true),
            (Some(false), false, 2, true),
            (Some(false), false, 3, false),
            (Some(false), false, 4, false),
            (Some(false), true, 0, false),
        ] {
            assert_eq!(
                should_offer_placement_test(preference, taken, cards),
                expected
            );
        }
    }
}
