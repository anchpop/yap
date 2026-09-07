import Foundation
import BridgeFFI
import SwiftUI

// Compile a real SwiftUI view against the generated value types.
struct CardPreview: View {
    let card: Card
    var body: some View {
        VStack {
            Text(card.term.text)
            if let gloss = card.term.gloss { Text(gloss) }
            ForEach(card.alternatives.indices, id: \.self) { i in Text(card.alternatives[i].text) }
            switch card.state {
            case .New: Text("New")
            case .Learning(let steps, let due): Text("\(steps): \(due ?? "unscheduled")")
            case .Known(let note): Text(note)
            case .Pair(let count, let enabled): Text(verbatim: "\(count): \(enabled)")
            }
        }
    }
}

private func require(_ condition: Bool, _ message: String) {
    precondition(condition, message)
}
private func rejects(_ body: () throws -> Void) {
    do { try body(); fatalError("expected invalid value to be rejected") }
    catch is BridgeError {}
    catch { fatalError("unexpected error: \(error)") }
}

@MainActor func testValues(_ counter: Counter) async throws {
    let card = counter.sample_card()
    require(counter.echo_alias(card: card) == card, "aliased value conversion")
    require(counter.maybe_card(card: card) == card && counter.maybe_card(card: nil) == nil, "optional value input/output")
    require(counter.cards == [card], "collection getter")
    require(await counter.echo_nested(cards: [[nil, card], []]) == [[nil, card], []], "async nested nullable collections")
    require(card.term.text == "語 🦀" && card.term.gloss == "language", "nested UTF-8 record")
    require(card.alternatives == [Term(text: "言葉", gloss: nil)], "optional field")
    require(card.state == .New && card.tags == ["日本語", ""], "enum and array")
    let state = ReviewState.Learning(steps: UInt32.max, due: nil)
    let revised = try counter.revise_card(card: card, state: state)
    require(revised.id == card.id + 1 && revised.state == state && revised.starred, "two value inputs")
    require(card.state == .New && !card.starred, "value semantics preserve caller input")
    require(counter.echo_cards(cards: [nil, revised, card]) == [nil, revised, card], "nested collections")
    require(counter.echo_cards(cards: []) == [], "empty collection")
    for state in [ReviewState.New, .Learning(steps: 0, due: "tomorrow"), .Known("語"), .Pair(7, false)] {
        require(counter.echo_state(state: state) == state, "all enum payload forms")
    }
    let renamed = counter.rename_card(card: card, text: "a\0b")
    require(renamed.term.text == "a\0b", "embedded NUL and String input")
    let token = AbortController()
    let later = try await counter.card_later(card: revised, cancellation: token.signal())
    require(later.id == revised.id + 1 && later.state == .Known("remembered"), "async owned data")
    let cancelled = AbortController()
    cancelled.abort()
    do {
        _ = try await counter.card_later(card: revised, cancellation: cancelled.signal())
        fatalError("expected data future cancellation")
    } catch let error as BridgeError { require(error.description == "operation aborted", "data future cancellation") }
    var invalid = card
    invalid.term.text = ""
    rejects { _ = try counter.revise_card(card: invalid, state: .New) }
    // Plain data can cross actors without touching confined Rust objects.
    let copied = await Task.detached { revised }.value
    require(copied == revised, "generated data conforms to Sendable")
    _ = CardPreview(card: copied)
    try testCodec()
    let tree = Tree.Record(TreeRecord(child: .Next(.Pair(BridgePair(.Leaf(7), true)))))
    require(counter.echo_tree(tree: tree) == tree, "recursive enum through a record, optional, and pair")
    let mutual = MutualA.Next(.Next(.End))
    require(counter.echo_mutual(tree: mutual) == mutual, "mutually recursive enum layout")
    let chain = Chain_UInt32.Link(1, .Link(2, .End))
    require(counter.echo_chain(chain: chain) == chain, "generic recursive enum layout")
    let collection = CollectionTree.Map(["nested": .Children([.Set([.Leaf(3)])])])
    require(counter.echo_collection_tree(tree: collection) == collection, "recursive collections need no extra enum indirection")
    print("PASS: generated Swift records/enums, nested arrays/options, all enum forms, sync/async roundtrips, SwiftUI view, and malformed values")
}

@MainActor private func testCodec() throws {
    let value: [String]? = ["語"]
    var writer = BridgeWriter()
    try value.bridgeWrite(&writer)
    let expected: [UInt8] = [1, 0,0,0,1, 0,0,0,3, 0xe8,0xaa,0x9e]
    require(writer.bytes == expected, "shared wire-format golden vector")
    for end in 0..<expected.count {
        rejects {
            var reader = BridgeReader(data: Data(expected.prefix(end)))
            _ = try Optional<Array<String>>.bridgeRead(&reader)
        }
    }
    var mapWriter = BridgeWriter()
    let map: [UInt32: UInt16?] = [1: 2]
    try map.bridgeWrite(&mapWriter)
    require(mapWriter.bytes == [0,0,0,1, 0,0,0,1, 1,0,2], "map wire vector matches Rust")
    var mapReader = BridgeReader(data: Data(mapWriter.bytes))
    require(try Dictionary<UInt32, UInt16?>.bridgeRead(&mapReader) == map, "optional map values")
    rejects { var r = BridgeReader(data: Data([0,0,0,2, 0,0,0,1, 0, 0,0,0,1, 0])); _ = try Dictionary<UInt32, UInt16?>.bridgeRead(&r) }
    rejects { var r = BridgeReader(data: Data([0,0,0,2, 0,0,0,1, 0,0,0,1])); _ = try Set<UInt32>.bridgeRead(&r) }
    for bits: UInt64 in [0, 1 << 63, 0x7ff0000000000000, 0x7ff8000000001234] {
        var w = BridgeWriter()
        try Double(bitPattern: bits).bridgeWrite(&w)
        var r = BridgeReader(data: Data(w.bytes))
        require(try Double.bridgeRead(&r).bitPattern == bits, "floating-point bits survive exactly")
    }
    var pairWriter = BridgeWriter()
    try BridgePair(UInt8(1), Int16(-2)).bridgeWrite(&pairWriter)
    require(pairWriter.bytes == [1,255,254], "pair wire vector matches Rust")
    var timestampWriter = BridgeWriter()
    let timestamp = BridgeTimestamp(seconds: -1, nanoseconds: 123_456_789)
    try timestamp.bridgeWrite(&timestampWriter)
    require(timestampWriter.bytes == [255,255,255,255,255,255,255,255, 7,91,205,21], "timestamp wire vector matches Rust")
    rejects { var r = BridgeReader(data: Data([2])); _ = try Bool.bridgeRead(&r) }
    rejects { var r = BridgeReader(data: Data([2])); _ = try Optional<UInt32>.bridgeRead(&r) }
    rejects { var r = BridgeReader(data: Data([0,0,0,1,0xff])); _ = try String.bridgeRead(&r) }
    rejects { var r = BridgeReader(data: Data([0xff,0xff,0xff,0xff])); _ = try Array<UInt32>.bridgeRead(&r) }
    rejects { var r = BridgeReader(data: Data([0,0,0,99])); _ = try ReviewState.bridgeRead(&r) }
    rejects { var w = BridgeWriter(); try Array(repeating: false, count: bridgeMaxItems + 1).bridgeWrite(&w) }
    rejects { var w = BridgeWriter(); try Array(repeating: Array(repeating: false, count: 256), count: 256).bridgeWrite(&w) }
    rejects { var r = BridgeReader(data: Data(), depth: bridgeMaxDepth); _ = try r.nested { _ in 0 } }
    rejects { var w = BridgeWriter(depth: bridgeMaxDepth); _ = try w.nested { _ in 0 } }
}
