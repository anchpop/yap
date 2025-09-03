use language_utils::Language;
use weapon::data_model::Event;

#[derive(Clone, Debug, tsify::Tsify, serde::Serialize, serde::Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum DeckSelection {
    Selected(Language),
    NoneSelected,
}

impl weapon::AppState for DeckSelection {
    type Event = DeckSelectionEvent;

    fn apply_event(self, event: &weapon::data_model::Timestamped<Self::Event>) -> Self {
        match event.event {
            DeckSelectionEvent::SelectLanguage(language) => DeckSelection::Selected(language),
        }
    }
}

#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Ord, PartialOrd, tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum DeckSelectionEvent {
    SelectLanguage(Language),
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
