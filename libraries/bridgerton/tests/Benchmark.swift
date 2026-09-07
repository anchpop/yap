import Foundation

@main struct Benchmark {
    @MainActor static func main() async throws {
        var results: [String: Double] = [:]
        func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
        func measure(_ name: String, count: Int, _ work: () throws -> Void) rethrows {
            let start = now()
            for _ in 0..<count { try work() }
            results[name] = Double(now() - start) / Double(count) / 1_000
        }
        let start = now()
        let counter = Counter()
        results["cold_interface_us"] = Double(now() - start) / 1_000
        measure("scalar_call_us", count: 100_000) { precondition(counter.value() == 0) }
        let card = counter.sample_card()
        let cards: [Card?] = Array(repeating: card, count: 1_000)
        measure("1000_cards_roundtrip_us", count: 100) {
            let result = counter.echo_cards(cards: cards)
            precondition(result.count == 1_000 && result[999] == card)
        }
        measure("1000_objects_create_and_release_us", count: 100) {
            let objects = many_objects(count: 1_000)
            precondition(objects.count == 1_000)
        }
        precondition(counter.live_counters() == 1)
        var callbacks = 0
        try measure("1000_callbacks_us", count: 100) {
            try emit_many(callback: { _ in callbacks += 1 }, count: 1_000)
        }
        precondition(callbacks == 100_000)
        let before = now()
        for _ in 0..<10_000 { let value = await ready_value(); precondition(value == 42) }
        results["ready_async_us"] = Double(now() - before) / 10_000 / 1_000
        let timer = now()
        for _ in 0..<100 { let text = await text_later(text: "wake"); precondition(text == "wake") }
        results["1ms_timer_with_wakeup_us"] = Double(now() - timer) / 100 / 1_000
        let data = try JSONSerialization.data(withJSONObject: results, options: [.prettyPrinted, .sortedKeys])
        print(String(decoding: data, as: UTF8.self))
    }
}
