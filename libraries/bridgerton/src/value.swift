// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Integer read/write adapted from UniFFI RustBufferTemplate.swift.
// See THIRD_PARTY.md for the exact source revision and modifications.

internal let bridgeMaxBytes = 16 * 1024 * 1024
internal let bridgeMaxItems = 65_536
internal let bridgeMaxDepth = 64

internal struct BridgeReader {
    let data: Data
    var offset = 0
    var depth = 0
    var remainingItems = bridgeMaxItems

    mutating func read<T: BridgeValue>() throws -> T { try T.bridgeRead(&self) }

    mutating func integer<T: FixedWidthInteger>() throws -> T {
        guard MemoryLayout<T>.size <= data.count - offset else {
            throw BridgeError(description: "truncated value")
        }
        let range = offset..<offset + MemoryLayout<T>.size
        var value: T = 0
        _ = withUnsafeMutableBytes(of: &value) { data.copyBytes(to: $0, from: range) }
        offset = range.upperBound
        return value.bigEndian
    }
    mutating func length(limit: Int) throws -> Int {
        let count = Int(try integer() as UInt32)
        guard count <= limit else { throw BridgeError(description: "invalid value length") }
        return count
    }
    mutating func nested<T>(_ body: (inout Self) throws -> T) throws -> T {
        guard depth < bridgeMaxDepth else { throw BridgeError(description: "value exceeds nesting limit") }
        depth += 1
        defer { depth -= 1 }
        return try body(&self)
    }
}

internal struct BridgeWriter {
    var bytes: [UInt8] = []
    var depth = 0
    var remainingItems = bridgeMaxItems
    mutating func put(_ value: [UInt8]) throws {
        guard value.count <= bridgeMaxBytes - bytes.count else {
            throw BridgeError(description: "value exceeds byte limit")
        }
        bytes.append(contentsOf: value)
    }
    mutating func integer<T: FixedWidthInteger>(_ value: T) throws {
        var value = value.bigEndian
        try withUnsafeBytes(of: &value) { try put(Array($0)) }
    }
    mutating func length(_ count: Int, limit: Int) throws {
        guard count <= limit else { throw BridgeError(description: "invalid value length") }
        try integer(UInt32(count))
    }
    mutating func nested<T>(_ body: (inout Self) throws -> T) throws -> T {
        guard depth < bridgeMaxDepth else { throw BridgeError(description: "value exceeds nesting limit") }
        depth += 1
        defer { depth -= 1 }
        return try body(&self)
    }
}

internal protocol BridgeValue: BridgeReturn {
    static func bridgeRead(_ reader: inout BridgeReader) throws -> Self
    func bridgeWrite(_ writer: inout BridgeWriter) throws
}

extension UInt32: BridgeValue {
    @MainActor internal static func bridgeReceive(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> Self { try bridgeNumber(result) }
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self { try reader.integer() }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws { try writer.integer(self) }
}
extension Bool: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        switch try reader.integer() as UInt8 {
        case 0: return false
        case 1: return true
        default: throw BridgeError(description: "invalid boolean tag")
        }
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws { try writer.integer(UInt8(self ? 1 : 0)) }
}
extension String: BridgeValue {
    @MainActor internal static func bridgeReceive(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> Self {
        guard result.data.len <= context.remainingBytes else {
            bridgerton_buffer_free(result.data)
            throw BridgeError(description: "value exceeds byte limit")
        }
        context.remainingBytes -= result.data.len
        return try bridgeString(result)
    }
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        let count = try reader.length(limit: bridgeMaxBytes)
        guard count <= reader.data.count - reader.offset else { throw BridgeError(description: "truncated value") }
        let range = reader.offset..<reader.offset + count
        guard let value = String(data: reader.data.subdata(in: range), encoding: .utf8) else {
            throw BridgeError(description: "invalid UTF-8")
        }
        reader.offset = range.upperBound
        return value
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {
        let bytes = Array(utf8)
        try writer.length(bytes.count, limit: bridgeMaxBytes)
        try writer.put(bytes)
    }
}
extension Optional: BridgeValue where Wrapped: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        try reader.nested { reader in
            if try Bool.bridgeRead(&reader) { return try Wrapped.bridgeRead(&reader) }
            return nil
        }
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {
        try writer.nested { writer in
            try (self != nil).bridgeWrite(&writer)
            if let value = self { try value.bridgeWrite(&writer) }
        }
    }
}
extension Array: BridgeValue where Element: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        try reader.nested { reader in
            let count = try reader.length(limit: reader.remainingItems)
            reader.remainingItems -= count
            return try (0..<count).map { _ in try Element.bridgeRead(&reader) }
        }
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {
        try writer.nested { writer in
            try writer.length(count, limit: writer.remainingItems)
            writer.remainingItems -= count
            for value in self { try value.bridgeWrite(&writer) }
        }
    }
}

// Input bytes are borrowed only during the C call. Rust fully decodes them before
// returning a task, so no Swift allocation is retained by a suspended Rust future.
@MainActor internal func withBridgeValue<T: BridgeValue, R>(
    _ value: T, _ body: (BridgeBytes) throws -> R
) throws -> R {
    var writer = BridgeWriter()
    try value.bridgeWrite(&writer)
    return try writer.bytes.withUnsafeBufferPointer { bytes in
        try body(BridgeBytes(data: bytes.baseAddress, len: bytes.count))
    }
}

@MainActor internal func bridgeValue<T: BridgeValue>(_ result: BridgeResult) throws -> T {
    var context = BridgeReturnContext()
    return try bridgeValue(result, context: &context)
}
@MainActor internal func bridgeValue<T: BridgeValue>(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> T {
    defer { bridgerton_buffer_free(result.data) }
    try bridgeCheck(result)
    guard result.data.len <= context.remainingBytes else { throw BridgeError(description: "value exceeds byte limit") }
    let data = result.data.data.map { Data(bytes: $0, count: result.data.len) } ?? Data()
    context.remainingBytes -= data.count
    var reader = BridgeReader(data: data, depth: context.depth, remainingItems: context.remainingItems)
    defer { context.remainingItems = reader.remainingItems }
    let value = try T.bridgeRead(&reader)
    guard reader.offset == data.count else { throw BridgeError(description: "trailing value bytes") }
    return value
}

extension Int32: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self { try reader.integer() }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws { try writer.integer(self) }
}
extension UInt64: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self { try reader.integer() }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws { try writer.integer(self) }
}

extension BridgeValue {
    @MainActor internal static func bridgeReceive(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> Self {
        try bridgeValue(result, context: &context)
    }
    @MainActor internal static func bridgeReceiveArray(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> [Self] {
        try bridgeValue(result, context: &context)
    }
    @MainActor internal static func bridgeReceiveOptional(_ result: BridgeResult, context: inout BridgeReturnContext) throws -> Self? {
        try bridgeValue(result, context: &context)
    }
}

extension UInt8: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try r.integer()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try w.integer(self)}
}
extension UInt16: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try r.integer()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try w.integer(self)}
}
extension Int8: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try r.integer()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try w.integer(self)}
}
extension Int16: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try r.integer()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try w.integer(self)}
}
extension Int64: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try r.integer()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try w.integer(self)}
}
extension Double: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {Self(bitPattern: try UInt64.bridgeRead(&r))}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try bitPattern.bridgeWrite(&w)}
}
extension Float: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {Self(bitPattern: try UInt32.bridgeRead(&r))}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try bitPattern.bridgeWrite(&w)}
}
extension Dictionary: BridgeValue, BridgeReturn where Key: BridgeValue, Value: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        try reader.nested { reader in
            let count = try reader.length(limit: reader.remainingItems)
            reader.remainingItems -= count
            var result = Self()
            for _ in 0..<count {
                let key = try Key.bridgeRead(&reader)
                let value = try Value.bridgeRead(&reader)
                guard result.updateValue(value, forKey: key) == nil else {throw BridgeError(description: "duplicate map key")}
            }
            return result
        }
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {
        try writer.nested { writer in
            guard count <= writer.remainingItems else {throw BridgeError(description: "invalid map length")}
            writer.remainingItems -= count
            try UInt32(count).bridgeWrite(&writer)
            for (key, value) in self {try key.bridgeWrite(&writer); try value.bridgeWrite(&writer)}
        }
    }
}
extension Set: BridgeValue, BridgeReturn where Element: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        let items = try Array<Element>.bridgeRead(&reader)
        let result = Self(items)
        guard result.count == items.count else {throw BridgeError(description: "duplicate set element")}
        return result
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {try Array(self).bridgeWrite(&writer)}
}
public struct BridgePair<A: Hashable & Sendable, B: Hashable & Sendable>: Hashable, Sendable {
    public var first: A
    public var second: B
    public init(_ first: A, _ second: B) {self.first = first; self.second = second}
}
extension BridgePair: BridgeValue, BridgeReturn where A: BridgeValue, B: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try Self(A.bridgeRead(&r), B.bridgeRead(&r))}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try first.bridgeWrite(&w); try second.bridgeWrite(&w)}
}
public struct BridgeUnit: Hashable, Sendable { public init() {} }
extension BridgeUnit: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {Self()}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {}
}
/// Lossless UTC timestamp; Date is a convenience view with floating-point precision.
public struct BridgeTimestamp: Hashable, Sendable {
    public var seconds: Int64
    public var nanoseconds: UInt32
    public init(seconds: Int64, nanoseconds: UInt32) {self.seconds = seconds; self.nanoseconds = nanoseconds}
    public var date: Date {Date(timeIntervalSince1970: Double(seconds) + Double(nanoseconds) / 1_000_000_000)}
}
extension BridgeTimestamp: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self {try Self(seconds: Int64.bridgeRead(&r), nanoseconds: UInt32.bridgeRead(&r))}
    internal func bridgeWrite(_ w: inout BridgeWriter) throws {try seconds.bridgeWrite(&w); try nanoseconds.bridgeWrite(&w)}
}

/// A Rust error payload that is a scalar/collection instead of an error enum.
public struct BridgeFailure<Value: Hashable & Sendable>: Swift.Error {
    public let value: Value
}
extension BridgeFailure: BridgeValue, BridgeReturn where Value: BridgeValue {
    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {
        Self(value: try Value.bridgeRead(&reader))
    }
    internal func bridgeWrite(_ writer: inout BridgeWriter) throws { try value.bridgeWrite(&writer) }
}

public struct BridgeTriple<A: Hashable & Sendable, B: Hashable & Sendable, C: Hashable & Sendable>: Hashable, Sendable {
    public var first: A
    public var second: B
    public var third: C
    public init(_ first: A, _ second: B, _ third: C) { self.first = first; self.second = second; self.third = third }
}
extension BridgeTriple: BridgeValue, BridgeReturn where A: BridgeValue, B: BridgeValue, C: BridgeValue {
    internal static func bridgeRead(_ r: inout BridgeReader) throws -> Self { try Self(A.bridgeRead(&r), B.bridgeRead(&r), C.bridgeRead(&r)) }
    internal func bridgeWrite(_ w: inout BridgeWriter) throws { try first.bridgeWrite(&w); try second.bridgeWrite(&w); try third.bridgeWrite(&w) }
}

// Swift records need explicit storage indirection for a Rust Box<Record> cycle.
// Replacing the enum value on mutation preserves ordinary struct value semantics.
@propertyWrapper public struct BridgeIndirect<Value: Hashable & Sendable>: Hashable, Sendable {
    private indirect enum Storage: Hashable, Sendable { case value(Value) }
    private var storage: Storage
    public var wrappedValue: Value {
        get { switch storage { case .value(let value): return value } }
        set { storage = .value(newValue) }
    }
    public init(wrappedValue: Value) { storage = .value(wrappedValue) }
}
