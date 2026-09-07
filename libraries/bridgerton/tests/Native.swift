import Foundation
import BridgeFFI

@MainActor private func check(_ condition: Bool, _ message: String) {
    if !condition { fatalError(message) }
}

private struct RejectSecondObject: BridgeReturn {
    let counter: Counter
    @MainActor static func bridgeReceive(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> Self {
        let counter = try Counter.bridgeReceive(result, context: &context)
        if counter.value() == 2 { throw BridgeError(description: "intentional object decode failure") }
        return Self(counter: counter)
    }
}

@MainActor private final class Witness {
    static var alive = 0
    init() { Self.alive += 1 }
    isolated deinit { Self.alive -= 1 }
}

@MainActor private func installObserver(_ counter: Counter) {
    let witness = Witness()
    counter.observe { [weak counter, witness] value in
        _ = witness
        check(Thread.isMainThread, "callback must run on the main thread")
        check(counter!.value() == value, "callback can reenter the same object")
        counter!.clear_observer()
    }
}

@main struct NativeTests {
    @MainActor static func main() async throws {
        check(echo_text(text: "語 🦀") == "語 🦀", "borrowed text alias")
        let text = await text_later(text: "owned across suspension")
        check(text == "owned across suspension", "async borrowed string")
        let sum = await sum_later(values: [1, 2, 3])
        check(sum == 6, "async borrowed slice")
        check(conditional_record(value: ConditionalRecord(present: 42)).present == 42, "conditional record")
        check(conditional_enum(value: .Tuple(7)) == .Tuple(7), "conditional tuple fields and variants")
        check(Selection().selected() == 7, "selection exports constructor and method")
        check(target_configuration().common == 42, "target-specific metadata")
        #if os(iOS)
        check(target_configuration().ios, "iOS-only field")
        #endif
        check(keyword_arguments(type: "value", class: 7) == "value:7", "keyword argument names")
        check(keyword_value(value: actor(type: "keyword")).type == "keyword", "keyword type and field")
        check(Selection().handle() == 23, "public method does not collide with internal handle storage")
        let link = Link(value: 1, next: Link(value: 2, next: nil))
        var copy = echo_link(value: link)
        copy.next?.value = 3
        check(link.next?.value == 2 && copy.next?.value == 3, "recursive records retain value semantics")
        let terms = [Term(text: "語", gloss: nil)]
        let echoedTerms = try await echo_terms(terms: terms)
        check(echoedTerms == terms, "Result/Option/Vec aliases")
        check(echo_numbers(numbers: [0, .max]) == [0, .max], "numeric vector alias")
        if let mode = CommandLine.arguments.dropFirst().first, mode != "--wrong-thread" {
            let counter = Counter()
            // Catching an Err would return normally and fail the subprocess check.
            do {
                switch mode {
                case "--panic": _ = counter.panic_now()
                case "--panic-result": _ = try counter.panic_result()
                case "--panic-async": _ = try await counter.panic_later()
                case "--consumed-object":
                    let owned = Counter()
                    let alias = owned
                    _ = counter.consume(other: owned)
                    _ = alias.value()
                case "--invalid-infallible":
                    _ = counter.echo_cards(cards: Array(repeating: nil, count: bridgeMaxItems + 1))
                default: fatalError("unknown test mode")
                }
            } catch { print("Unexpected recoverable error: \(error)") }
            return
        }
        if CommandLine.arguments.contains("--wrong-thread") {
            // Deliberately bypass Swift isolation through the raw C API.
            let handle = bridgerton_counter_new().handle!
            let bits = UInt(bitPattern: handle)
            await Task.detached {
                _ = bridgerton_counter_value(UnsafeRawPointer(bitPattern: bits))
            }.value
            fatalError("the Rust boundary should have rejected this call")
        }

        var created: Counter? = try await Counter.create(initial: 42)
        check(created!.value() == 42, "async factory returns an owned object")
        created = nil
        do {
            do { _ = try FallibleFactory(); fatalError("expected constructor error") }
            catch ReviewError.Offline {}
            let probe = Counter()
            do { _ = try probe.checked_value; fatalError("expected getter error") }
            catch ReviewError.Offline {}
            let card = try probe.typed_result(fail: false)
            check(card == probe.sample_card(), "aliased Result success")
            do { _ = try probe.typed_result(fail: true); fatalError("expected typed error") }
            catch ReviewError.InvalidAnswer(let term, let attempts) {
                check(term == "語" && attempts == 2, "typed error payload")
            }
            do { try await probe.typed_result_later(); fatalError("expected async typed error") }
            catch ReviewError.Rejected(let rejected) { check(rejected == card, "async error record payload") }
            do { try probe.io_result(); fatalError("expected IO error") }
            catch let error as IoError { check(error.kind == .NotFound && error.message == "missing test pack", "portable IO error") }
            do { try probe.source_result(); fatalError("expected source error") }
            catch SourceError.Read(let message) { check(message == "opaque source", "opaque error field") }
            do { try probe.string_error(); fatalError("expected string error") }
            catch let error as BridgeFailure<String> { check(error.value == "plain error", "scalar error payload") }
            do { try probe.named_error(); fatalError("expected Error enum") }
            catch Error.Offline {}
            probe.optional_progress(progress: nil)
            var progressCalls = 0
            probe.optional_progress { message, percent in
                check(message == "Loading" && percent == 50, "optional callback payload")
                progressCalls += 1
            }
            probe.aliased_observer { value in check(value == probe.value(), "aliased callback") }
            check(progressCalls == 1 && probe.aliased_object(other: probe) == probe.value(), "aliased object")
        }
        let counter = Counter()
        check(counter.platform_value() == 42, "native conditional signature")
        check(counter.conditional_getter == 17, "cfg_attr activates getter annotation")
        try await testValues(counter)
        let wire = Envelope_WireState(value: WireState.InProgress(completed: 3))
        check(counter.echo_wire_state(value: wire) == wire, "generic transparent record and tagged enum")
        check(counter.echo_wire_id(value: WireId(value: 42)).value == 42, "explicit serde transparent record")

        check(counter.live_counters() == 1, "initial object count")
        installObserver(counter)
        check(Witness.alive == 1, "Rust retains the Swift closure")
        check(try counter.add(amount: 7) == 7, "sync result")
        check(Witness.alive == 0, "callback is released after reentrant unregister")
        check(counter.label() == "Yap 語 — 7", "UTF-8 result")
        let snapshot = counter.snapshot()
        let laterSnapshot = try await counter.snapshot_later()
        let made = await Snapshot.create(value: 91)
        check(snapshot.value() == 7 && laterSnapshot.value() == 7, "inferred object return transport")
        check(made.value() == 91, "factory without constructor")
        check(made.doubled == 182, "getter through the unified attribute")


        do {
            _ = try counter.fail()
            fatalError("expected Rust error")
        } catch let error as BridgeError {
            check(error.description == "intentional error: 日本語", "typed error transport")
        }

        let triple = BridgeTriple(UInt32(42), "three", true)
        guard case .TripleValue(let value) = counter.triple_value(value: TripleValue(parts: triple)) else { fatalError("wrong case") }
        check(value.parts == triple, "three-field tuple and same-name enum case")
        counter.observe_three { number, text, flag in check(number == 42 && text == "three" && flag, "three-argument callback") }
        do { _ = try counter.return_budget(); fatalError("expected aggregate item limit") }
        catch let error as BridgeError { check(error.description.contains("length"), "nested return budget is shared") }

        check(counter.echo_bytes(bytes: [0, 128, 255]) == [0, 128, 255], "byte values")
        check(counter.nested_bytes() == [[0, 128, 255], []], "nested byte values")
        let liveBefore = counter.live_counters()
        let owned = Counter()
        _ = try owned.add(amount: 19)
        check(counter.consume(other: owned) == 19, "by-value object argument")
        check(counter.live_counters() == liveBefore, "consumed wrapper does not retain Rust object")
        check(counter.consume_optional(other: nil) == nil, "absent object argument")
        check(counter.consume_optional(other: Counter()) == 0, "optional object argument")
        check(try await counter.consume_later(other: Counter()) == 0, "async owns moved object")
        var saved: Counter?
        try counter.emit_object { object in
            saved = object
            check(counter.value() == 7, "object callback can reenter its sender")
        }
        check(saved!.value() == 0 && counter.live_counters() == liveBefore + 1, "callback object outlives invocation")
        check(counter.consume(other: saved!) == 0, "callback object can be passed back by value")
        saved = nil
        try counter.emit_objects { first, label, last in
            check(label == "objects", "mixed callback payload")
            check(counter.consume(other: first) == 0, "consume inside callback")
            saved = last
        }
        check(saved!.value() == 0, "second callback object retained")
        saved = nil
        try counter.emit_optional_object(present: false) { check($0 == nil, "nil callback object") }
        try counter.emit_optional_object(present: true) { saved = $0 }
        saved = nil
        do {
            try counter.emit_invalid_objects { _, _, _ in fatalError("invalid callback must not run") }
            fatalError("expected callback decoding error")
        } catch let error as BridgeError { check(error.description.contains("decode"), "callback decode failure reaches Rust caller") }
        check(counter.live_counters() == liveBefore, "partial callback failure releases decoded and unclaimed objects")
        do {
            try counter.emit_large_strings { _, _ in fatalError("oversized callback must not run") }
            fatalError("expected aggregate callback byte limit")
        } catch let error as BridgeError { check(error.description.contains("decode"), "callback strings share the byte budget") }


        var objects: [Counter]? = counter.object_list()
        check(objects!.map { $0.value() } == [1, 2, 3], "returned object array")
        withExtendedLifetime(objects) {
            check(counter.live_counters() == liveBefore + 3, "Swift owns every returned object")
        }
        objects = nil
        check(counter.live_counters() == liveBefore, "array destruction releases all handles")
        var optional: Counter? = counter.optional_object(present: true)
        check(optional?.value() == 0, "optional object")
        optional = nil
        check(counter.optional_object(present: false) == nil, "absent object")
        var nested: [Counter?]? = counter.nested_objects()
        check(nested!.count == 2 && nested![0] == nil && nested![1]?.value() == 0, "nested optional objects")
        nested = nil
        var laterObjects: [Counter]? = try await counter.objects_later()
        check(laterObjects!.count == 3, "async object array")
        laterObjects = nil
        check(counter.nested_values() == [["one"], [], ["two"]], "nested values retain their value transport")
        check(counter.live_counters() == liveBefore, "nested and async handles released")
        let rawOwner = bridgerton_counter_new().handle!
        let rawObjects = bridgerton_counter_object_list(rawOwner)
        _ = bridgerton_counter_free(rawOwner)
        var returnContext = BridgeReturnContext()
        do {
            _ = try [RejectSecondObject].bridgeReceive(rawObjects, context: &returnContext)
            fatalError("expected object decoding failure")
        } catch let error as BridgeError { check(error.description == "intentional object decode failure", "decode error propagates") }
        check(counter.live_counters() == liveBefore, "decoding failure releases claimed and unclaimed objects")

        // Nonthrowing async methods still supply their result after cancellation.
        let precancelled = Task { @MainActor in await counter.value_later(milliseconds: 1) }
        precancelled.cancel()
        check(await precancelled.value == 7, "precancelled nonthrowing call finishes")
        let nonthrowing = Task { @MainActor in await counter.value_later(milliseconds: 30) }
        try await Task.sleep(for: .milliseconds(5))
        check(counter.active_operations() == 1, "nonthrowing call suspended")
        nonthrowing.cancel()
        check(await nonthrowing.value == 7, "cancelled nonthrowing call finishes")
        check(counter.active_operations() == 0, "nonthrowing future state released")

        // Nonthrowing signal-aware methods return normally, including pre-cancelled tasks.
        let alreadyStopped = Task { @MainActor in await counter.abortable_wait(milliseconds: 60_000) }
        alreadyStopped.cancel()
        check(await alreadyStopped.value, "pre-cancelled task supplies an aborted signal")
        let parent = AbortController()
        let signal = parent.signal()
        let stopped = Task { @MainActor in await counter.abortable_wait(milliseconds: 60_000, signal: signal) }
        let sibling = Task { @MainActor in await counter.abortable_wait(milliseconds: 40, signal: signal) }
        try await Task.sleep(for: .milliseconds(5))
        await Task.detached { stopped.cancel() }.value
        check(await stopped.value, "Swift cancellation reaches nonthrowing Rust")
        check(!signal.aborted(), "task cancellation does not cancel parent")
        check(!(await sibling.value), "sibling sharing the parent finishes normally")
        let grouped = (0..<3).map { _ in Task { @MainActor in
            await counter.abortable_wait(milliseconds: 60_000, signal: signal)
        } }
        try await Task.sleep(for: .milliseconds(5))
        parent.abort()
        for job in grouped { check(await job.value, "parent abort reaches all children") }
        check(counter.active_operations() == 0, "cooperative cancellation releases futures")
        check(!(await counter.abortable_wait(milliseconds: 1)), "omitting a signal still completes")

        let token = AbortController()
        let after = try await counter.add_later(amount: 3, milliseconds: 10, cancellation: token.signal())
        check(after == 10, "non-Send future resumes after background wake")
        check(counter.active_operations() == 0, "completed future drops its local state")

        let cancelledToken = AbortController()
        let explicit = Task { @MainActor in
            try await counter.add_later(amount: 100, milliseconds: 60, cancellation: cancelledToken.signal())
        }
        try await Task.sleep(for: .milliseconds(5))
        check(counter.active_operations() == 1, "operation suspended")
        cancelledToken.abort()
        cancelledToken.abort()
        do { _ = try await explicit.value; fatalError("expected cancellation") }
        catch let error as BridgeError { check(error.description == "operation aborted", "portable cancellation") }
        check(counter.active_operations() == 0, "explicit cancellation drops future state")

        let automatic = Task { @MainActor in
            try await counter.add_later(amount: 100, milliseconds: 60, cancellation: token.signal())
        }
        try await Task.sleep(for: .milliseconds(5))
        automatic.cancel()
        do { _ = try await automatic.value; fatalError("expected Swift cancellation") }
        catch is CancellationError {}
        check(counter.active_operations() == 0, "Swift cancellation drops future on main actor")
        check(counter.value() == 10, "cancelled tasks did not mutate state")

        // Interleave multiple !Send operations on one object.
        let jobs = (0..<24).map { index in
            Task { @MainActor in
                try await counter.add_later(amount: 1, milliseconds: UInt32(index % 5), cancellation: token.signal())
            }
        }
        for job in jobs { _ = try await job.value }
        check(counter.value() == 34, "all interleaved operations completed")
        check(counter.active_operations() == 0, "no remaining future state")

        // Swift 6.2 isolated deinit must schedule release on main, even for a last off-actor reference.
        var releasedElsewhere: Counter? = Counter()
        weak let weakObject = releasedElsewhere
        let release = Task.detached { [object = releasedElsewhere!] in
            try? await Task.sleep(for: .milliseconds(10))
            withExtendedLifetime(object) {}
        }
        releasedElsewhere = nil
        await release.value
        for _ in 0..<100 where weakObject != nil {
            try await Task.sleep(for: .milliseconds(1))
        }
        check(weakObject == nil, "isolated destruction finished")
        check(counter.live_counters() == 1, "Rust object destroyed on its original thread")

        // Allow any queued timer notifications to drain after task cleanup.
        try await Task.sleep(for: .milliseconds(80))
        check(counter.value() == 34, "late wakeups are harmless")
        print("PASS: Swift fallibility matches Rust, callbacks, UTF-8, recoverable errors, non-Send async, cancellation policies, interleaving, and destruction")
    }
}
