# Bridgerton

One Rust API produces real wasm-bindgen bindings for JavaScript and generated
main-actor bindings for Swift. Rust objects and futures can contain `Rc`, `Cell`,
and `RefCell`: neither `Send` nor `Sync` is required.

This is the workspace's binding layer. `src/` is the runtime (including the
`platform` module of shared host utilities), `macros/` the attribute macro,
`cli/` the `cargo bridgerton` build command, and `fixture/` the test crate.
Application crates depend on `bridgerton` alone: no `wasm-bindgen`, `tsify`,
or `js-sys` declarations, no feature flags, and no `cfg(target_arch)` in their
code. [Real Yap in SwiftUI](../../prototypes/yap-swift) exposes the existing
`Weapon` and `Deck` APIs through it and runs a native screen.

## Start here

- [The Rust API](fixture/src/lib.rs): ordinary methods, with `#[bridge(opaque)]` on the
  struct and `#[bridge]` on its impl. `#[bridge(constructor)]` selects a synchronous JS/Swift
  constructor; associated functions (including async factories) are inferred. No platform
  branches, foreign handles, or thread annotations in the application code.
- [Swift usage and checks](tests/Native.swift).
- [JavaScript usage and checks](tests/node.cjs).
- [The macro](macros/src/lib.rs): derives both bindings from those methods.
- [Rust runtime](src/native.rs) and
  [Swift runtime template](src/runtime.swift): ownership and async.

For example, the same Rust method:

```rust
pub async fn add_later(
    &self,
    amount: u32,
    milliseconds: u32,
    cancellation: AbortSignal,
) -> Result<u32, Error>
```

is called from Swift on the main actor:

```swift
let counter = Counter()
let controller = AbortController()
let value = try await counter.add_later(
    amount: 3, milliseconds: 10, cancellation: controller.signal()
)
```

and JavaScript:

```js
const counter = new Counter();
const controller = new AbortController();
const value = await counter.add_later(3, 10, controller.signal);
```

The WASM macro expansion delegates ABI conversion, JavaScript object ownership,
and Promise integration to wasm-bindgen. The native expansion emits a small C
ABI and Swift source. The generator collects definitions emitted by the macro;
there is no second handwritten interface description. The bridge has no UniFFI
dependency. Its Swift integer codec includes adapted UniFFI code; see
[source attribution and license](THIRD_PARTY.md).

## Build with `cargo bridgerton`

One command builds either platform. A cargo alias in `.cargo/config.toml` runs
the `cli/` crate, so nothing needs installing:

```sh
cargo bridgerton web --package yap-frontend-rs --release
cargo bridgerton swift --package yap-swift-prototype --out-dir prototypes/yap-swift/generated
cargo bridgerton package --bindings prototypes/yap-swift/generated --module Yap --out-dir dist/Yap
```

`web` wraps wasm-pack (`--target web|nodejs|bundler`, `--out-dir`, `--release`,
`--features`). `swift` builds the package's `cdylib`, loads it, and invokes the
bridge's generation entry point, writing `Bridge.swift`, `BridgeFFI.h`,
`module.modulemap`, and `build.json`; `--target aarch64-apple-ios-sim --simulator UUID`
(or `--runner`) executes the generator on that target instead of the host.
`package` turns one or more `swift` outputs into an XCFramework plus a Swift
package. There is no application-specific generator or list of exported objects.

The struct annotation owns the Swift class, handle, and destructor. Each
annotated impl contributes a Swift extension automatically, even in a different
file. Generated metadata registers through `inventory` across linked crates.
The collector checks names and constructors across impls and orders output
deterministically. Only metadata factories are shared: application objects and
futures retain their existing thread confinement.

## Existing wasm-bindgen APIs

Yap's main impl now uses one attribute:

```rust
use bridgerton::bridge;

#[bridge]
impl Weapon {
    pub async fn create(/* existing arguments */) -> Result<Self, Error> { /* ... */ }

    #[bridge(getter)]
    pub fn device_id(&self) -> String { /* ... */ }
}
```

It exports every public method, infers static factories and object returns, and
leaves private helpers alone. On WASM it emits wasm-bindgen functions; values
decode inside the method body so an error releases borrowed objects normally.
Collections are adapted automatically, including nested nullable elements, and
byte vectors keep their `Uint8Array` ABI. TypeScript collection declarations
support concrete generic types such as `Heteronym<string>`. On native it also
generates the Swift/C bindings. Getters become Swift properties
(`weapon.device_id`); only getters returning `Result` throw. There is no method
allowlist to keep in sync; `#[bridge(only(...))]` exports a subset of an impl.

Method-level conditional compilation works with the same Rust attributes:

```rust
#[bridge]
impl Weapon {
    pub fn device_id(&self) -> String { /* ... */ }

    #[cfg(target_os = "macos")]
    pub fn reveal_data_directory(&self) { /* ... */ }
}
```

Rust evaluates each condition before the bridge inspects the signature.
The method, C wrapper, and metadata are gated together, so generated Swift
includes only active methods and their value dependencies. Nested `cfg_attr`,
feature conditions, and conditional getter/constructor/skip annotations work
on both the native and wasm-bindgen paths. No bridge-specific condition syntax
or registration is needed. As with any conditional API, generate bindings
using the features and target configuration intended for the consuming app;
the current generator loads host-native libraries.

## Generated data types

Types explicitly choose one of two representations:

```rust
#[bridge(transparent)]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Progress {
    pub completed: u32,
}

#[bridge(transparent)]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewState {
    NotStarted,
    Learning { steps: u32 },
}

#[bridge(opaque)]
pub struct Counter {
    value: std::cell::Cell<u32>,
}
```

`transparent` transfers contents as a value. On WASM the bridge emits the
TypeScript declaration itself (a generator adapted from Tsify, see
THIRD_PARTY.md) and converts through serde-wasm-bindgen; native builds generate
the Swift struct/enum and its binary codec. Both come from the same fields and
Serde attributes. The macro infers
struct versus enum; there are no `record`, `native_record`, or `native_enum`
public modes. `opaque` generates a Rust object wrapper on both platforms and
replaces `native_object`. Bare `#[bridge]` belongs on impls, not types.

Serde derives and attributes remain explicit. Bridge transparency does **not**
mean `#[serde(transparent)]`: tags, names, aliases, defaults, skipped fields, and
newtype representations remain controlled by Serde. Put the bridge attribute
before the derives/helper attributes so it can consume its options first.
Declaration options sit on the attribute: `#[bridge(transparent, namespace)]`
declares an enum's cases inside a TypeScript namespace as well; `missing_as_null`,
`hashmap_as_object`, and `large_number_types_as_bigints` choose the JavaScript
representation. A field can take `#[bridge(type = "...")]` to override its
TypeScript type or `#[bridge(optional)]` to declare it optional. No crate
other than `bridgerton` needs to be declared: generated wasm-bindgen code
resolves its crate names through the bridge's re-exports.

Conversions run inside generated method bodies and return errors normally,
allowing Rust locals to be dropped even for malformed JS input. Arrays preserve
each element's configured conversion.

There is no feature switch. Native builds always generate Swift metadata, so
every bridged declaration is checked for both platforms on every build, and
Tokio support is always present natively. Target checks happen in the consumer,
so a WASM build never emits native bindings.

Native data derives implement `NativeType`: method signatures register their
value dependencies recursively. Rust resolves aliases and concrete generic
arguments; the generator emits concrete Swift types (for example,
`SyncState_String_String`). Object and impl metadata are discovered automatically;
records/enums and handwritten aliases are reached through method signatures,
including callback payloads. Bridge-declared records/enums use this same
discovery and codec generator. Unreferenced value types are not emitted.

Native data supports generic records/enums, tuple structs, pairs and triples, `Box`,
`BTreeMap`, `BTreeSet`, optionals, arrays, and 8/16/32/64-bit integers,
`usize` (transported as `UInt64`), `f32`, and `f64`. Generated types are
`Hashable` and `Sendable`. With `chrono`, UTC timestamps use an exact
seconds/nanoseconds `BridgeTimestamp`; its `Date` property is a convenience
view with floating-point precision. `#[serde(skip)]` fields stay internal and
are restored with `Default` when a foreign value is decoded. Other Serde tags,
renames, aliases, and stored representations are untouched. Swift naming
collisions between different Rust types fail generation.

The generator follows Swift's inline value layout and marks only enum cases
participating in recursive cycles as `indirect`. It follows records, optionals,
pairs, triples, aliases, and other enums; arrays, dictionaries, and sets already provide
storage indirection. Ordinary payload cases stay inline. Purely recursive
structs with erased Rust `Box` storage still require a separate representation;
Swift has no `indirect struct`.

Native values use a bounded binary encoding inspired by UniFFI: big-endian
integers, UTF-8 strings and arrays prefixed with their lengths, one-byte bool and
optional tags, and one-based enum tags. Record fields follow declaration order.
Input bytes are borrowed only inside the C call, decoded to owned Rust values
before returning, and never captured by a suspended future. Output bytes belong
to Rust and are freed exactly once with `defer`, including decoding failures.
The native codec rejects invalid tags, invalid UTF-8, truncation, trailing data,
and values exceeding 16 MiB, 65,536 array elements in total, or 64 nested containers/records.
These limits apply to the native transport only, not to stored data.

WASM goes through wasm-bindgen and serde-wasm-bindgen. By default absent option
fields are `undefined`, maps are JS maps, and large numbers are numbers subject
to the serializer's safe-integer checks; the declaration options above change
these. Ordinary Serde enum tagging is used unless the type chooses another
representation. The bridge's native binary
codec and its limits are not used on WASM. As with ordinary wasm-bindgen async
exports, the body starts on a microtask: JS values are decoded then, before the
Rust method begins. Once decoded, Rust owns an independent copy.

[The Swift value tests](tests/Values.swift) include a real SwiftUI `View` compiled
against the generated types. This verifies that the types are usable in SwiftUI;
it does not implement observable Rust state or launch a native application.

## Ownership model

1. Each native object handle owns one Rust `Rc` reference. Generated Swift
   classes are `@MainActor`; `isolated deinit` releases their handles there too.
   Swift references can travel between tasks, but accessing or destroying the
   underlying Rust state stays on the main thread.
2. Native entry points check the actual main thread before touching confined
   pointers. A caller bypassing Swift's isolation through raw C aborts before
   the access. There are no `unsafe impl Send` or `unsafe impl Sync` declarations.
3. Each call retains its receiver and borrowed object arguments with `Rc` clones.
   Async calls move those references into the future, keeping borrows valid
   through suspension. By-value object arguments (including `Option<Object>`)
   instead transfer ownership and invalidate the Swift wrapper, as wasm-bindgen
   does for JS wrappers. Reusing any Swift alias of that wrapper traps before
   touching Rust. Moving an object that Rust is currently borrowing returns a
   conversion error. All incoming owned arguments acquire Rust drop guards
   before any fallible decoding, so failures release later arguments too.
4. Swift polls and drops the future on the main actor. A Rust `Waker` owns only
   an `Arc` containing a notification function and numeric ID, never the future
   or an application object. Background wakeups enqueue that ID onto the main
   actor. Swift's waiter table ignores IDs removed at completion/cancellation;
   IDs are never reused. Rust also disconnects the notification when freeing the
   task. Thus even a racing late notification cannot reach freed Rust state.
5. `Callback<T>` retains the host closure and releases it on the owner thread.
   Native callbacks support zero to three value or object arguments. An owned
   lazy sequence uses the same transport as method returns: values use their
   codec, objects receive owned Swift wrappers, and unclaimed arguments are
   dropped when the sequence is freed. Objects may outlive the callback or be
   passed straight back to Rust. All arguments share decoding budgets; failure
   releases partial results and returns an error from `Callback::call`.
   Existing wasm-bindgen exports can directly accept `Callback<()>`,
   `Callback<u32>`, `Callback<String>`, generated opaque objects, or pairs/triples
   whose elements convert into `JsValue`.
   The fixture releases `RefCell` borrows before invoking or destroying host
   callbacks, so they can reenter it. As with ordinary closures, a callback that
   strongly captures its owning object can form a reference cycle; the Swift
   fixture uses a weak capture and explicitly unregisters.

Only the notification cell needs a mutex. There is no Rust object registry or
thread-safe wrapper around the application state. Raw C handles still require
the usual FFI contract: valid type, live allocation, exactly one release per
owned reference. Main-thread checks do not validate arbitrary pointers.

## Run

From the workspace root:

```sh
python3 libraries/bridgerton/check.py
```

Requires macOS, Rust with `wasm32-unknown-unknown`, Swift 6.2+, wasm-pack, and Node. TypeScript checks use
Yap's installed TypeScript compiler. The browser check uses Yap's existing
Playwright installation and Chromium; use `--skip-browser` if
those are unavailable. Generated Swift, C headers, WASM packages, and executables
go in the ignored `generated/` directory.

The checks cover:

- Rust formatting, unit tests, and warning-free native/WASM Clippy.
- Swift 6 complete concurrency checking, including an intentionally invalid
  off-actor call that must fail compilation.
- A raw C call from the wrong thread that must abort.
- Sync values, UTF-8 strings, errors, callbacks and callback reentrancy.
- A genuinely non-`Send` future resumed after a background-thread wake.
- Explicit cancellation on both platforms and Swift `Task.cancel()` cleanup.
- Nonthrowing async results still complete after Swift task cancellation.
- Subprocess checks that sync/async Rust panics terminate the process, including
  panics inside `Result` methods, and that infallible conversion failures trap.
- Interleaved operations, retained borrowed arguments, and isolated destruction.
- A registered waker invoked on background threads before and after its future
  and confined state have been destroyed.
- Node and real Chromium loading the generated wasm-bindgen module.
- Nested records, all enum payload forms, arrays/optionals, and sync/async data
  round trips on both platforms, including two encoded arguments in one call.
- Matching Rust/Swift golden bytes, malformed values, every truncated prefix of
  a native record, and async execution after the host frees input buffers.
- A compiled SwiftUI view and TypeScript checks that catch wrong inputs/results.

Validated locally with Swift 6.2.3 and Rust 1.96.0 on Apple Silicon macOS, plus
an iOS device build; execution on an iOS device or simulator is not yet routine.

## Supported surface

The macro supports one synchronous constructor per object type, `&self` methods,
callbacks, borrowed or owned bridged objects, optional owned objects, and the
value types above. Outputs can be `()`, scalars, `String`, values, objects,
nested arrays/optionals of objects, or `Result` wrapping those. Associated
functions become static Swift/JS methods, including async factories returning
`Self`. `#[bridge(skip)]` excludes a method while leaving it callable from Rust;
`#[bridge(only(methods...))]` exports only the named methods of an impl. Named
return types select their transport through Rust traits: objects transfer an
owned handle, values transfer encoded contents. This works across modules and
crates and does not depend on declaration order.

Native arrays and optionals of ordinary values keep the binary codec. Containers
of objects use an owned return iterator: Swift takes one element at a time, and
freeing the iterator drops every unclaimed Rust object. Swift also releases
already-decoded objects on failure. Nested returns share the byte, item, and
depth budgets; handles never enter the ordinary value byte format. Tests cover
empty/present optionals, nested and async object arrays, partial-decoding cleanup,
and aggregate limits. Object arrays are supported as returns and callback
arguments; ordinary value fields and array inputs cannot contain objects. Swift payload decoding uses the expected field
type, avoiding collisions between an enum case and its payload type name.

The TypeScript declaration parser remains narrower than native data discovery.
Collection aliases without their own WASM conversion are not yet supported;
write the collection type directly in exported signatures.
Generic methods/objects, lifetime or const generic data parameters, overloads,
and arbitrary foreign values are not supported. Impl method annotations support
constructors, getters, and skipping exports; other wasm-bindgen method options
need explicit support before use. There is one public macro,
`#[bridgerton::bridge]`, or `#[bridge]` after `use bridgerton::bridge`.

Swift fallibility follows the Rust signature: `Result<T, E>` becomes `throws`,
and `T` becomes nonthrowing. This applies to constructors, getters, and async
methods. Native Rust panics abort at the non-unwinding C boundary, including
panics during future polling; they never become catchable Swift errors.
Conversion failures in nonthrowing calls trap. Throwing calls can report both
ordinary Rust errors and conversion failures; exported error values retain their
cases and payloads. An error enum used by
`Result<T, E>` gains Swift `Error` conformance automatically. Native return
classification follows wasm-bindgen's `ReturnWasmAbi`/descriptor approach:
Rust traits resolve the return type, including `Result` aliases, before Swift
fallibility is rendered. Opaque `bridgerton::Error` messages stay `BridgeError`;
scalar errors such as `String` become `BridgeFailure<String>`. Standard
`std::io::Error` becomes `IoError` with a portable `IoErrorKind`, diagnostic
message, and optional OS code. Destructor failures are fatal. Callback decoding
failures return an error to the Rust caller. The WASM path keeps wasm-bindgen's panic behavior.

JavaScript receives the same information: every error is thrown as an `Error`
whose `message` is the Rust `Display` text, and a typed error also carries a
`detail` property with the same shape Swift receives. `std::io::Error` throws
`Error & { detail: IoError }`; a bridged error enum throws
`Error & { detail: { type: "Case"; ...fields } }`, declared in the generated
TypeScript under the enum's name; a transparent value used as an error carries
that value as `detail`. A raw `JsValue` error is rethrown as is.

Error enums with opaque source fields use `#[bridge(error)]`:

```rust
#[bridge(error)]
pub enum LoadError {
    Http(u16),
    InvalidData(String),
    Io(#[bridge(message)] std::io::Error),
}
```

Swift receives the same cases; ordinary fields keep their value types, while
`message` explicitly transports an opaque field's `Display` text (or, on the
web, a raw JavaScript error value) as `String`. This is outbound error
transport: no Rust error reconstruction or second DTO is needed. Existing
Serde/thiserror behavior is untouched. This mode currently requires a
non-generic enum without conditional fields/variants. A normal bridged value enum also works
as an error, including its ordinary generic/value support.

Native owned argument transport is also type-directed. Aliases of callbacks,
optional callbacks, objects, optional objects, and values retain their behavior; aliased borrowed objects
resolve to the original generated class. Borrowed value arguments receive a
Rust diagnostic requesting an owned value. The JavaScript adaptation still has
syntactic handling for collections, optionals, and fallible returns; arbitrary
aliases in that adaptation are not yet interchangeable with direct syntax.

Generated Swift automatically checks a fingerprint before its first application
call. A fixed, versioned C handshake compares the generated interface and native
codec/runtime implementation with the linked library. Stale bindings fail before
passing application handles or arguments. The fingerprint is conservative: it is
not an ABI compatibility promise, and bindings should be regenerated with their
library. Tests link old Swift against a changed Rust record layout with unchanged
C symbols and verify that rejection precedes the constructor.

Swift task cancellation drops a throwing Rust future and throws
`CancellationError`. Nonthrowing async methods keep running to supply their
result even if the Swift task is cancelled. Signal-aware nonthrowing methods can
instead stop cooperatively and return normally. An owned `AbortSignal` or
`Option<AbortSignal>` argument automatically receives a child signal connected
to Swift task cancellation; cancelling one task never aborts its caller's
controller or sibling calls. Optional signals default to `nil` in Swift, and
still receive task cancellation when omitted. Rust calls `.aborted()` at safe
boundaries or `.until(future).await` for an interruptible wait.

On the web these arguments accept real browser `AbortSignal` values, including
pre-aborted signals and shared controllers. `.until` removes its browser event
listener on completion, cancellation, or drop. The small `abort-signal` crate
shared with fetch-happen owns the platform mechanism (Tokio cancellation tokens
natively), while bridgerton owns only its language bindings.

For example, Swift can call Yap's nonthrowing prefetch API without managing a token:

```swift
let work = Task { @MainActor in
    await deck.cache_challenge_audio(banned_challenge_types: [], access_token: nil)
}
work.cancel()
```

Explicit controllers remain useful for stopping several operations together:
pass `controller.signal()` in Swift or `controller.signal` in JavaScript, then
call `controller.abort()`. Aborting a controller does not preempt synchronous
Rust code; the operation must yield and cooperate. Dropping a controller alone
does not abort it.

The bridge has no timer API or built-in executor. The fixture uses `futures-timer`
on native (a shared timer driver) and `gloo-timers` on WASM to exercise background
wakeups. Native Tokio integration is built in: supply the host-owned
`tokio::runtime::Handle` through `bridgerton::native::set_tokio_handle`.
Install it once on the main thread
before using Tokio-dependent APIs. The host chooses worker count, enabled drivers,
and shutdown, and must keep the runtime alive and driven until all such work is
finished. A handle alone does not keep the drivers alive. Replacing an installed
handle is rejected so suspended operations cannot switch runtimes.

When a handle is installed, the bridge enters its context during native calls,
polls, and drops, restoring the prior context on return (including reentrant calls).
It never moves confined futures onto Tokio workers. Binding generation needs no
runtime setup. The real Yap integration exercises native file I/O and background
pack loading; authenticated networking and other CPU-intensive APIs still need testing.

The [real Yap host](../../prototypes/yap-swift) exercises platform I/O, existing
state and subscriptions, typed event creation, locked native persistence/reopen,
and a running SwiftUI screen. `cargo bridgerton package` produces the
XCFramework; `ForeignValue` and authenticated server sync validation remain deferred. Native
foreign entry points are Apple-only; ordinary Rust APIs and source generation
can compile elsewhere, but calling this native ABI on non-Apple hosts aborts.

`ListenerKey` in Yap is an opaque `#[bridge(opaque)]` object on both platforms. It
requires no slotmap-token codec or native type alias. Returning it or delivering
it in a callback transfers a wrapper to Swift; passing it to `unsubscribe` or
as a sync modifier consumes that wrapper. Rust's internal `Copy` behavior is
unchanged, and no persisted event format is involved.

The runtime's `AbortSignal` intentionally retains its existing non-consuming
argument conversion and task-cancellation wiring. The low-level
`#[bridge(opaque, custom_arguments)]` escape hatch generates its object
wrapper while letting the runtime supply its argument traits. Ordinary application
objects use `#[bridge(opaque)]` and need no such configuration.

Free functions take the same bare `#[bridge]` as impls and are exported on both
platforms: real wasm-bindgen functions for JavaScript and main-actor functions
for Swift. Yap's rule is that every type or function crossing the boundary
carries an unconditional `#[bridge(transparent)]`, `#[bridge(opaque)]`, or
`#[bridge]`; anything else is plain Rust. Application code never gates a bridge
attribute on `target_arch`, and no `JsValue` appears in an exported signature.
Platform utilities the application needs (timers, delays, blocking work,
logging, process environment, and `broadcast` to other instances of the app)
live in `bridgerton::platform`, so application crates contain no browser or
Tokio calls of their own. Bridged methods take `&self`: objects are shared by
reference on both platforms, so keep mutable state in a `Cell` or `RefCell`.