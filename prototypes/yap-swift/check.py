#!/usr/bin/env python3
"""Build the real Yap SwiftUI prototype and run its offline integration test (macOS, Swift 6.2+)."""
import argparse
import json
import os
from pathlib import Path
import platform
import plistlib
import subprocess
import tempfile

from pack_server import serve_packs

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
GENERATED = HERE / "generated"


def run(*args, env=None):
    print("+ " + " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=ROOT, env=env, check=True, timeout=600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packs", type=Path, default=ROOT / "out", help="directory containing fra_for_eng split language-pack files")
    parser.add_argument("--open", action="store_true", help="launch the built SwiftUI application after testing")
    args = parser.parse_args()
    packs = args.packs.resolve()
    for part in ("core", "sentences"):
        if not (packs / "fra_for_eng" / f"language_data_{part}.rkyv").is_file():
            parser.error(f"missing French {part} language pack under {packs}/fra_for_eng")
    run("cargo", "test", "-p", "yap-frontend-rs", "language_pack::native_tests", "--lib", "--locked")
    run("cargo", "test", "-p", "yap-swift-prototype", "--lib", "--locked")
    run("cargo", "bridgerton", "swift", "--package", "yap-swift-prototype", "--out-dir", GENERATED, "--locked")
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    target = Path(metadata["target_directory"]) / "debug"
    # Match the local macOS version: locally built C dependencies can target the
    # host SDK. A distributable build needs an explicit shared deployment target.
    triple = f"{platform.machine()}-apple-macosx{platform.mac_ver()[0]}"
    swift = ("swiftc", "-target", triple, "-swift-version", "6", "-strict-concurrency=complete", "-warnings-as-errors", "-parse-as-library", "-I", GENERATED)
    link = (target / "libyap_swift_prototype.a", "-framework", "Security", "-framework", "SystemConfiguration")
    run(*swift, GENERATED / "Bridge.swift", HERE / "swift/Smoke.swift", *link, "-o", GENERATED / "smoke")
    with tempfile.TemporaryDirectory(prefix="yap-swift-smoke-") as data, serve_packs(packs) as server:
        run(GENERATED / "smoke", HERE / "smoke", env={**os.environ, "YAP_DATA_DIR": data, "YAP_AI_BACKEND_URL": server.url})
        assert {part for part, _ in server.downloads} == {"core", "sentences"}
        assert len(server.downloads) == len(set(server.downloads)), "a cached chunk was downloaded again"
        assert server.offline and server.offline_downloads == 0, "reopening attempted an HTTP download"
        print(f"PASS: {len(server.downloads)} HTTP chunks; cache reopen needed no downloads")
    app = GENERATED / "Yap Prototype.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True, exist_ok=True)
    run(*swift, GENERATED / "Bridge.swift", HERE / "swift/App.swift", *link, "-o", contents / "MacOS/YapPrototype")
    (contents / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": "YapPrototype", "CFBundleIdentifier": "town.yap.swift-prototype",
        "CFBundleName": "Yap Prototype", "CFBundlePackageType": "APPL", "CFBundleVersion": "1",
        "NSHighResolutionCapable": True,
    }))
    run("codesign", "--force", "--sign", "-", app)
    print(f"PASS: real Yap integration and SwiftUI build. App: {app}")
    if args.open:
        with serve_packs(packs) as server:
            print(f"Serving fixture packs at {server.url} until the app quits", flush=True)
            subprocess.run([str(contents / "MacOS/YapPrototype")], cwd=ROOT,
                           env={**os.environ, "YAP_AI_BACKEND_URL": server.url}, check=True)


if __name__ == "__main__":
    main()
