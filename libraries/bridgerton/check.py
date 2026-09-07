#!/usr/bin/env python3
"""Build and test both bindings. Run from any directory; requires macOS + Swift 6.2+."""
import argparse
import os
from pathlib import Path
import resource
import re
import subprocess

ROOT = Path(__file__).resolve().parent
BRIDGERTON = ("cargo", "bridgerton")
ENV = dict(os.environ)
TARGET = Path(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, text=True).split('"target_directory":"')[1].split('"')[0])


def run(*args, expect_failure=None, expect_crash=False):
    print("+ " + " ".join(args), flush=True)
    if expect_failure is None:
        subprocess.run(args, cwd=ROOT, env=ENV, check=True, timeout=300)
    else:
        result = subprocess.run(args, cwd=ROOT, env=ENV, capture_output=True, text=True, timeout=30)
        if result.returncode == 0 or expect_failure not in result.stderr or (expect_crash and result.returncode >= 0):
            raise RuntimeError(f"expected failure containing {expect_failure!r}: {result}")
        print(f"PASS: expected rejection ({expect_failure})", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-browser", action="store_true", help="omit Chromium (requires yap-frontend Playwright)")
    args = parser.parse_args()
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    run("cargo", "fmt", "--all", "--", "--check")
    run("python3", "tests/api.py")
    packages = ("-p", "bridgerton", "-p", "bridgerton-macros", "-p", "bridge-fixture", "-p", "cargo-bridgerton")
    run("cargo", "test", *packages, "--locked")
    run("cargo", "clippy", *packages, "--all-targets", "--locked", "--", "-D", "warnings")
    run(*BRIDGERTON, "swift", "--package", "bridge-fixture", "--out-dir", "generated", "--locked")
    run("python3", "tests/generator.py")
    run("python3", "tests/interface.py")
    run("cargo", "build", "-p", "bridge-fixture", "--lib", "--locked")
    swift = ("swiftc", "-swift-version", "6", "-strict-concurrency=complete", "-warnings-as-errors", "-parse-as-library", "-I", "generated")
    source = (ROOT / "tests/Native.swift").read_text()
    header = (ROOT / "generated/BridgeFFI.h").read_text()
    for name in set(re.findall(r"\bbridgerton_counter_\w+", source)):
        matches = re.findall(r"\b(bridgerton_[0-9a-f]+_" + name + r")\(", header)
        if len(matches) != 1:
            raise RuntimeError(f"expected one qualified fixture symbol for {name}: {matches}")
        source = re.sub(r"\b" + name + r"\b", matches[0], source)
    (ROOT / "generated/Native.swift").write_text(source)
    run(*swift, "generated/Bridge.swift", "generated/Native.swift", "tests/Values.swift", str(TARGET / "debug/libbridge_fixture.a"), "-o", "generated/native-tests")
    run("generated/native-tests")
    for mode in ("--panic", "--panic-result", "--panic-async"):
        run("generated/native-tests", mode, expect_failure="intentional Rust panic", expect_crash=True)
    run("generated/native-tests", "--invalid-infallible", expect_failure="invalid value length", expect_crash=True)
    run("generated/native-tests", "--consumed-object", expect_failure="object was consumed", expect_crash=True)
    run("generated/native-tests", "--wrong-thread", expect_failure="outside the main thread")
    run(*swift, "-typecheck", "generated/Bridge.swift", "tests/IsolationFailure.swift", expect_failure="main actor-isolated")
    run("cargo", "run", "-p", "bridge-fixture", "--bin", "native-ownership", "--locked")
    run(*BRIDGERTON, "web", "--package", "bridge-fixture", "--target", "nodejs", "--out-dir", "../generated/node", "--locked")
    run("cargo", "clippy", "-p", "bridge-fixture", "--lib", "--target", "wasm32-unknown-unknown", "--locked", "--", "-D", "warnings")
    run("node", "tests/node.cjs")
    run("node", "../../yap-frontend/node_modules/typescript/bin/tsc", "--noEmit", "--strict", "--target", "es2022", "--module", "commonjs", "--moduleResolution", "node", "--lib", "es2022,dom,esnext.disposable", "tests/types.ts")
    # Descriptor interpretation must also survive optimizer/LTO transformations.
    ENV["CARGO_PROFILE_RELEASE_LTO"] = "true"
    run(*BRIDGERTON, "web", "--package", "bridge-fixture", "--target", "nodejs", "--out-dir", "../generated/node", "--release", "--locked")
    run("node", "tests/node.cjs")
    if not args.skip_browser:
        run(*BRIDGERTON, "web", "--package", "bridge-fixture", "--target", "web", "--out-dir", "../generated/web", "--release", "--locked")
        run("node", "tests/browser.cjs")
    print("PASS: all requested prototype checks", flush=True)


if __name__ == "__main__":
    main()
