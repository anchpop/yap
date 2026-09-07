#!/usr/bin/env python3
"""Check an independent consumer crate: declarations, conditional methods, and rejected modes."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = '''use bridgerton::bridge;
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[bridge(transparent)]
struct Payload { value: u32 }
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[bridge(transparent)]
enum Choice { Value(Payload) }
#[bridge(opaque)]
struct Probe;
#[bridge(opaque)]
struct ReturnedOnly;
#[bridge]
impl Probe {
    pub fn value(&self) -> Choice { Choice::Value(Payload { value: 42 }) }
    #[cfg_attr(all(), bridge(skip))]
    pub fn identity<T>(&self, value: T) -> T { value }
    #[cfg(any())]
    pub unsafe fn missing<T>(&self, value: MissingType<T>) -> MissingType<T> { value }
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
    pub fn nested_missing(&self) -> MissingType { unreachable!() }
}
#[bridge]
impl Probe {
    pub fn make(&self) -> ReturnedOnly { ReturnedOnly }
}
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[cfg(feature = "extra")]
#[bridge(transparent)]
struct FeaturePayload { value: u32 }
#[bridge]
impl Probe {
    #[cfg(all())]
    #[cfg(feature = "extra")]
    #[cfg_attr(all(), bridge(constructor))]
    pub fn new(value: FeaturePayload) -> Self { let _ = value; Self }
    #[cfg_attr(feature = "extra", cfg(any()), allow(dead_code))]
    #[bridge(constructor)]
    pub fn new() -> Self { Self }
    #[cfg_attr(all(), cfg_attr(all(), cfg(feature = "extra")))]
    pub fn feature_payload(&self) -> FeaturePayload { FeaturePayload { value: 7 } }
    #[cfg_attr(any(), bridge(skip))]
    pub fn always(&self) -> u32 { 1 }
}
#[bridge(only(selected))]
impl Probe {
    #[cfg_attr(all(), cfg(feature = "extra"))]
    pub fn selected(&self) -> u32 { 2 }
    #[cfg_attr(all(), bridge(skip))]
    pub fn private_generic<T>(&self, value: T) -> T { value }
}
fn main() {
    let Choice::Value(value) = Probe.value();
    assert_eq!(Probe.identity(value.value), 42);
}
'''


def main():
    with tempfile.TemporaryDirectory(prefix="bridgerton-consumer-") as directory:
        directory = Path(directory)
        (directory / "src").mkdir()
        (directory / "Cargo.toml").write_text(
            '[package]\nname = "bridge-api-check"\nversion = "0.0.0"\nedition = "2024"\n'
            '[workspace]\n[features]\nextra = []\n[dependencies]\nbridgerton = { path = '
            + json.dumps(str(ROOT)) + ' }\n'
        )
        source = directory / "src/main.rs"
        env = dict(os.environ)

        def check(text, *args, failure=None):
            source.write_text(text)
            result = subprocess.run(["cargo", "run", "--offline", "--quiet", *args],
                                    cwd=directory, env=env, capture_output=True, text=True, timeout=180)
            if failure:
                assert result.returncode != 0 and failure in result.stderr, result.stderr
            else:
                assert result.returncode == 0, result.stderr

        check(SOURCE)
        check(SOURCE, "--features", "extra")
        # A consumer declares only `bridgerton`, on both targets.
        subprocess.run(["cargo", "check", "--offline", "--quiet", "--target", "wasm32-unknown-unknown"],
                       cwd=directory, env=env, check=True, timeout=180)
        native_source = SOURCE.replace("fn main() {", '''fn main() {
    let definition = bridgerton::exports::definition().unwrap();
    let types = definition.types.swift();
    assert!(types.contains("struct `Payload`") && types.contains("enum `Choice`"));
    assert_eq!(definition.swift.matches("class `Probe`:").count(), 1);
    assert!(definition.swift.contains("class `ReturnedOnly`:"));
    assert!(definition.header.contains("bridgerton_returnedonly_free"));
    assert!(definition.header.contains("bridgerton_probe_make"));
    assert_eq!(definition.header.contains("bridgerton_probe_feature_payload"), cfg!(feature = "extra"));
    assert_eq!(definition.header.contains("bridgerton_probe_selected"), cfg!(feature = "extra"));
    assert_eq!(types.contains("struct `FeaturePayload`"), cfg!(feature = "extra"));
    assert_eq!(definition.swift.matches("public convenience init(").count(), 2); // Probe and AbortController
    assert!(!definition.header.contains("missing"));
    assert!(!definition.header.contains("identity"));
''')
        output = directory / "generated"
        native_source = native_source.replace("fn main() {", 'fn main() { bridgerton::native::generate(std::path::Path::new(' + json.dumps(str(output)) + ')).unwrap();')
        for features in ((), ("--features", "extra")):
            check(native_source, *features)
            subprocess.run(["swiftc", "-swift-version", "6", "-strict-concurrency=complete",
                            "-warnings-as-errors", "-typecheck", "-I", str(output), str(output / "Bridge.swift")],
                           cwd=directory, env=env, check=True, timeout=60)
        check(native_source + '#[bridge(transparent)] struct Task { value: u32 } #[bridge] impl Probe { pub fn reserved(&self) -> Task { Task { value: 1 } } }', failure='reserved Swift type name')
        check(native_source + 'mod duplicate { use bridgerton::bridge; #[bridge(opaque)] struct Probe; }', failure='duplicate Swift object name')
        check(native_source + 'mod duplicate { use bridgerton::bridge; use super::Probe; #[bridge(transparent)] pub struct Payload { value: u32 } #[bridge] impl Probe { pub fn collision(&self) -> Payload { Payload { value: 1 } } } }', failure='Swift type collision')
        check(SOURCE + 'type TooMany = bridgerton::Callback<(u32,u32,u32,u32)>; #[bridge] impl Probe { pub fn too_many(&self, callback: TooMany) { let _ = callback; } }', failure='NativeArgument')
        check(SOURCE + '#[bridge] impl Probe { pub fn borrowed_value(&self, value: &Payload) { let _ = value; } }', failure='borrowed native arguments must be bridged objects')
        check(SOURCE + '#[bridge(serde)] impl Probe {}', failure="unknown bridge option")
        check(SOURCE + '#[bridge] impl crate::Probe {}', failure="single identifier")
        check(SOURCE + '#[bridge(part = "loading")] impl Probe {}', failure="unknown bridge option")
        for mode in ("record", "native_record", "native_enum", "native_object"):
            check(SOURCE + f'#[bridge({mode})] struct Obsolete;', failure="unknown bridge option")
        check(SOURCE + '#[bridge] struct Ambiguous;', failure="choose bridge(transparent) or bridge(opaque)")
        check(SOURCE + '#[bridge(transparent)] struct Unserialized { value: u32 } fn assert_serializable() { fn requires<T: bridgerton::serde::Serialize>() {} requires::<Unserialized>(); }', failure="Serialize")
        print("PASS: ordinary Rust, conditional methods/constructors and type discovery with features on/off, Swift typechecking, and rejected obsolete modes")


if __name__ == "__main__":
    main()
