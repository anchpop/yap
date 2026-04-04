use std::collections::{BTreeMap, BTreeSet};

use language_utils::Language;
use weapon::data_model::Event;

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum HeardAbout {
    FriendsOrFamily,
    Reddit,
    TikTok,
    GoogleSearch,
    YouTube,
    TwitterX,
    Other,
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Motivation {
    SpendTimeProductively,
    SupportMyEducation,
    ConnectWithPeople,
    BoostMyCareer,
    PrepareForTravel,
    JustForFun,
    Other,
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum ExperienceLevel {
    New,
    CommonWords,
    BasicConversations,
    VariousTopics,
    MostTopics,
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum DailyReviewTarget {
    Casual,
    Regular,
    Serious,
    Intense,
}

impl DailyReviewTarget {
    /// Target time in seconds for each daily review goal.
    /// Casual=5min, Regular=10min, Serious=15min, Intense=20min
    pub fn target_seconds(&self) -> u32 {
        match self {
            DailyReviewTarget::Casual => 5 * 60,
            DailyReviewTarget::Regular => 10 * 60,
            DailyReviewTarget::Serious => 15 * 60,
            DailyReviewTarget::Intense => 20 * 60,
        }
    }
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSelections {
    pub starting_fresh: bool,
    pub motivation: Option<Motivation>,
    pub experience_level: Option<ExperienceLevel>,
    pub study_goal: Option<DailyReviewTarget>,
}

#[derive(Clone, Debug, tsify::Tsify, serde::Serialize, serde::Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DeckSelection {
    pub target_language: Option<Language>,
    pub native_language: Option<Language>,
    pub onboarding_selections: Option<OnboardingSelections>,
    pub heard_about: Option<HeardAbout>,
    pub onboarded_languages: Vec<Language>,
}

impl DeckSelection {
    pub fn starting_fresh(&self) -> Option<bool> {
        self.onboarding_selections
            .as_ref()
            .map(|s| s.starting_fresh)
    }
}

pub struct DeckSelectionPartial {
    pub target_language: Option<Language>,
    pub native_language: Option<Language>,
    pub onboarding_selections: BTreeMap<Language, OnboardingSelections>,
    pub selected_languages: BTreeSet<Language>,
    pub heard_about: Option<HeardAbout>,
}

impl weapon::AppState for DeckSelection {
    type Event = DeckSelectionEvent;
    type Partial = DeckSelectionPartial;

    fn process_event(
        mut partial: Self::Partial,
        _context: &<Self::Event as weapon::data_model::Event>::Context,
        event: &weapon::data_model::Timestamped<Self::Event>,
    ) -> Self::Partial {
        match &event.event {
            DeckSelectionEvent::SelectTargetLanguage(language) => {
                partial.target_language = Some(*language);
                partial
            }
            DeckSelectionEvent::SelectBothLanguages { native, target } => {
                partial.native_language = Some(*native);
                partial.target_language = Some(*target);
                partial.selected_languages.insert(*target);
                partial
            }
            DeckSelectionEvent::SetOnboardingSelections {
                selections,
                target_language,
            } => {
                partial
                    .onboarding_selections
                    .insert(*target_language, selections.clone());
                partial
            }
            DeckSelectionEvent::SetHeardAbout { heard_about } => {
                partial.heard_about = Some(heard_about.clone());
                partial
            }
        }
    }

    fn finalize(
        partial: Self::Partial,
        _context: &<Self::Event as weapon::data_model::Event>::Context,
    ) -> Self {
        DeckSelection {
            onboarding_selections: partial
                .target_language
                .as_ref()
                .and_then(|target_language| partial.onboarding_selections.get(target_language))
                .cloned(),
            onboarded_languages: partial.selected_languages.into_iter().collect(),
            target_language: partial.target_language,
            native_language: partial.native_language,
            heard_about: partial.heard_about,
        }
    }
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum DeckSelectionEvent {
    #[serde(alias = "SelectLanguage")]
    SelectTargetLanguage(Language),
    SelectBothLanguages {
        native: Language,
        target: Language,
    },
    SetOnboardingSelections {
        selections: OnboardingSelections,
        target_language: Language,
    },
    SetHeardAbout {
        heard_about: HeardAbout,
    },
}

// V1 event type for backwards compatibility with serialized events
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub enum DeckSelectionEventV1 {
    #[serde(alias = "SelectLanguage")]
    SelectTargetLanguage(Language),
    SelectBothLanguages {
        native: Language,
        target: Language,
    },
    SetStartingFresh {
        starting_fresh: bool,
        target_language: Language,
    },
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, Ord, PartialOrd, Eq, PartialEq, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "version")]
pub enum VersionedDeckSelectionEvent {
    V1(DeckSelectionEventV1),
    V2(DeckSelectionEvent),
}

impl Event for DeckSelectionEvent {
    type Versioned = VersionedDeckSelectionEvent;
    type Context = ();

    fn to_versioned(&self) -> Self::Versioned {
        VersionedDeckSelectionEvent::V2(self.clone())
    }

    fn from_versioned(versioned: &Self::Versioned, _context: &Self::Context) -> Option<Self> {
        Some(match versioned {
            VersionedDeckSelectionEvent::V1(v1) => match v1 {
                DeckSelectionEventV1::SelectTargetLanguage(lang) => {
                    DeckSelectionEvent::SelectTargetLanguage(*lang)
                }
                DeckSelectionEventV1::SelectBothLanguages { native, target } => {
                    DeckSelectionEvent::SelectBothLanguages {
                        native: *native,
                        target: *target,
                    }
                }
                DeckSelectionEventV1::SetStartingFresh {
                    starting_fresh,
                    target_language,
                } => DeckSelectionEvent::SetOnboardingSelections {
                    selections: OnboardingSelections {
                        starting_fresh: *starting_fresh,
                        ..Default::default()
                    },
                    target_language: *target_language,
                },
            },
            VersionedDeckSelectionEvent::V2(event) => event.clone(),
        })
    }
}

impl From<DeckSelectionEvent> for VersionedDeckSelectionEvent {
    fn from(event: DeckSelectionEvent) -> Self {
        VersionedDeckSelectionEvent::V2(event)
    }
}
