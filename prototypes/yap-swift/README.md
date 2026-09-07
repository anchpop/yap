# Real Yap in SwiftUI

A running macOS SwiftUI screen backed directly by `yap-frontend-rs::Weapon` and
`Deck`, using [Bridgerton](../../libraries/bridgerton). There is no second Rust
application model, native service layer, or handwritten Swift FFI declaration.
The small host crate links the existing Yap crate into native libraries. The
bridge discovers exported objects, impls, and their value types automatically.

## Run

Requires macOS, Swift 6.2+, Rust, and Yap's generated French language pack
(`out/fra_for_eng/language_data_core.rkyv` and `language_data_sentences.rkyv`).
From the repo root:

```sh
python3 prototypes/yap-swift/check.py --open
```

Use `--packs /path/to/packs` for a different asset location. The script builds
Rust, generates Swift types and C declarations, runs the integration test, then
compiles and ad-hoc signs `generated/Yap Prototype.app`. Omit `--open` to test and
build without launching. The harness serves local packs over a loopback HTTP
server using the backend's `/language-data` request format. With `--open`, it
keeps that server running until the app quits; keep the terminal open. Launching
the app separately uses Yap's normal backend configuration. Deployment targets
match the local Mac; this is not yet a redistributable package.

For bindings alone, run the reusable bridge command:

```sh
cargo bridgerton swift --package yap-swift-prototype --out-dir prototypes/yap-swift/generated
```

This builds and loads the host's `cdylib`; Swift links the corresponding
`staticlib`. Yap has no handwritten generator or export registration list.
Synthetic event JSON belongs to the [smoke harness](smoke), independently of
binding generation.

The screen shows today's summary and the daily goal. “Try a 20-minute goal”
constructs a generated Swift `DeckEvent` and calls the existing `add_deck_event` API. Its
subscription rebuilds a `Deck` snapshot and updates SwiftUI through Observation.
This exercises the actual event fold; it is not a complete settings editor.
The app and test each use a fresh temporary data directory. They do not open an
existing profile, authenticate, contact the production backend, or upload events
when run through this harness. Pack downloads contact only the loopback server. App temporary
directories are left to the system's temporary-file cleanup; the test removes its
own directory immediately.

## Integration shape

- `Weapon.create`, stream requests/subscriptions, local storage loading, event
  ingestion, and `get_deck_state` retain their existing flow. `Deck` retains its
  existing state computation. Event definitions and stored serialization are
  unchanged.
- The main `Weapon` impl uses just `#[bridge]`: every public method is
  exported, factories and return transport are inferred, and private helpers
  remain private. `#[bridge(getter)]` preserves JS getters and generates Swift
  properties. WASM uses wasm-bindgen and preserves existing Tsify conversions;
  Swift uses `throws` only for Rust methods returning `Result`, so ordinary
  getters and subscriptions require no `try`. Rust panics terminate the native app.
  Value types use `#[bridge(transparent)]`; object types use
  `#[bridge(opaque)]`. Structs and enums keep explicit Serde derives.
  There is no feature switch: native builds always generate the Swift metadata,
  so every declaration is checked for both platforms. Every exported type or function uses an
  unconditional bridge attribute; nothing in application code is gated on
  `target_arch`, and no exported signature mentions `JsValue`. Types that are
  never exported (versioned event history, backend request bodies, language
  pack internals) carry no binding attribute at all. The complete public UI impl of `Deck`
  now uses plain `#[bridge]`, including review/action methods. The shared pack-loader impl uses
  plain `#[bridge]`, with no part label or registration step. `transparent` generates Tsify web bindings and Swift value transport while
  leaving explicit Serde derives and stored representations unchanged. `Weapon`
  and `Deck` each declare `opaque` once; factories and object returns require
  no method-name lists or repeated transport annotations.
- Host callbacks become `Callback<()>` or `Callback<(ListenerKey, String)>`.
  On WASM these accept ordinary JS functions through wasm-bindgen's ABI. On
  native they retain Swift main-actor closures. The JS call sites are unchanged.
  `ListenerKey` is an opaque object on both platforms; no handwritten slotmap
  conversion or Swift integer alias is needed. A callback may retain its key
  and later pass it into `sync`; that call consumes the Swift wrapper, as does
  `unsubscribe` for a subscription key.
- Yap chooses `LocalEventStore`, allowing its native UI listeners to capture
  `Rc` state. Weapon's normal `EventStore` still requires native `Send + Sync`
  listeners, so MCP keeps its existing server model. The implementation is
  shared through a callback-ownership parameter, including storage and sync.
- OPFS's existing native filesystem implementation supplies device IDs and
  event-log reading/writing. `weblocks` now supplies native file locks through
  the same `acquire` call used in browsers; no native lock implementation lives
  in Yap. Pending native lock requests are cancellable by dropping the future,
  and separate processes coordinate through a per-user named-lock directory.
  OPFS owns native app-directory resolution: the host configures `Yap` once
  using `configure_app`, or uses `configure_root` when `YAP_DATA_DIR` is set.
  Shared Yap code simply calls `app_specific_dir().await?` on every platform.
  Desktop paths follow `ProjectDirs`; iOS uses the app container's Application
  Support directory. The default macOS path remains `Application Support/Yap`.
- Both platforms use the same pack-loading flow: chunked cache reads/writes,
  hash validation, core-first loading, progress callbacks, and full-pack upgrades.
  Both download through `fetch_happen`; no local-file source lives in Yap.
  The prototype harness supplies the native `YAP_AI_BACKEND_URL` runtime setting
  before launch, pointing at its temporary loopback fixture server. Native hosts
  read that override once on first use; absent it, the existing feature and
  compile-time URL defaults apply. Native deserialization runs on Tokio's
  blocking pool. The smoke test disables fixture downloads before reopening to
  verify that the shared cache suffices.
- The native host explicitly owns Tokio: `try YapHost.initialize()` is called
  once at Swift startup. The host chooses two workers and a process-long lifetime,
  and installs its handle using the bridge's built-in Tokio support. There is
  no additional feature flag to enable.
  Binding generation creates no runtime. The bridge enters that context during
  native calls and future polls. Confined objects and futures stay on the main actor; only independent
  filesystem work runs on background threads. Shared platform utilities live in
  `bridgerton::platform`: timezone
  lookup, performance timers, async delays, blocking work, logging, and native
  environment lookup. Yap calls these helpers directly; it keeps its backend
  defaults and language-pack decoding logic.

## Checks

The native runner uses strict Swift 6 concurrency checking and verifies:

- Async construction, native filesystem access, and persistent device identity.
- Real French pack loading and returned `Deck` object ownership.
- Generated `Course`, `Language`, `TodaySummary`, and nested records/arrays.
- Both callback signatures, callback reentrancy, and unsubscribe.
- Immutable event folding, typed `DeckEvent`/`DeckSelectionEvent` inputs,
  optional counts, generic sync state, and lossless timestamps.
- Native offline sync, persistence, and reopening the saved deck/selection;
  prior `Deck` snapshots stay unchanged.
- Missing-pack and invalid-timezone errors crossing as Swift errors.
- Core-first pack loading, main-actor progress callbacks, full-pack upgrades,
  and reopening the shared chunk cache with the local asset source unavailable.
- Generated `LanguageDataError` cases with typed payloads (for example,
  `UnsupportedCourse(Course)`); opaque source-error fields carry diagnostic text.
- A real add-card event, returned `CardSummary` and `ReviewInfo` objects, the
  next challenge, and a review event applied through the existing Rust APIs.

The bridge also checks generated Swift against the linked Rust interface before
calling application code. No host-side checksum registration is needed.

For a real browser regression using an isolated Chromium profile:

```sh
CARGO_PROFILE_RELEASE_LTO=true cargo bridgerton web --package yap-frontend-rs --release --target web --out-dir ../prototypes/yap-swift/generated/web --features local-backend
node prototypes/yap-swift/check-web.cjs
```

This checks the actual Yap WASM factory, OPFS, callback ABI, reentrancy,
unsubscribe, device identity, and portable errors. It uses Yap's installed
Playwright. The [bridge's own check script](../../libraries/bridgerton/check.py) additionally
checks cancellation, ownership, malformed inputs, Node, and TypeScript.

## Still ahead

This is an offline integration slice, not a complete native Yap client. The
native smoke test now drives a review through the existing `Deck` APIs and
`Weapon.add_deck_event`. The next UI slice is a SwiftUI review screen consuming
these generated types. Native account/authenticated server-sync orchestration
and validation, audio playback, and iOS
packaging/device testing remain. The prototype uses local assets through the
shared progressive loader; native HTTP streaming is tested against an isolated
local server, while authenticated production networking remains unvalidated. General
`ForeignValue` interop is not needed for this slice and remains unimplemented.

Yap pins OPFS and weblocks to Git revisions in the workspace `Cargo.toml`;
builds fetch the native implementations without needing local checkouts. Lock
metadata lives in the per-user local-data `weblocks` directory, independently
of the temporary test event store; those lock files must not be unlinked while
any participant may be using them.

The binding library still needs a broader correctness review before production.
It deliberately supports a limited signature surface; see its README for ABI
ownership requirements and limitations.

Native Deck also exports `cache_challenge_audio`. Its optional portable abort
signal accepts an explicit `AbortController().signal()` and is automatically
connected to Swift task cancellation even when omitted. Cancellation returns
normally and skips cleanup, preserving the cache. Already shared audio downloads
and cache writes finish so stopping background prefetch cannot interrupt playback;
no further request starts after cancellation is observed. The delay and cache
errors now use platform-appropriate implementations.

The full Deck UI interface retains its original object/value distinction:
`ReviewInfo`, `CardSummary`, `LockupOffer`, and `ReleaseOffer` remain objects with
getters; challenge content, dictionary/pronunciation data, and statistics are
generated values. The integration test exercises a real French add-card event,
arrays of card-summary objects, a returned review-info object, its next challenge,
and the resulting review event. Internal Rust-only accessor impls remain ordinary
Rust impls. Event serialization and review logic are unchanged.
