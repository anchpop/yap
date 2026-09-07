#!/usr/bin/env python3
"""Check optimized target metadata, simulator execution, and two-library SwiftPM composition."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
BRIDGERTON = ("cargo", "bridgerton")
ENV = {**os.environ, "CARGO_PROFILE_RELEASE_LTO": "true"}


def run(*command, cwd=ROOT):
    print("+ " + " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=cwd, env=ENV, check=True, timeout=300)


def native_test(bindings, work, simulator=None):
    source = (ROOT / "tests/Native.swift").read_text()
    header = (bindings / "BridgeFFI.h").read_text()
    for name in set(re.findall(r"\bbridgerton_counter_\w+", source)):
        matches = re.findall(r"\b(bridgerton_[0-9a-f]+_" + name + r")\(", header)
        assert len(matches) == 1, (name, matches)
        source = re.sub(r"\b" + name + r"\b", matches[0], source)
    test = work / "Native.swift"
    test.write_text(source)
    info = json.loads((bindings / "build.json").read_text())
    extra = []
    if simulator:
        sdk = subprocess.check_output(["xcrun", "--sdk", "iphonesimulator", "--show-sdk-path"], text=True).strip()
        extra = ["-target", "arm64-apple-ios18.0-simulator", "-sdk", sdk]
    executable = work / "native-tests"
    run("swiftc", "-O", "-swift-version", "6", "-strict-concurrency=complete", "-warnings-as-errors", "-parse-as-library", *extra, "-I", bindings, bindings / "Bridge.swift", test, ROOT / "tests/Values.swift", info["archives"][0], "-o", executable)
    if simulator:
        run("xcrun", "simctl", "spawn", simulator, executable)
    else:
        run(executable)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--simulator", help="booted iOS simulator UUID")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="bridgerton-release-") as temporary:
        work = Path(temporary)
        host = work / "host"
        generate = [*BRIDGERTON, "swift", "--package", "bridge-fixture", "--release", "--out-dir"]
        run(*generate, host)
        native_test(host, work)
        bindings = ["--bindings", host]
        if args.simulator:
            ios = work / "ios"
            run(*generate, ios, "--target", "aarch64-apple-ios-sim", "--simulator", args.simulator)
            host_source = (host / "Bridge.swift").read_text()
            ios_source = (ios / "Bridge.swift").read_text()
            assert "public var `ios`: Bool" not in host_source
            assert "public var `ios`: Bool" in ios_source
            assert "class `Counter`" in ios_source, "target runner discarded application inventory"
            native_test(ios, work, args.simulator)
            bindings += ["--bindings", ios]
        first = work / "FirstBridge"
        second = work / "SecondBridge"
        run(*BRIDGERTON, "package", *bindings, "--module", "FirstBridge", "--out-dir", first)
        run(*BRIDGERTON, "package", "--bindings", host, "--module", "SecondBridge", "--out-dir", second)
        app = work / "Composition"
        source = app / "Sources" / "Check"
        source.mkdir(parents=True)
        (app / "Package.swift").write_text('''// swift-tools-version: 6.2
import PackageDescription
let package = Package(name: "Composition", platforms: [.macOS(.v15)], dependencies: [
    .package(path: "../FirstBridge"), .package(path: "../SecondBridge")
], targets: [.executableTarget(name: "Check", dependencies: [
    .product(name: "FirstBridge", package: "FirstBridge"),
    .product(name: "SecondBridge", package: "SecondBridge")
])])
''')
        (source / "main.swift").write_text('''import FirstBridge
import SecondBridge
@main struct Check {
    @MainActor static func main() async throws {
        let a = FirstBridge.Counter()
        let b = SecondBridge.Counter()
        precondition(a.live_counters() == 1 && b.live_counters() == 1)
        _ = try a.add(amount: 10)
        precondition(a.value() == 10 && b.value() == 0)
        precondition(a.snapshot().value() == 10)
        let x = try await FirstBridge.Counter.create(initial: 42)
        let y = try await SecondBridge.Counter.create(initial: 17)
        precondition(x.value() == 42 && y.value() == 17)
        print("PASS: independent library state, object returns, and async calls")
    }
}
''')
        run("swift", "run", "--package-path", app, "-c", "release", "Check")
        run("cargo", "build", "-p", "bridge-fixture", "--lib", "--release", "--target", "aarch64-apple-ios", "--locked")
        # A device binary build is separate from execution; never label it a device test.
        report = {"optimized_native": "passed", "two_library_swiftpm": "passed", "ios_simulator": "passed" if args.simulator else "not run", "ios_device_build": "passed", "ios_device_execution": "not run"}
        output = ROOT / "generated-release"
        output.mkdir(exist_ok=True)
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        for package in (first, second):
            destination = output / package.name
            if destination.exists():
                shutil.rmtree(destination)
            shutil.copytree(package, destination)
    print("PASS: requested release checks; physical-device execution remains separate")


if __name__ == "__main__":
    main()
