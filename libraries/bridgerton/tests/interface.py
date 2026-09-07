#!/usr/bin/env python3
"""Compile old Swift against a changed Rust record layout with unchanged C symbols."""
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BRIDGERTON = ("cargo", "bridgerton")
ENV = dict(os.environ)
SOURCE = '''use bridgerton::bridge;
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[bridge(transparent)]
pub struct Payload { pub first: u32, pub second: u32 }
#[bridge(opaque)]
pub struct Probe;
#[bridge]
impl Probe {
    #[bridge(constructor)]
    pub fn new() -> Self { Self }
    pub fn inspect(&self, value: Payload) -> u32 { value.first }
}
'''


def run(*args):
    subprocess.run(list(map(str, args)), cwd=ROOT, env=ENV, check=True, timeout=180)


def main():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    with tempfile.TemporaryDirectory(prefix="bridgerton-interface-") as temporary:
        directory = Path(temporary)
        manifest = directory / "Cargo.toml"
        manifest.write_text('[package]\nname = "interface-fixture"\nversion = "0.0.0"\nedition = "2024"\n'
            '[workspace]\n[lib]\ncrate-type = ["cdylib", "staticlib"]\n[dependencies]\nbridgerton = {path = '
            + json.dumps(str(ROOT)) + '}\n')
        (directory / "src").mkdir()
        source = directory / "src/lib.rs"
        source.write_text(SOURCE)
        output = directory / "generated"
        run("cargo", "generate-lockfile", "--offline", "--manifest-path", manifest)
        run(*BRIDGERTON, "swift", "--manifest-path", manifest, "--package", "interface-fixture", "--out-dir", output)
        archive = json.loads((output / "build.json").read_text())["archives"][0]
        swift = directory / "Check.swift"
        swift.write_text('@main struct Check { @MainActor static func main() { '
            'precondition(Probe().inspect(value: Payload(first: 11, second: 22)) == 11) } }')
        executable = directory / "check"
        command = ["swiftc", "-swift-version", "6", "-strict-concurrency=complete", "-warnings-as-errors", "-parse-as-library",
                   "-I", output, output / "Bridge.swift", swift, archive, "-o", executable]
        run(*command)
        run(executable)
        # Field names/types and all C symbols remain available, but the byte layout changes.
        source.write_text(SOURCE.replace('pub first: u32, pub second: u32', 'pub second: u32, pub first: u32')
            .replace('pub fn new() -> Self { Self }', 'pub fn new() -> Self { panic!("application entry reached") }'))
        run("cargo", "build", "--offline", "--locked", "--manifest-path", manifest, "--lib")
        run(*command)
        result = subprocess.run([str(executable)], capture_output=True, text=True, env=ENV, timeout=30)
        assert result.returncode < 0 and 'Swift bindings do not match' in result.stderr, result
        assert 'application entry reached' not in result.stderr, result.stderr
    print("PASS: matching interfaces work; stale bindings reject reordered fields before entering Rust application code")


if __name__ == "__main__":
    main()
