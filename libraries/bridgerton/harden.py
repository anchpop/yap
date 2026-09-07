#!/usr/bin/env python3
"""Run Miri codec checks, ASan lifecycle stress, and bounded libFuzzer coverage."""
import argparse
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent


def run(*command, env=None):
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fuzz-seconds", type=int, default=60)
    args = parser.parse_args()
    if args.fuzz_seconds <= 0:
        parser.error("--fuzz-seconds must be positive")
    run("cargo", "+nightly", "miri", "test", "-p", "bridgerton", "--lib", "value::tests")
    # Xcode 26's new linker rejects ASan-instrumented inventory initializers.
    # Both the native test and codec fuzzer now link the runtime's inventory.
    # Use the classic linker for both instrumented executables.
    asan = {**os.environ, "RUSTFLAGS": "-Zsanitizer=address -C link-arg=-Wl,-ld_classic"}
    run("cargo", "+nightly", "run", "--target", "aarch64-apple-darwin", "-p", "bridge-fixture", "--bin", "native-ownership", env=asan)
    fuzz = {**os.environ, "RUSTFLAGS": "-C link-arg=-Wl,-ld_classic"}
    run("cargo", "+nightly", "fuzz", "run", "values", "--", f"-max_total_time={args.fuzz_seconds}", "-max_len=4096", env=fuzz)
    print("PASS: Miri, ASan lifetime stress, and codec fuzzing")


if __name__ == "__main__":
    main()
