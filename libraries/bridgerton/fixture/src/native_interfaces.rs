use super::{Card, Counter};
use bridgerton::{Callback, bridge};

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
pub enum ReviewError {
    Offline,
    InvalidAnswer { term: String, attempts: u32 },
    Rejected(Box<Card>),
}

// Source errors keep their containing case and diagnostic; ordinary fields
// retain their types. No Rust error reconstruction or duplicate DTO is needed.
#[bridge(error)]
pub enum SourceError {
    Read(#[bridge(message)] std::io::Error),
    Status(u16),
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "read failed: {error}"),
            Self::Status(status) => write!(f, "status {status}"),
        }
    }
}

type ReviewResult<T> = Result<T, ReviewError>;
type Progress = Option<Callback<(String, f32)>>;
type Observer = Callback<u32>;
type CounterAlias = Counter;

#[bridge]
impl Counter {
    pub fn typed_result(&self, fail: bool) -> ReviewResult<Card> {
        if fail {
            Err(ReviewError::InvalidAnswer {
                term: "語".into(),
                attempts: 2,
            })
        } else {
            Ok(self.sample_card())
        }
    }
    pub async fn typed_result_later(&self) -> ReviewResult<()> {
        Err(ReviewError::Rejected(Box::new(self.sample_card())))
    }
    pub fn io_result(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing test pack",
        ))
    }
    pub fn string_error(&self) -> Result<(), String> {
        Err("plain error".into())
    }
    pub fn source_result(&self) -> Result<(), SourceError> {
        Err(SourceError::Read(std::io::Error::other("opaque source")))
    }
    pub fn optional_progress(&self, progress: Progress) {
        if let Some(progress) = progress {
            progress.call(("Loading".into(), 50.0)).unwrap();
        }
    }
    pub fn aliased_observer(&self, callback: Observer) {
        callback.call(self.value()).unwrap();
    }
    pub fn aliased_object(&self, other: &CounterAlias) -> u32 {
        other.value()
    }
}

#[bridge(opaque)]
pub struct FallibleFactory;
#[bridge]
impl FallibleFactory {
    #[bridge(constructor)]
    pub fn new() -> ReviewResult<Self> {
        Err(ReviewError::Offline)
    }
}
#[bridge]
impl Counter {
    #[bridge(getter)]
    pub fn checked_value(&self) -> ReviewResult<u32> {
        Err(ReviewError::Offline)
    }
}

// A common Rust error type name must not shadow Swift's Error protocol.
#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
pub enum Error {
    Offline,
}
#[bridge]
impl Counter {
    pub fn named_error(&self) -> Result<(), Error> {
        Err(Error::Offline)
    }
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
pub struct TripleValue {
    pub parts: (u32, String, bool),
}
#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
pub enum NameShadow {
    TripleValue(TripleValue),
}
#[bridge]
impl Counter {
    pub fn triple_value(&self, value: TripleValue) -> NameShadow {
        NameShadow::TripleValue(value)
    }
    pub fn return_budget(&self) -> Result<Vec<Vec<String>>, bridgerton::Error> {
        Ok(vec![
            vec![String::new(); 40_000],
            vec![String::new(); 40_000],
        ])
    }
}

#[bridge]
impl Counter {
    pub fn emit_optional_object(
        &self,
        present: bool,
        callback: bridgerton::Callback<Option<crate::Counter>>,
    ) -> Result<(), bridgerton::Error> {
        callback.call(present.then(crate::Counter::new))
    }
    pub fn emit_invalid_objects(
        &self,
        callback: bridgerton::Callback<(crate::Counter, Vec<String>, crate::Counter)>,
    ) -> Result<(), bridgerton::Error> {
        callback.call((
            crate::Counter::new(),
            vec![String::new(); 65_537],
            crate::Counter::new(),
        ))
    }
}

#[bridge]
impl Counter {
    pub fn emit_large_strings(
        &self,
        callback: bridgerton::Callback<(String, String)>,
    ) -> Result<(), bridgerton::Error> {
        callback.call(("a".repeat(9 * 1024 * 1024), "b".repeat(9 * 1024 * 1024)))
    }
}
