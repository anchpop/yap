#!/usr/bin/env python3
"""Measure optimized Swift crossings and Rust codec allocation costs."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
BRIDGERTON = ("cargo", "bridgerton")
OUT = ROOT / "generated-benchmark"
ENV = {**os.environ, "CARGO_PROFILE_RELEASE_LTO": "true"}


def main():
    subprocess.run([*BRIDGERTON, "swift", "--package", "bridge-fixture", "--release", "--out-dir", str(OUT)], cwd=ROOT, env=ENV, check=True)
    info = json.loads((OUT / "build.json").read_text())
    subprocess.run(["swiftc", "-O", "-swift-version", "6", "-strict-concurrency=complete", "-warnings-as-errors", "-parse-as-library", "-I", str(OUT), str(OUT / "Bridge.swift"), "tests/Benchmark.swift", info["archives"][0], "-o", str(OUT / "benchmark")], cwd=ROOT, env=ENV, check=True)
    swift = json.loads(subprocess.check_output([str(OUT / "benchmark")], env=ENV, text=True))
    rust = json.loads(subprocess.check_output(["cargo", "run", "--release", "-p", "bridge-fixture", "--bin", "codec-benchmark"], cwd=ROOT, env=ENV, text=True))
    result = {"rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(), "swift": subprocess.check_output(["swiftc", "--version"], text=True).strip(), "swift_microseconds": swift, "rust_codec": rust}
    (OUT / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
