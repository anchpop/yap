#[path = "1-event.rs"]
mod event;

#[path = "2-event-type.rs"]
mod event_type;

#[path = "3-timestamped.rs"]
mod timestamped;

#[path = "4-event-stream-store.rs"]
mod event_stream_store;

#[path = "5-stream-store.rs"]
mod stream_store;

#[path = "6-dirty-tracker.rs"]
mod dirty_tracker;

#[path = "7-event-store.rs"]
mod event_store;

pub use dirty_tracker::*;
pub use event::*;
pub use event_store::*;
pub use event_stream_store::*;
pub use event_type::*;
pub use stream_store::*;
pub use timestamped::*;

#[bridgerton::bridge(opaque)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ListenerKey(pub(crate) slotmap::DefaultKey);

/// `Send`/`Sync` bounds that apply only off-wasm.
///
/// The browser build registers JS-callback listeners, which are inherently
/// single-threaded and `!Send`; native consumers (yap-mcp, tests) need
/// `EventStore` to cross thread boundaries. These markers make `Send`
/// (`+ Sync` for the shared-callback variant) required on native and a no-op
/// on wasm.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSendSync for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSendSync for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off-wasm, `EventStore` must be `Send` so it can live behind an async
    /// mutex (yap-mcp holds one across await points).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn event_store_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<EventStore<String, String>>();
    }

    #[test]
    fn test_empty_events() {
        let events: EventStreamStore<String, i32> = EventStreamStore::default();
        let collected: Vec<i32> = events.iter().cloned().collect();
        let expected: Vec<i32> = vec![];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_single_key_events() {
        let mut events = EventStreamStore::default();
        events.add_event_unchecked("device1", 3);
        events.add_event_unchecked("device1", 1);
        events.add_event_unchecked("device1", 2);

        let collected: Vec<_> = events.iter().cloned().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_multiple_keys_events() {
        let mut events = EventStreamStore::default();
        events.add_event_unchecked("device1", 1);
        events.add_event_unchecked("device2", 2);
        events.add_event_unchecked("device1", 3);
        events.add_event_unchecked("device2", 4);
        events.add_event_unchecked("device3", 0);

        let collected: Vec<_> = events.iter().cloned().collect();
        assert_eq!(collected, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_duplicate_values_across_keys() {
        let mut events = EventStreamStore::default();
        events.add_event_unchecked("device1", 1);
        events.add_event_unchecked("device2", 1);
        events.add_event_unchecked("device1", 2);
        events.add_event_unchecked("device2", 2);

        // BTreeSet will deduplicate within each key, but we'll see duplicates across keys
        let collected: Vec<_> = events.iter().cloned().collect();
        assert_eq!(collected, vec![1, 1, 2, 2]);
    }

    #[test]
    fn test_string_events() {
        let mut events = EventStreamStore::default();
        events.add_event_unchecked("user1", "apple".to_string());
        events.add_event_unchecked("user2", "banana".to_string());
        events.add_event_unchecked("user1", "cherry".to_string());
        events.add_event_unchecked("user2", "apricot".to_string());

        let collected: Vec<_> = events.iter().cloned().collect();
        assert_eq!(collected, vec!["apple", "apricot", "banana", "cherry"]);
    }

    #[test]
    fn test_interleaved_ordering() {
        let mut events = EventStreamStore::default();
        // Add events in a way that tests the merge algorithm
        events.add_event_unchecked("A", 1);
        events.add_event_unchecked("A", 4);
        events.add_event_unchecked("A", 7);
        events.add_event_unchecked("B", 2);
        events.add_event_unchecked("B", 5);
        events.add_event_unchecked("B", 8);
        events.add_event_unchecked("C", 3);
        events.add_event_unchecked("C", 6);
        events.add_event_unchecked("C", 9);

        let collected: Vec<_> = events.iter().cloned().collect();
        assert_eq!(collected, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
