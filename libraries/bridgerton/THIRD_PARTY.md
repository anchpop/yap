# Borrowed implementation

`src/value.swift` adapts the big-endian integer reader and writer
from Mozilla UniFFI's `uniffi_bindgen/src/bindings/swift/templates/RustBufferTemplate.swift`,
revision `3a2d44a7786d0cb364e446bd96711a0d31dd9904`:

https://github.com/mozilla/uniffi-rs/blob/3a2d44a7786d0cb364e446bd96711a0d31dd9904/uniffi_bindgen/src/bindings/swift/templates/RustBufferTemplate.swift

That file is distributed under the Mozilla Public License 2.0, reproduced in
`licenses/MPL-2.0.txt`. The adapted file retains that license notice. Generated
`Bridge.swift` incorporates this code and its notice; distribute its corresponding
source, including the modified template, under the MPL's terms.

Changes: reader/writer structs in one generated module, checked lengths and
nesting limits, our error type, scoped borrowed input buffers, and our owned
output buffer ABI. No UniFFI runtime, generator, or metadata dependency is used.

The Rust codec and record/enum generation are original implementations informed
by UniFFI's design: big-endian scalars, length-prefixed strings/sequences,
one-byte option tags, field-order records, and one-based enum tags. This is our
own bounded format, not a promise of UniFFI ABI compatibility.

# TypeScript declaration generator

`macros/src/typescript/` adapts the declaration generator from Tsify
(`tsify-macros` 0.5.5, https://github.com/madonoharu/tsify), distributed under
MIT OR Apache-2.0; the MIT license text is reproduced in `licenses/tsify-MIT.txt`.

Changes: options come from the bridge attribute instead of `#[tsify(...)]`,
field options use `#[bridge(type = "...")]` and `#[bridge(optional)]`, the
wasm-bindgen ABI implementations and `#[declare]` macro are omitted (the
bridge's own conversions apply), and the JavaScript serialization defaults
are fixed to serde-wasm-bindgen's.
