#!/usr/bin/env python3
"""Transcribe films until the ElevenLabs credit balance runs low, then stop.

The plan's credits reset monthly and do not carry over, so anything unused is
lost — but anything spent past the allowance bills the card. This walks a list
of films, refuses to start one it cannot afford, and leaves a floor untouched.

Balance comes from the API rather than local arithmetic, which drifts; the
number that matters is the one ElevenLabs will bill on.

Usage: spend_credits.py --films f.json [--floor 15000] [--max-credits N] [--dry-run]
"""

import argparse
import json
import os
import pathlib
import subprocess
import time
import urllib.error
import urllib.request

CORPUS = pathlib.Path("/data/andrep/subtitle-corpus")
REPO = "/data/coding/yap"
# Captured once with `nix print-dev-env`. Emphatically not `nix develop`: on a
# dirty tree that copies the whole repo (~7.8GB) into the nix store *per
# invocation*, which filled the root disk mid-run and made every later film
# fail before it started — silently, because the error goes to stderr.
DEVENV = "/data/coding/yap-tmp/devenv.sh"
# Measured twice on this account, over 27.7h and 21.1h of audio: 1,579/hour.
CREDITS_PER_HOUR = 1580


def load_env(path):
    if not os.path.exists(path):
        return
    for line in open(path):
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            k, v = line.split("=", 1)
            os.environ.setdefault(k.strip(), v.strip().strip('"').strip("'"))


def balance(retries=6):
    """Credits left in this billing period, straight from the account.

    Backs off on 429: the subscription endpoint rate-limits well below one call
    per film, and a guard that crashes is worse than no guard.
    """
    for attempt in range(retries):
        try:
            req = urllib.request.Request(
                "https://api.elevenlabs.io/v1/user/subscription",
                headers={"xi-api-key": os.environ["ELEVENLABS_API_KEY"]})
            d = json.load(urllib.request.urlopen(req, timeout=60))
            return d["character_limit"] - d["character_count"]
        except urllib.error.HTTPError as e:
            if e.code != 429 or attempt == retries - 1:
                raise
            time.sleep(2 ** attempt * 5)
    raise RuntimeError("unreachable")


class Ledger:
    """Live balance, reconciled with the API only as often as it allows.

    Between reconciliations the estimate moves by the same per-hour figure used
    to decide affordability, and it is deliberately pessimistic: an over-estimate
    stops early, an under-estimate spends your money.
    """

    def __init__(self, every=5):
        self.actual = balance()
        self.estimate = self.actual
        self.every = every
        self.since = 0

    def spend(self, credits):
        self.estimate -= credits
        self.since += 1

    def left(self):
        if self.since >= self.every:
            try:
                self.actual = balance()
                self.estimate = self.actual
                self.since = 0
            except Exception as e:
                print(f"          (balance check failed, using estimate: {e})", flush=True)
        return self.estimate


def hours(imdb):
    return json.load(open(CORPUS / imdb / "audio.json"))["duration_ms"] / 3600000


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--films", required=True, help="JSON list of imdb ids")
    ap.add_argument("--floor", type=int, default=15000,
                    help="never spend below this many credits")
    ap.add_argument("--max-credits", type=int, default=130000,
                    help="hard ceiling on this run's own spend, independent of "
                         "the balance API. The floor trusts a number fetched "
                         "over the network and a per-hour estimate; this trusts "
                         "neither, so a wrong estimate or a stale balance "
                         "cannot run away.")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()

    load_env(f"{REPO}/.env")
    films = json.loads(pathlib.Path(a.films).read_text())

    ledger = Ledger()
    start = ledger.actual
    print(f"balance {start:,} credits, floor {a.floor:,}, budget {start - a.floor:,} "
          f"(~{(start - a.floor) / CREDITS_PER_HOUR:.0f}h)\n", flush=True)

    done = failed = skipped = 0
    spent_here = 0
    for n, imdb in enumerate(films, 1):
        if (CORPUS / imdb / "transcript.jsonl").exists():
            print(f"[{n}/{len(films)}] {imdb} already transcribed", flush=True)
            continue
        try:
            h = hours(imdb)
        except Exception as e:
            print(f"[{n}/{len(films)}] {imdb} no audio.json ({e})", flush=True)
            continue
        need = int(h * CREDITS_PER_HOUR)
        if spent_here + need > a.max_credits:
            print(f"\nSTOPPING: run ceiling reached — {spent_here:,} spent, {imdb} "
                  f"needs ~{need:,}, ceiling {a.max_credits:,}. "
                  f"{len(films) - n + 1} films left unrun.", flush=True)
            skipped = len(films) - n + 1
            break
        left = ledger.left()
        if left - need < a.floor:
            print(f"\nSTOPPING: {imdb} needs ~{need:,}, balance {left:,}, floor "
                  f"{a.floor:,}. {len(films) - n + 1} films left unrun.", flush=True)
            skipped = len(films) - n + 1
            break
        print(f"[{n}/{len(films)}] {imdb} {h:.2f}h ~{need:,} credits "
              f"(balance {left:,})", flush=True)
        if a.dry_run:
            ledger.spend(need)
            spent_here += need
            done += 1
            continue
        r = subprocess.run(
            ["sg", "media", "-c",
             f"bash -c 'source {DEVENV} >/dev/null 2>&1; "
             f"./target/release/subtitle-corpus transcribe --imdb {imdb}'"],
            cwd=REPO, capture_output=True, text=True)
        tail = [x for x in r.stdout.splitlines() if x.strip()][-1:] or ["(no output)"]
        print(f"          {tail[0]}", flush=True)

        # The artifact, not the exit status, is the evidence. A run that dies
        # before the binary starts writes nothing to stdout, and a blank line
        # reads exactly like success — 25 films were lost that way once.
        if not (CORPUS / imdb / "transcript.jsonl").exists():
            failed += 1
            print(f"          ! no transcript written (exit {r.returncode})", flush=True)
            for line in [x for x in r.stderr.splitlines() if x.strip()][-3:]:
                print(f"            {line}", flush=True)
            continue
        ledger.spend(need)
        spent_here += need
        done += 1

    end = balance()
    print(f"\ntranscribed {done}, failed {failed}, skipped {skipped}")
    print(f"credits: {start:,} -> {end:,}  (spent {start - end:,}, "
          f"~{(start - end) / CREDITS_PER_HOUR:.1f}h)")


if __name__ == "__main__":
    main()
