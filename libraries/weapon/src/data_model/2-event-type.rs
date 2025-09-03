//! # EventType
//! For more flexibility, we split events into "User events" and "Meta events".
//! User events are determined by application developer, and will typically be created by user actions.
//! Meta events are reserved for internal use. Currently, there are no meta events.
//! But they will be used for things like naming the device and storing other metadata.

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
pub enum MetaEvent {}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(target_arch = "wasm32", derive(tsify::Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi, into_wasm_abi))]
pub enum EventType<E> {
    User(E),
    Meta(MetaEvent),
}

impl<E> EventType<E> {
    pub fn map<G, F: Fn(E) -> G>(self, f: F) -> EventType<G> {
        match self {
            EventType::User(e) => EventType::User(f(e)),
            EventType::Meta(e) => EventType::Meta(e),
        }
    }
}

impl<E, Error> EventType<Result<E, Error>> {
    pub fn transpose(self) -> Result<EventType<E>, Error> {
        match self {
            EventType::User(e) => e.map(EventType::User),
            EventType::Meta(e) => Ok(EventType::Meta(e)),
        }
    }
}

impl<E: crate::Event> crate::Event for EventType<E> {
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let s = self.clone().map(|e| e.to_json()).transpose()?;
        serde_json::to_value(&s)
    }

    fn from_json(json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        let s = serde_json::from_value::<EventType<serde_json::Value>>(json.clone())?;
        s.map(|e| E::from_json(&e)).transpose()
    }
}
