# Weapon - A Local-First Cross-Device Sync Engine

Weapon is a Rust library for building local-first applications with cross-device sync. It is designed primarily to be compiled to WASM and used with React applications. That said, isn't react-specific in any way and would probably work in a Dioxus app (or similar) as well. I made it for [Yap.Town](https://yap.town), a language learning app I work on sometimes.

## Event-Based

Weapon is event-based. In other words:
- In response to user actions, your code generates "events" with unique IDs and timestamps
- Each event describes a modification to the application state
- Application state is then derived from "replaying" the chronological sequence of events (starting from the initial state)
- Synchronization simply merges events from all devices

This architecture can support any op-based CRDT.

## Features

1. All data is stored locally using browser storage (OPFS)
2. Works fully offline with zero network dependency (great for PWAs!)
3. Users can use your app without logging in
4. When they do log in, their local data automatically syncs to their account and is saved in the cloud
5. Changes sync instantly across all devices (supabase, websockets)
6. Complete audit trail of all changes

One cool thing about event sourcing is that it enables fixing bugs retroactively. When you fix a bug in your state computation logic, users will replay all historical events through the corrected code to regenerate a bug-free state. This effectively "rewrites history" as if the bug never existed.

Take a budgeting app for example. In your first attempt you use floating point numbers. Later, you discover floating-point precision errors and switch to fixed-precision arithmetic. Users' devices will replay all events and recalculate every transaction with the correct precision, fixing all historical calculation errors automatically.

This might not sound like a huge deal, but in some cases it's extremely convenient. 

## More in-depth explanation

### Events

Events are the atomic units of change in Weapon. Each event must:
- Have a timestamp and device-specific index
- Be immutable once created
- Be versioned for backward compatibility (stored in versioned form, converted to current form during processing)
- Be serializable to JSON for storage/transmission

```rust
use weapon::data_model::Event;
use serde::{Serialize, Deserialize};

// A simple event with no versioning (Versioned = Self)
#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub enum MyEvent {
    Increment,
    Decrement,
}

impl Event for MyEvent {
    type Versioned = Self;
    type Context = (); // Used to store any additional info needed to implement from_versioned

    fn to_versioned(&self) -> Self::Versioned {
        self.clone()
    }

    fn from_versioned(versioned: &Self::Versioned, _context: &Self::Context) -> Option<Self> {
        Some(versioned.clone())
    }
}
```

### App State

Application state is computed by applying events in chronological order.

There is a distinction between the "partial" state and the "final" state. You give weapon the initial "partial" state, which is the same for all users, and a `process_event` function for updating that state given an event. Then you also provide a function to "finalize" a partial state into the true app state. (You might want to defer some expensive operations to this finalization function when possible, because it only runs once, as opposed to the `process_event` function which runs once for each event.)

The partial state type is defined by `AppState::Partial`:

```rust
use weapon::{AppState, data_model::{Event, Timestamped}};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub enum CounterEvent {
    Increment,
    Decrement,
}

impl Event for CounterEvent {
    type Versioned = Self;
    type Context = ();  // No context needed for deversioning

    fn to_versioned(&self) -> Self::Versioned { self.clone() }
    fn from_versioned(v: &Self::Versioned, _: &Self::Context) -> Option<Self> { Some(v.clone()) }
}

pub struct Counter(i32);

impl AppState for Counter {
    type Event = CounterEvent;
    type Partial = i32;  // Partial state is just the count

    fn process_event(count: Self::Partial, _context: &<Self::Event as Event>::Context, event: &Timestamped<Self::Event>) -> Self::Partial {
        match event.event {
            CounterEvent::Increment => count + 1,
            CounterEvent::Decrement => count - 1,
        }
    }

    fn finalize(count: Self::Partial, _context: &<Self::Event as Event>::Context) -> Self {
        Counter(count)
    }
}
```

## Example: Todo App

```rust
use weapon::{AppState, data_model::{Event, Timestamped}};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// --- Events ---

// Current event type - what your app logic uses
#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub enum TodoEvent {
    Add { id: String, text: String },
    Complete { id: String },
    Delete { id: String },
}

// Versioned wrapper for storage - allows future migrations
#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(tag = "version")]
pub enum VersionedTodoEvent {
    V1(TodoEvent),
    // V2(TodoEventV2), // Add new versions as schema evolves
}

impl Event for TodoEvent {
    type Versioned = VersionedTodoEvent;
    type Context = ();

    fn to_versioned(&self) -> Self::Versioned {
        VersionedTodoEvent::V1(self.clone())
    }

    fn from_versioned(versioned: &Self::Versioned, _context: &Self::Context) -> Option<Self> {
        match versioned {
            VersionedTodoEvent::V1(event) => Some(event.clone()),
            // Future: migrate V2 events to current format here
        }
    }
}

// --- State ---

#[derive(Clone)]
pub struct Todo { text: String, completed: bool }

// Final state with derived data
pub struct TodoList {
    todos: HashMap<String, Todo>,
    pending_count: usize,  // Derived in finalize()
}

impl AppState for TodoList {
    type Event = TodoEvent;
    type Partial = HashMap<String, Todo>;

    fn process_event(mut todos: Self::Partial, _context: &<Self::Event as Event>::Context, event: &Timestamped<Self::Event>) -> Self::Partial {
        match &event.event {
            TodoEvent::Add { id, text } => {
                todos.insert(id.clone(), Todo { text: text.clone(), completed: false });
            }
            TodoEvent::Complete { id } => {
                if let Some(todo) = todos.get_mut(id) {
                    todo.completed = true;
                }
            }
            TodoEvent::Delete { id } => {
                todos.remove(id);
            }
        }
        todos
    }

    fn finalize(todos: Self::Partial, _context: &<Self::Event as Event>::Context) -> Self {
        let pending_count = todos.values().filter(|t| !t.completed).count();
        TodoList { todos, pending_count }
    }
}
```

### React Integration

```typescript
import { Weapon } from 'weapon-wasm';

function WeaponProvider({ userId, children }) {
    const [weapon, setWeapon] = useState(null);
    
    useEffect(() => {
        async function init() {
            // Initialize Weapon with sync callback
            const weaponInstance = await Weapon.create(
                userId,
                async (listenerId, streamId) => {
                    // Sync when events change
                    await weaponInstance.sync(streamId, accessToken);
                }
            );
            setWeapon(weaponInstance);
        }
        init();
    }, [userId]);
    
    // Subscribe to stream changes
    useEffect(() => {
        if (!weapon) return;
        
        const unsubscribe = weapon.subscribe_to_stream('deck_events', () => {
            // React to changes
            setDeckState(weapon.get_deck_state());
        });
        
        return () => weapon.unsubscribe(unsubscribe);
    }, [weapon]);
    
    return (
        <WeaponContext.Provider value={weapon}>
            {children}
        </WeaponContext.Provider>
    );
}

// Usage in components
function DeckComponent() {
    const weapon = useWeapon();
    
    const handleCardReview = (cardId, rating) => {
        // Add event - automatically syncs
        weapon.add_deck_event({
            type: 'CardReviewed',
            card_id: cardId,
            rating: rating
        });
    };
    
    return <div>...</div>;
}
```

### 5. Cross-Tab Synchronization

Weapon supports synchronization between browser tabs using BroadcastChannel:

```javascript
// Automatically handled by Weapon - tabs notify each other of changes
const channel = new BroadcastChannel('weapon-opfs-sync');

channel.onmessage = (event) => {
    if (event.data?.type === 'opfs-written') {
        // Reload affected stream from local storage
        weapon.load_from_local_storage(event.data.stream_id);
    }
};
```

## Sync Strategy

Weapon implements a simple synchronization strategy:

1. **Event Generation**: User actions create timestamped events
2. **Device Identification**: Each device gets a unique ID
3. **Local Storage**: Events are immediately persisted locally
4. **Cloud Sync**: Events sync to cloud when online
5. **Conflict Resolution**: Events merge chronologically by timestamp
6. **Real-time Updates**: Changes propagate instantly via WebSockets

The sync protocol ensures:
- No data loss during offline periods
- Eventual consistency across all devices
- Minimal sync overhead (only new events transfer)
- Automatic conflict resolution via timestamps

## Benefits

- **Instant UI Response**: No network latency for user actions
- **Offline Capable**: Full functionality without internet
- **Cross-Device Sync**: Seamless experience across devices
- **Data Portability**: Export/import entire event history
- **Time Travel**: Replay events to any point in time
- **Audit Trail**: Complete history of all changes
- **Conflict-Free**: Automatic merging of concurrent edits

## Status

Weapon is currently in active development and used in production by Yap.Town. While functional, the API may evolve significantly.
