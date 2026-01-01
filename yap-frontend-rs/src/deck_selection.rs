use std::collections::BTreeMap;

use language_utils::Language;
use weapon::data_model::Event;
#[derive(Clone, Debug, tsify::Tsify, serde::Serialize, serde::Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DeckSelection {
    pub target_language: Option<Language>,
    pub native_language: Option<Language>,
    pub starting_fresh: Option<bool>,
}

pub struct DeckSelectionPartial {
    pub target_language: Option<Language>,
    pub native_language: Option<Language>,
    pub starting_fresh: BTreeMap<Language, bool>,
}

impl weapon::PartialAppState for DeckSelection {
    type Event = DeckSelectionEvent;
    type Partial = DeckSelectionPartial;

    fn process_event(
        mut partial: Self::Partial,
        event: &weapon::data_model::Timestamped<Self::Event>,
    ) -> Self::Partial {
        match event.event {
            DeckSelectionEvent::SelectTargetLanguage(language) => {
                partial.target_language = Some(language);
                partial
            }
            DeckSelectionEvent::SelectBothLanguages { native, target } => {
                partial.native_language = Some(native);
                partial.target_language = Some(target);
                partial
            }
            DeckSelectionEvent::SetStartingFresh {
                starting_fresh,
                target_language,
            } => {
                partial
                    .starting_fresh
                    .insert(target_language, starting_fresh);
                partial
            }
        }
    }

    fn finalize(partial: Self::Partial) -> Self {
        DeckSelection {
            starting_fresh: partial
                .target_language
                .as_ref()
                .and_then(|target_language| partial.starting_fresh.get(target_language))
                .cloned(),
            target_language: partial.target_language,
            native_language: partial.native_language,
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
    V1(DeckSelectionEvent),
}
impl Event for DeckSelectionEvent {
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let versioned = VersionedDeckSelectionEvent::from(self.clone());
        serde_json::to_value(versioned)
    }

    fn from_json(json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value::<VersionedDeckSelectionEvent>(json.clone())
            .map(|versioned| versioned.into())
    }
}
impl From<DeckSelectionEvent> for VersionedDeckSelectionEvent {
    fn from(event: DeckSelectionEvent) -> Self {
        VersionedDeckSelectionEvent::V1(event)
    }
}
impl From<VersionedDeckSelectionEvent> for DeckSelectionEvent {
    fn from(event: VersionedDeckSelectionEvent) -> Self {
        match event {
            VersionedDeckSelectionEvent::V1(event) => event,
        }
    }
}
