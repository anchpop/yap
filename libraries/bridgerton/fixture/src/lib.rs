//! This is the application-facing API: no cfgs, JsValue, Swift handles, Send, or locks.
mod conditional_methods;
mod extra_methods;
mod native_interfaces;
#[cfg(not(target_arch = "wasm32"))]
mod recursive_values;

mod release_api;
use bridgerton::{AbortSignal, Callback, Error, bridge};
pub use release_api::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

// Fixture timing: a shared native timer driver exercises background wakeups.
async fn sleep(milliseconds: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(milliseconds).await;
    #[cfg(not(target_arch = "wasm32"))]
    futures_timer::Delay::new(std::time::Duration::from_millis(milliseconds.into())).await;
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub text: String,
    pub gloss: Option<String>,
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewState {
    New,
    Learning { steps: u32, due: Option<String> },
    Known(String),
    Pair(u32, bool),
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: u32,
    pub term: Term,
    pub alternatives: Vec<Term>,
    pub state: ReviewState,
    pub tags: Vec<String>,
    pub starred: bool,
}

// Conversion belongs to the type, so callers may use a Rust alias too.
pub type CardAlias = Card;

// Exercise a failing output conversion at the real WASM boundary. Throwing
// from low-level ABI conversion would strand Counter's receiver borrow.
#[cfg(target_arch = "wasm32")]
pub struct CannotEncode;

#[cfg(target_arch = "wasm32")]
impl bridgerton::IntoWasm for CannotEncode {
    type Output = wasm_bindgen::JsValue;
    fn into_wasm(self) -> Result<Self::Output, wasm_bindgen::JsValue> {
        Err(wasm_bindgen::JsValue::from_str(
            "intentional encoding failure",
        ))
    }
}

#[cfg(target_arch = "wasm32")]
#[bridge]
impl Counter {
    pub fn fail_encoding(&self) -> CannotEncode {
        CannotEncode
    }

    pub async fn fail_encoding_later(&self) -> CannotEncode {
        sleep(1).await;
        CannotEncode
    }
}

thread_local! {
    static LIVE_COUNTERS: Cell<u32> = const { Cell::new(0) };
    static ACTIVE_OPERATIONS: Cell<u32> = const { Cell::new(0) };
}

#[bridge(opaque)]
pub struct Counter {
    value: Rc<Cell<u32>>,
    callback: RefCell<Option<Callback<u32>>>,
}

#[bridge]
impl Counter {
    #[bridge(constructor)]
    pub fn new() -> Self {
        LIVE_COUNTERS.with(|count| count.set(count.get() + 1));
        Self {
            value: Rc::new(Cell::new(0)),
            callback: RefCell::new(None),
        }
    }

    pub async fn create(initial: u32) -> Result<Self, Error> {
        let counter = Self::new();
        sleep(1).await;
        counter.value.set(initial);
        Ok(counter)
    }

    pub fn value(&self) -> u32 {
        self.value.get()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            value: Rc::new(Cell::new(self.value())),
        }
    }

    pub async fn snapshot_later(&self) -> Result<crate::Snapshot, Error> {
        let snapshot = self.snapshot();
        sleep(1).await;
        Ok(snapshot)
    }

    pub fn sample_card(&self) -> Card {
        Card {
            id: self.value(),
            term: Term {
                text: "語 🦀".into(),
                gloss: Some("language".into()),
            },
            alternatives: vec![Term {
                text: "言葉".into(),
                gloss: None,
            }],
            state: ReviewState::New,
            tags: vec!["日本語".into(), String::new()],
            starred: false,
        }
    }

    pub fn revise_card(&self, mut card: Card, state: ReviewState) -> Result<Card, Error> {
        if card.term.text.is_empty() {
            return Err(Error::new("card text cannot be empty"));
        }
        card.id = card
            .id
            .checked_add(1)
            .ok_or_else(|| Error::new("card id overflow"))?;
        card.state = state;
        card.starred = true;
        Ok(card)
    }

    pub fn echo_cards(&self, cards: Vec<Option<Card>>) -> Vec<Option<Card>> {
        cards
    }

    pub fn echo_alias(&self, card: CardAlias) -> CardAlias {
        card
    }

    pub fn maybe_card(&self, card: Option<Card>) -> Option<Card> {
        card
    }

    pub async fn echo_nested(&self, cards: Vec<Vec<Option<Card>>>) -> Vec<Vec<Option<Card>>> {
        sleep(1).await;
        cards
    }

    #[bridge(getter)]
    pub fn cards(&self) -> Vec<Card> {
        vec![self.sample_card()]
    }

    pub fn echo_state(&self, state: ReviewState) -> ReviewState {
        state
    }

    pub fn rename_card(&self, mut card: Card, text: String) -> Card {
        card.term.text = text;
        card
    }

    pub async fn card_later(&self, card: Card, cancellation: AbortSignal) -> Result<Card, Error> {
        let _operation = Operation::new();
        let local = self.value.clone();
        cancellation.until(sleep(10)).await?;
        assert!(Rc::ptr_eq(&local, &self.value));
        self.revise_card(card, ReviewState::Known("remembered".into()))
    }

    pub fn add(&self, amount: u32) -> Result<u32, Error> {
        let value = self
            .value
            .get()
            .checked_add(amount)
            .ok_or_else(|| Error::new("counter overflow"))?;
        self.value.set(value);
        let callback = self.callback.borrow().clone();
        if let Some(callback) = callback {
            callback.call(value)?;
        }
        Ok(value)
    }

    pub fn observe(&self, callback: Callback<u32>) {
        let old = self.callback.replace(Some(callback));
        drop(old); // Host destruction may call back into Rust, after the borrow is released.
    }

    pub fn clear_observer(&self) {
        let old = self.callback.take();
        drop(old);
    }

    pub async fn add_later(
        &self,
        amount: u32,
        milliseconds: u32,
        cancellation: AbortSignal,
    ) -> Result<u32, Error> {
        let _operation = Operation::new();
        let local = self.value.clone(); // Rc is intentionally retained across suspension: !Send.
        cancellation.until(sleep(milliseconds)).await?;
        assert!(Rc::ptr_eq(&local, &self.value));
        self.add(amount)
    }

    /// Like background prefetch: cancellation returns normally and skips the work.
    pub async fn abortable_wait(&self, milliseconds: u32, signal: Option<AbortSignal>) -> bool {
        let _operation = Operation::new();
        let pause = sleep(milliseconds);
        if let Some(signal) = signal {
            signal.until(pause).await.is_err()
        } else {
            pause.await;
            false
        }
    }

    pub fn consume(&self, other: Counter) -> u32 {
        other.value()
    }
    pub fn consume_optional(&self, other: Option<Counter>) -> Option<u32> {
        other.map(|other| other.value())
    }
    pub async fn consume_later(&self, other: Counter) -> Result<u32, Error> {
        sleep(5).await;
        Ok(other.value())
    }
    pub fn consume_with_card(&self, card: Card, other: Counter) -> u32 {
        other.value() + card.id
    }
    pub fn try_consume_two(&self, first: Counter, second: Counter) -> Result<u32, Error> {
        Ok(first.value() + second.value())
    }
    pub fn emit_object(&self, callback: Callback<Counter>) -> Result<(), Error> {
        callback.call(Counter::new())
    }
    pub fn emit_objects(
        &self,
        callback: Callback<(Counter, String, Counter)>,
    ) -> Result<(), Error> {
        callback.call((Counter::new(), "objects".into(), Counter::new()))
    }

    pub fn observe_three(&self, callback: bridgerton::Callback<(u32, String, bool)>) {
        callback.call((42, "three".into(), true)).unwrap();
    }
    pub fn echo_indices(&self, indices: Vec<usize>) -> Vec<usize> {
        indices
    }
    pub fn echo_bytes(&self, bytes: Vec<u8>) -> Vec<u8> {
        bytes
    }
    pub fn nested_bytes(&self) -> Vec<Vec<u8>> {
        vec![vec![0, 128, 255], vec![]]
    }

    pub fn object_list(&self) -> Vec<Counter> {
        (1..=3)
            .map(|n| {
                let counter = Self::new();
                counter.add(n).unwrap();
                counter
            })
            .collect()
    }
    pub fn optional_object(&self, present: bool) -> Option<Counter> {
        present.then(Self::new)
    }
    pub async fn objects_later(&self) -> Result<Vec<Counter>, Error> {
        sleep(1).await;
        Ok(self.object_list())
    }
    pub fn nested_objects(&self) -> Vec<Option<Counter>> {
        vec![None, Some(Self::new())]
    }
    pub fn nested_values(&self) -> Vec<Vec<String>> {
        vec![vec!["one".into()], vec![], vec!["two".into()]]
    }

    pub fn live_counters(&self) -> u32 {
        LIVE_COUNTERS.with(Cell::get)
    }
    pub fn active_operations(&self) -> u32 {
        ACTIVE_OPERATIONS.with(Cell::get)
    }

    pub fn fail(&self) -> Result<u32, Error> {
        // Platform storage errors use the same ordinary `?` propagation.
        #[cfg(not(target_arch = "wasm32"))]
        let result: Result<u32, _> = Err(std::io::Error::other("intentional error: 日本語"));
        #[cfg(target_arch = "wasm32")]
        let result: Result<u32, _> =
            Err(wasm_bindgen::JsValue::from_str("intentional error: 日本語"));
        let value = result?;
        Ok(value)
    }

    pub fn panic_now(&self) -> u32 {
        panic!("intentional Rust panic")
    }

    pub fn panic_result(&self) -> Result<u32, Error> {
        panic!("intentional Rust panic in Result")
    }

    pub async fn panic_later(&self) -> Result<u32, Error> {
        sleep(1).await;
        panic!("intentional Rust panic after suspension")
    }

    pub async fn value_later(&self, milliseconds: u32) -> u32 {
        let _operation = Operation::new();
        sleep(milliseconds).await;
        self.value.get()
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        LIVE_COUNTERS.with(|count| {
            count.set(
                count
                    .get()
                    .checked_sub(1)
                    .expect("counter dropped on wrong thread"),
            )
        });
    }
}

struct Operation;
impl Operation {
    fn new() -> Self {
        ACTIVE_OPERATIONS.with(|count| count.set(count.get() + 1));
        Self
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        ACTIVE_OPERATIONS.with(|count| {
            count.set(
                count
                    .get()
                    .checked_sub(1)
                    .expect("operation dropped on wrong thread"),
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_callback_can_reenter_without_a_borrow_panic() {
        let counter = Rc::new(Counter::new());
        let weak = Rc::downgrade(&counter);
        counter.observe(Callback::new(move |value| {
            let counter = weak.upgrade().unwrap();
            assert_eq!(counter.value(), value);
            counter.clear_observer();
            Ok(())
        }));
        assert_eq!(counter.add(5).unwrap(), 5);
        assert!(counter.callback.borrow().is_none());
    }
}

// Native metadata needs no feature on the consumer or the bridge.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_attributes {
    use bridgerton::bridge;

    #[bridge(transparent)]
    #[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
    #[serde(crate = "bridgerton::serde")]
    #[derive(Debug, PartialEq)]
    pub struct Payload {
        value: u32,
    }

    #[bridge(opaque)]
    pub struct Probe;

    #[bridge]
    impl Probe {
        pub fn echo(value: Payload) -> Payload {
            value
        }
    }

    #[test]
    fn native_metadata_is_generated_without_marker_features() {
        let definition = bridgerton::exports::definition().unwrap();
        assert!(definition.header.contains("bridgerton_probe_echo"));
        assert!(definition.types.swift().contains("struct `Payload`"));
        let input = Payload { value: 42 };
        let encoded = bridgerton::value::encode(&input).unwrap();
        let decoded = bridgerton::value::decode(&encoded).unwrap();
        assert_eq!(Probe::echo(decoded), input);
    }
}

// Declared after its callers: transport resolution uses Rust traits, not macro order.
#[bridge(opaque)]
pub struct Snapshot {
    value: Rc<Cell<u32>>,
}

#[bridge]
impl Snapshot {
    // An object with only an inferred async factory needs no constructor annotation.
    pub async fn create(value: u32) -> Self {
        sleep(1).await;
        Self {
            value: Rc::new(Cell::new(value)),
        }
    }

    pub fn value(&self) -> u32 {
        self.value.get()
    }

    #[bridge(getter)]
    pub fn doubled(&self) -> u32 {
        self.value.get() * 2
    }

    #[bridge(skip)]
    pub fn rust_only<T>(&self, value: T) -> T {
        value
    }
}

// Bridge transparency does not select a Serde representation.
#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize, Debug, PartialEq)]
#[serde(
    crate = "bridgerton::serde",
    tag = "kind",
    content = "payload",
    rename_all = "snake_case"
)]
pub enum WireState {
    NotStarted,
    InProgress {
        #[serde(rename = "completedCount")]
        completed: u32,
    },
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize, Debug, PartialEq)]
#[serde(crate = "bridgerton::serde")]
pub struct Envelope<T> {
    pub value: T,
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize, Debug, PartialEq)]
#[serde(crate = "bridgerton::serde", transparent)]
pub struct WireId {
    pub value: u32,
}

#[bridge]
impl Counter {
    pub fn echo_wire_state(&self, value: Envelope<WireState>) -> Envelope<WireState> {
        value
    }
    pub fn maybe_wire_state(
        &self,
        value: Option<Envelope<WireState>>,
    ) -> Option<Envelope<WireState>> {
        value
    }
    pub fn echo_wire_id(&self, value: WireId) -> WireId {
        value
    }
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
pub struct BrokenValue {
    pub value: u32,
}
impl bridgerton::serde::Serialize for BrokenValue {
    fn serialize<S: bridgerton::serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(bridgerton::serde::ser::Error::custom(
            "intentional Tsify serialization failure",
        ))
    }
}
#[bridge]
impl Counter {
    pub fn fail_value_output(&self) -> BrokenValue {
        BrokenValue { value: 1 }
    }
    pub async fn fail_value_output_later(&self) -> BrokenValue {
        sleep(1).await;
        BrokenValue { value: 1 }
    }
}
