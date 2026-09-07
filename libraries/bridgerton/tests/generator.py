#!/usr/bin/env python3
"""Exercise the generator ABI, optimized registration retention, and failure reporting."""
import os
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BRIDGERTON = ("cargo", "bridgerton")


def main():
    env = {**os.environ, "CARGO_PROFILE_RELEASE_LTO": "true"}
    with tempfile.TemporaryDirectory(prefix="bridgerton-語-") as directory:
        output = Path(directory)
        command = [*BRIDGERTON, "swift", "--package", "bridge-fixture", "--release", "--out-dir"]
        subprocess.run([*command, str(output)], cwd=ROOT, env=env, check=True, timeout=300)
        for name in ("Bridge.swift", "BridgeFFI.h", "module.modulemap"):
            assert (output / name).read_bytes() == (ROOT / "generated" / name).read_bytes(), name
        feature_output = output.parent / (output.name + "-features")
        try:
            subprocess.run([*command, str(feature_output), "--features", "extra", "--no-default-features"], cwd=ROOT, env=env, check=True, timeout=300)
            assert "public var `extra`: Bool" in (feature_output / "Bridge.swift").read_text()
            assert "public var `extra`: Bool" not in (output / "Bridge.swift").read_text()
            build = json.loads((feature_output / "build.json").read_text())
            assert build["features"] == ["extra"] and not build["default_features"]
        finally:
            import shutil
            shutil.rmtree(feature_output, ignore_errors=True)
        # Rust's I/O error must cross the generator ABI; no false success message.
        result = subprocess.run([*command, str(output / "Bridge.swift")],
                                cwd=ROOT, env=env, capture_output=True, text=True, timeout=300)
        assert result.returncode != 0 and "error:" in result.stderr, result
        assert "Generated Swift bindings" not in result.stdout, result.stdout
    print("PASS: LTO retains split-impl metadata; generation is deterministic, accepts Unicode paths, and reports I/O errors")


if __name__ == "__main__":
    main()
