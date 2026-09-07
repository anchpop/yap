//! # EventType
//! For more flexibility, we split events into "User events" and "Meta events".
//! User events are determined by application developer, and will typically be created by user actions.
//! Meta events are reserved for internal use. Currently, there are no meta events.
//! But they will be used for things like naming the device and storing other metadata.

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
pub enum MetaEvent {}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
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

    pub fn map_ref<G, F: Fn(&E) -> G>(&self, f: F) -> EventType<G> {
        match self {
            EventType::User(e) => EventType::User(f(e)),
            // MetaEvent is uninhabited, so this arm can never be reached.
            EventType::Meta(e) => match *e {},
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
    type Versioned = EventType<E::Versioned>;
    type Context = E::Context;

    fn to_versioned(&self) -> Self::Versioned {
        self.map_ref(|e| e.to_versioned())
    }

    fn from_versioned(versioned: &Self::Versioned, context: &Self::Context) -> Option<Self> {
        match versioned {
            EventType::User(v) => E::from_versioned(v, context).map(EventType::User),
            // MetaEvent is uninhabited, so this arm can never be reached.
            EventType::Meta(e) => match *e {},
        }
    }
}
