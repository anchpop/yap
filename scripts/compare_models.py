"""End-to-end model A vs model B comparison at threshold=0.

For each model:
  1. Patch modal-envs/wav2vec2_phoneme.py with the MODEL_ID + MODEL_REVISION
     (parse `model@<sha>` syntax). Bump the image cache-buster echo so
     Modal rebuilds and pulls fresh weights from HuggingFace.
  2. Deploy to Modal (~70s rebuild).
  3. Run the compare-audio-models binary with
     WAV2VEC2_CACHE_VERSION_OVERRIDE so the cache is partitioned per
     (model, revision) — old caches stay intact.
  4. Dump per-clip results to /tmp/per_clip_<safe_name>.jsonl.

Then diff the two per-clip files, categorize each failure into buckets
(nasal-vowel / schwa→ø / extras / ʁ-variants / missing / etc.), and
print a markdown report you can paste straight into a conversation.

Subcommands:
  compare       (default)   Compare two models end-to-end.
  save-baseline             Tag a per-clip dump as a named baseline.
  diff-baseline             Diff current cached output vs a saved baseline.

Usage examples:

    # Compare two HF model IDs at HEAD (`main`)
    python3 scripts/compare_models.py \\
        anchpop/lexide-pronunciation-unified \\
        anchpop/lexide-pronunciation-unified-vad

    # Pin specific HF commits for reproducibility
    python3 scripts/compare_models.py \\
        anchpop/lexide-pronunciation-unified@abc123 \\
        anchpop/lexide-pronunciation-unified-vad@def456

    # Skip deploy/compare, just re-diff existing caches (e.g. tweaking
    # categorization or filtering by actor)
    python3 scripts/compare_models.py A B --skip-deploy --actor Jean-Cavard

    # Save current VAD run as a named baseline
    python3 scripts/compare_models.py save-baseline vad-2026-05-25 \\
        anchpop/lexide-pronunciation-unified-vad

    # Compare current VAD against that baseline (re-runs the new side)
    python3 scripts/compare_models.py diff-baseline vad-2026-05-25 \\
        anchpop/lexide-pronunciation-unified-vad
"""

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import unicodedata
from collections import defaultdict

ROOT = pathlib.Path(__file__).resolve().parent.parent
MODAL_PY = ROOT / "modal-envs" / "wav2vec2_phoneme.py"
# Honor CARGO_TARGET_DIR so the build can land off the (often full) main disk.
_CARGO_TARGET = pathlib.Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
BIN = _CARGO_TARGET / "release" / "compare-audio-models"
BASELINES = ROOT / ".audio-eval-baselines"


# ============================================================
# Model ID + HF revision parsing
# ============================================================

def parse_model(spec: str):
    """Return (model_id, revision). Revision defaults to "main"."""
    if "@" in spec:
        model_id, revision = spec.split("@", 1)
        return model_id, revision
    return spec, "main"


_NAME_SANITIZE = re.compile(r"[^A-Za-z0-9_]")


def safe_name(model_id: str, revision: str) -> str:
    """Filename-safe slug.

    Preserves the historical mapping `/` → `__` for the org/name separator
    in HF model ids (so existing cache dirs and per-clip dumps stay
    addressable). Everything else outside `[A-Za-z0-9_]` collapses to `_`,
    which keeps HF revisions like `refs/pr/123` or `feature/foo` safe.
    """
    base = _NAME_SANITIZE.sub("_", model_id.replace("/", "__"))
    if revision != "main":
        base = f"{base}__{_NAME_SANITIZE.sub('_', revision[:12])}"
    return base


# ============================================================
# Modal deploy patching
# ============================================================

def resolve_revision(model_id: str, revision: str) -> str:
    """Resolve a possibly-mutable HF ref (branch/tag, e.g. 'main') to an exact
    commit SHA, so the stable prediction cache key pins to specific weights — a
    later force-push to the same ref can't silently reuse stale cached
    predictions under an unchanged key."""
    from huggingface_hub import HfApi
    return HfApi().model_info(model_id, revision=revision).sha


def resolve_model(spec: str):
    """Parse a `model[@ref]` spec and resolve its ref to an exact commit SHA.
    Centralized so cache keys, dump paths, the deploy, labels, and saved-baseline
    metadata all pin to the same concrete weights and never drift out of sync."""
    model_id, revision = parse_model(spec)
    return model_id, resolve_revision(model_id, revision)


def patch_modal(model_id: str, revision: str, deploy_marker: str):
    # `deploy_marker` is unique per (re)deploy: it feeds MODEL_WEIGHTS_VERSION
    # (the image cache-buster, so Modal builds a fresh image + routes to a new
    # container) AND DEPLOY_MARKER (the freshness/contamination check). It is
    # deliberately NOT the prediction cache key — that stays stable per model.
    src = MODAL_PY.read_text()
    src = re.sub(r'MODEL_ID = "[^"]*"', f'MODEL_ID = "{model_id}"', src)
    src = re.sub(r'MODEL_REVISION = "[^"]*"', f'MODEL_REVISION = "{revision}"', src)
    src = re.sub(
        r"echo 'MODEL_WEIGHTS_VERSION=[^']*'",
        f"echo 'MODEL_WEIGHTS_VERSION={deploy_marker}'",
        src,
    )
    # DEPLOY_MARKER is echoed back by the /predict marker_only path so the
    # freshness check can assert it's hitting the just-deployed container.
    src = re.sub(r'DEPLOY_MARKER = "[^"]*"', f'DEPLOY_MARKER = "{deploy_marker}"', src)
    MODAL_PY.write_text(src)


# Evals deploy to a DEDICATED app, never the production "wav2vec2-phoneme"
# that generate-data / yap-ai-backend hit. The Modal file reads WAV2VEC2_APP_NAME
# (default "wav2vec2-phoneme"), so passing it here on deploy isolates eval runs.
MODAL_APP_NAME = "wav2vec2-phoneme-eval"
# Structural guard: the harness must NEVER target production, even if this
# constant is edited. "Eval never touches prod" is the whole point.
assert MODAL_APP_NAME.endswith("-eval"), (
    f"eval harness must target an -eval app, not {MODAL_APP_NAME!r}"
)


def stop_app():
    """Force any warm container to shut down before the next deploy.

    Without this, `modal deploy` updates the image registration but leaves
    the previously-warm container serving requests — so a subsequent bulk
    inference run can hit the OLD model. Killing the app guarantees the
    next request lands on a fresh container with the new weights.
    """
    print(f"  → modal app stop {MODAL_APP_NAME} ...", flush=True)
    subprocess.run(["modal", "app", "stop", MODAL_APP_NAME],
                   capture_output=True, text=True)


def deploy():
    print("  → modal deploy ...", flush=True)
    r = subprocess.run(
        ["modal", "deploy", str(MODAL_PY)],
        capture_output=True, text=True,
        env={**os.environ, "WAV2VEC2_APP_NAME": MODAL_APP_NAME},
    )
    if r.returncode != 0:
        print(r.stdout); print(r.stderr, file=sys.stderr)
        sys.exit(f"modal deploy failed (exit {r.returncode})")
    last = [ln for ln in r.stdout.splitlines() if "App deployed" in ln]
    print(f"  → {last[0].strip() if last else 'deploy ok'}", flush=True)


# ── Deploy-marker freshness guard ────────────────────────────────────────────
# Modal's `app stop` returns before warm containers fully drain, so a fresh
# deploy can be silently served by the PREVIOUS model's still-warm container —
# we caught this returning byte-identical predictions for two genuinely
# different checkpoints. The guard below verifies freshness by EXACT MARKER
# MATCH: every deploy stamps a unique DEPLOY_MARKER into the Modal app, the
# /predict marker_only path echoes it back, and we refuse to run the bulk
# comparison until the live container reports the marker we just deployed.
# This is correct (not a heuristic): it covers the first model too, and works
# even when two checkpoints would produce identical predictions.

# The eval app's endpoint, derived from MODAL_APP_NAME — deliberately NOT
# honoring an ambient WAV2VEC2_ENDPOINT_URL, which might point at production
# (e.g. exported for generate-data) and silently break eval isolation. Both
# the freshness guard and the Rust verifier (via run_compare's env) hit exactly
# this container.
ENDPOINT_URL = f"https://anchpop--{MODAL_APP_NAME}-wav2vec2phoneme-predict.modal.run"


def probe_endpoint() -> dict | None:
    """Return the live container's marker_only JSON — `{deploy_marker, ...}` on
    success or `{load_error, ...}` if the model failed to load — or None on a
    transport error (container still cold/starting)."""
    import json as _json
    import urllib.request
    try:
        req = urllib.request.Request(
            ENDPOINT_URL, data=_json.dumps({"marker_only": True}).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=300) as resp:
            return _json.loads(resp.read())
    except Exception:
        return None


def wait_for_marker(expected: str, redeploy=None, max_attempts: int = 6) -> None:
    """Block until the live container reports `expected` as its DEPLOY_MARKER.

    If the marker is wrong (a stale warm container is still serving), force a
    genuinely fresh deploy via `redeploy` (bumped image version + drain wait)
    and retry. A plain re-deploy of an unchanged image is a no-op Modal serves
    from the warm container, so the image must actually change — `redeploy`
    handles that. Aborts rather than running a contaminated comparison.
    """
    for attempt in range(1, max_attempts + 1):
        resp = probe_endpoint()
        if resp is None:
            print(f"  → marker probe {attempt}: no response yet, retrying ...", flush=True)
            time.sleep(10)
            continue
        # The container started but the model itself failed to load (e.g. a head
        # shape mismatch). Surface the real traceback immediately — don't spin
        # through redeploys mistaking it for the warm-container bug.
        if resp.get("load_error"):
            sys.exit(
                "Modal container started but the model failed to load:\n\n"
                + resp["load_error"]
            )
        marker = resp.get("deploy_marker")
        if marker == expected:
            print(f"  → deploy marker verified ✓ ({expected})", flush=True)
            return
        print(f"  → marker {attempt}/{max_attempts} MISMATCH "
              f"(live={marker}, want={expected}) — stale container, "
              f"forcing fresh redeploy ...", flush=True)
        if redeploy is not None:
            expected = redeploy()  # redeploy bumps the marker; track the new one
        else:
            stop_app(); deploy()
        time.sleep(20)
    sys.exit(
        f"Deploy marker never reached {expected!r} after {max_attempts} "
        "redeploys — likely the Modal warm-container bug. Aborting rather "
        "than reporting contaminated results."
    )


def ensure_bin():
    if BIN.exists():
        return
    print("Building compare-audio-models (one-time)...", flush=True)
    r = subprocess.run(
        ["cargo", "build", "--release", "--bin", "compare-audio-models"],
        cwd=ROOT,
    )
    if r.returncode != 0 or not BIN.exists():
        sys.exit("cargo build failed")


def run_compare(lang_dir: str, cache_subdir: str, dump_path: str, expected_marker: str):
    # Threshold=0 is hardcoded — see memory: feedback_audio_verify_threshold.md.
    env = {**os.environ,
           "AUDIO_VERIFY_THRESHOLD": "0",
           "WAV2VEC2_CACHE_VERSION_OVERRIDE": cache_subdir,
           # Point the bin at the eval app, not the production default.
           "WAV2VEC2_ENDPOINT_URL": ENDPOINT_URL,
           # Reject (don't cache) any response not served by the container we
           # just deployed — per-request contamination guard.
           "WAV2VEC2_EXPECTED_DEPLOY_MARKER": expected_marker,
           "PER_CLIP_DUMP_PATH": dump_path}
    print(f"  → compare-audio-models (cache={cache_subdir})", flush=True)
    r = subprocess.run([str(BIN), lang_dir], cwd=ROOT, env=env,
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(r.stdout); print(r.stderr, file=sys.stderr)
        sys.exit(f"compare-audio-models failed (exit {r.returncode})")


def populate(model_id: str, revision: str, lang_dir: str):
    """Patch + deploy + populate cache for one model. Returns dump_path.

    Stops the running app, deploys the requested checkpoint, then verifies
    via DEPLOY_MARKER that the live container is the one we just deployed
    (not a stale warm container from a previous model) before running the
    bulk comparison.

    The modal py file is patched in-place to deploy each model, then
    restored to its original contents in a finally block so a normal
    comparison run doesn't dirty the worktree.
    """
    # `revision` arrives already resolved to an exact commit SHA (see
    # resolve_model), so the stable cache key, dump path, and deploy all pin to
    # the same concrete weights.
    sn = safe_name(model_id, revision)
    dump_path = f"/tmp/per_clip_{sn}.jsonl"
    print(f"\n### {model_id} (revision={revision})")
    original_modal_src = MODAL_PY.read_text()

    # Two distinct identifiers, deliberately decoupled:
    #  • cache_key   — stable (model, revision, decoder). The prediction cache
    #    is partitioned by this, so re-running the same pinned model reuses
    #    predictions instead of re-inferring everything.
    #  • deploy_marker — bumped on every (re)deploy (unique timestamp) so the
    #    image content changes (forcing a fresh Modal image + container) and so
    #    the freshness/contamination guard can pin the exact container. NEVER
    #    part of the cache key.
    cache_key = f"{sn}__greedy_v1"
    state = {"deploy_marker": None}

    def do_deploy() -> str:
        deploy_marker = f"{sn}__{int(time.time())}"
        state["deploy_marker"] = deploy_marker
        stop_app()
        patch_modal(model_id, revision, deploy_marker)
        deploy()
        return deploy_marker

    try:
        marker = do_deploy()
        wait_for_marker(marker, redeploy=do_deploy)
        run_compare(lang_dir, cache_key, dump_path, state["deploy_marker"])
    finally:
        MODAL_PY.write_text(original_modal_src)
    return dump_path


# ============================================================
# Diff/categorization (mirrors Rust normalize_phoneme + French canon)
# ============================================================

STRIP_CHARS = set("ˈˌ.ːˑ‿")
# Mirror canonicalize_for_language for French in audio_verification.rs:
# r/ʀ/ɾ/x → ʁ, ts/tɕ/tɕh → t, ɥ → y, ɑ → a.
FRENCH_CANON = {
    "r": "ʁ", "ʀ": "ʁ", "ɾ": "ʁ", "x": "ʁ",
    "ts": "t", "tɕ": "t", "tɕh": "t", "tɕʰ": "t",
    "ɥ": "y",
    "ɑ": "a",
}
NASAL = "̃"


def normalize_phoneme(tok: str):
    s = "".join(c for c in tok if c not in STRIP_CHARS and not c.isdigit() and not c.isspace())
    s = unicodedata.normalize("NFC", s)
    if not s:
        return None
    return FRENCH_CANON.get(s, s)


def align(p, e):
    """Needleman-Wunsch on phoneme sequences; returns list of (op, p_idx, e_idx)."""
    n, m = len(p), len(e)
    dp = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1): dp[i][0] = -i
    for j in range(m + 1): dp[0][j] = -j
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = 1 if p[i - 1] == e[j - 1] else -1
            dp[i][j] = max(dp[i - 1][j - 1] + s, dp[i - 1][j] - 1, dp[i][j - 1] - 1)
    ops, i, j = [], n, m
    while i > 0 or j > 0:
        if i > 0 and j > 0:
            s = 1 if p[i - 1] == e[j - 1] else -1
            if dp[i][j] == dp[i - 1][j - 1] + s:
                ops.append(("match" if s == 1 else "sub", i - 1, j - 1))
                i -= 1; j -= 1
                continue
        if i > 0 and (j == 0 or dp[i][j] == dp[i - 1][j] - 1):
            ops.append(("ins", i - 1, None))
            i -= 1
        else:
            ops.append(("del", None, j - 1))
            j -= 1
    return list(reversed(ops))


def categorize_failure(exp, pred):
    """Bucket a (expected, predicted) pair into one of the failure categories.

    Categories chosen to match the cheat-sheet we developed analyzing Jean:
      audio_defect   — model returned no phonemes
      nasal_vowel    — expected has a nasal vowel; difference involves it
      schwa_to_o     — wikipron has ə that model emits as ø (or drops)
      ʁ_variant     — model emits x/ɾ/r where ʁ is expected
      e_vs_ɛ        — single sub between e and ɛ
      a_vs_ɑ        — single sub between a and ɑ (after canon this won't fire)
      extra_phonemes — pred strictly longer than exp
      missing_phoneme — pred strictly shorter than exp (and not just trailing schwa)
      phrase_multi  — multi-word phrase (>6 expected tokens)
      other         — everything else
    """
    if not pred:
        return "audio_defect"
    if not exp:
        return "other"
    if len(exp) > 6:
        return "phrase_multi"
    # Nasal: difference involves combining tilde
    if any(NASAL in t for t in exp):
        return "nasal_vowel"
    # Schwa-to-rounded or schwa dropped
    if any(t == "ə" for t in exp):
        rew = [("ø" if t == "ə" else t) for t in exp]
        if rew == pred:
            return "schwa_to_o"
        dropped = [t for t in exp if t != "ə"]
        if dropped == pred:
            return "schwa_to_o"
        for keep_idx in range(len(exp)):
            test = [("ø" if (t == "ə" and i == keep_idx) else t)
                    for i, t in enumerate(exp)]
            if test == pred:
                return "schwa_to_o"
            test_dropped = [t for i, t in enumerate(test)
                            if not (i != keep_idx and t == "ə")]
            if test_dropped == pred:
                return "schwa_to_o"
    # Single-sub patterns
    if len(exp) == len(pred):
        diffs = [(e, p) for e, p in zip(exp, pred) if e != p]
        if len(diffs) == 1:
            e, p = diffs[0]
            if {e, p} == {"e", "ɛ"}:
                return "e_vs_ɛ"
            if {e, p} == {"a", "ɑ"}:
                return "a_vs_ɑ"
            if e == "ʁ" and p in {"x", "ɾ", "r"}:
                return "ʁ_variant"
    # Generic R variant anywhere
    if "ʁ" in exp and (set(pred) - set(exp)) & {"x", "ɾ", "r"}:
        return "ʁ_variant"
    if len(pred) > len(exp):
        return "extra_phonemes"
    if len(pred) < len(exp):
        return "missing_phoneme"
    return "other"


# ============================================================
# Diff entry points
# ============================================================

def load_dump(path: str, actor_filter: str | None = None):
    out = {}
    for line in open(path):
        d = json.loads(line)
        if actor_filter and d.get("actor") != actor_filter:
            continue
        out[d["wav"]] = d
    return out


def per_stats(dump: dict, actor: str | None = None, keys: set | None = None):
    """Phoneme Error Rate: sum(edit_distance) / sum(len(expected)) over scoreable clips.

    A scoreable clip is one with non-empty `expected` and a non-None
    `edit_distance` — i.e., one where the verifier actually ran an
    alignment. Clips that got "no ground truth available" are skipped
    (they shouldn't be counted as either correct or incorrect — they're
    unmeasurable).

    `keys` (optional) restricts the calculation to a specific subset
    of clip paths — pass the head-to-head shared key set so PER and
    pass-rate are computed over the same population.

    Returns (per_pct, edits, ref_len, n_scoreable, n_skipped).
    """
    edits = ref_len = n_clips = n_skipped = 0
    items = ((k, d) for k, d in dump.items() if keys is None or k in keys)
    for _k, d in items:
        if actor and d.get("actor") != actor:
            continue
        exp = d.get("expected")
        ed = d.get("edit_distance")
        if not exp or ed is None:
            n_skipped += 1
            continue
        edits += ed
        ref_len += len(exp)
        n_clips += 1
    pct = (100.0 * edits / ref_len) if ref_len else 0.0
    return pct, edits, ref_len, n_clips, n_skipped


def render_diff(label_a: str, label_b: str, dump_a: str, dump_b: str,
                actor_filter: str | None = None):
    a = load_dump(dump_a, actor_filter)
    b = load_dump(dump_b, actor_filter)
    shared = set(a) & set(b)
    if not shared:
        print(f"No shared clips between {dump_a} and {dump_b}"
              f"{' for actor ' + actor_filter if actor_filter else ''}")
        return

    both_pass = sum(1 for k in shared if a[k]["passed"] and b[k]["passed"])
    both_fail = sum(1 for k in shared if not a[k]["passed"] and not b[k]["passed"])
    a_only = sorted(k for k in shared if a[k]["passed"] and not b[k]["passed"])
    b_only = sorted(k for k in shared if not a[k]["passed"] and b[k]["passed"])
    a_pass = both_pass + len(a_only)
    b_pass = both_pass + len(b_only)

    print(f"\n# {label_a}  vs  {label_b}")
    if actor_filter:
        print(f"\n_Filter: actor = `{actor_filter}`_")
    # PER on the shared clip set only — apples-to-apples with the pass counts.
    per_a, ea, ra, _, _ = per_stats(a, actor_filter, keys=shared)
    per_b, eb, rb, _, _ = per_stats(b, actor_filter, keys=shared)

    print(f"\n| metric | {label_a} | {label_b} |")
    print(f"|---|---|---|")
    print(f"| total pass @ threshold=0 | **{a_pass}** ({100*a_pass/len(shared):.1f}%) | "
          f"**{b_pass}** ({100*b_pass/len(shared):.1f}%) |")
    print(f"| **PER** (edit_dist / ref_len) | **{per_a:.2f}%** ({ea}/{ra}) | "
          f"**{per_b:.2f}%** ({eb}/{rb}) |")
    print(f"| both pass | {both_pass} | |")
    print(f"| both fail | {both_fail} | |")
    print(f"| A-only pass (B regressions) | {len(a_only)} | |")
    print(f"| B-only pass (B recoveries)  | | {len(b_only)} |")
    print(f"| **net pass (B − A)** | | **{b_pass - a_pass:+d}** |")
    print(f"| **net PER (B − A)** | | **{per_b - per_a:+.2f}pp** |")

    # If multiple actors present and no actor filter, also break PER out per-actor
    if not actor_filter:
        actors_a = {d["actor"] for d in a.values() if d.get("actor")}
        actors_b = {d["actor"] for d in b.values() if d.get("actor")}
        actors = sorted(actors_a | actors_b)
        if len(actors) > 1:
            print(f"\n**Per-actor PER:**\n")
            print(f"| actor | {label_a} | {label_b} | Δ |")
            print(f"|---|---|---|---|")
            for ac in actors:
                pa, *_ = per_stats(a, ac, keys=shared)
                pb, *_ = per_stats(b, ac, keys=shared)
                print(f"| {ac} | {pa:.2f}% | {pb:.2f}% | {pb-pa:+.2f}pp |")

    def both_fail_category_summary():
        if both_fail == 0:
            return
        buckets = defaultdict(int)
        for k in shared:
            if a[k]["passed"] or b[k]["passed"]:
                continue
            # Both fail; characterize using one of them (they may differ in
            # specifics but tend to fail for similar reasons).
            exp = [normalize_phoneme(t) for t in (b[k]["expected"] or []) if normalize_phoneme(t)]
            pred = [normalize_phoneme(t) for t in b[k]["predicted"] if normalize_phoneme(t)]
            buckets[categorize_failure(exp, pred)] += 1
        print(f"\n## Shared failures (both A and B fail) — {both_fail} clips\n")
        print(f"| category | count |")
        print(f"|---|---|")
        for cat, n in sorted(buckets.items(), key=lambda x: -x[1]):
            print(f"| {cat} | {n} |")

    def render_category(label, loser_dict, winner_dict, clips):
        if not clips:
            return
        buckets = defaultdict(list)
        for c in clips:
            exp = [normalize_phoneme(t) for t in (loser_dict[c]["expected"] or [])
                   if normalize_phoneme(t)]
            loser_n = [normalize_phoneme(t) for t in loser_dict[c]["predicted"]
                       if normalize_phoneme(t)]
            winner_n = [normalize_phoneme(t) for t in winner_dict[c]["predicted"]
                        if normalize_phoneme(t)]
            cat = categorize_failure(exp, loser_n)
            actor = c.rsplit("/", 2)[-2] if "/" in c else ""
            buckets[cat].append({
                "clip": f"{actor}/{c.rsplit('/', 1)[-1]}" if actor else c,
                "text": loser_dict[c]["text"],
                "exp": " ".join(exp),
                "loser": " ".join(loser_n),
                "winner": " ".join(winner_n),
            })
        print(f"\n## {label} — {len(clips)} clips, by category\n")
        for cat, rows in sorted(buckets.items(), key=lambda x: -len(x[1])):
            print(f"### {cat} ({len(rows)})\n")
            print(f"| clip | text | expected | loser | winner |")
            print(f"|---|---|---|---|---|")
            for r in rows:
                print(f"| `{r['clip']}` | `{r['text']}` | `{r['exp']}` | "
                      f"`{r['loser']}` | `{r['winner']}` |")
            print()

    render_category(f"{label_b} REGRESSIONS (A ✓ B ✗)", b, a, a_only)
    render_category(f"{label_b} RECOVERIES (B ✓ A ✗)", a, b, b_only)
    both_fail_category_summary()


# ============================================================
# Baseline save/diff
# ============================================================

def cmd_save_baseline(args):
    name = args.name
    model_id, revision = resolve_model(args.model)
    BASELINES.mkdir(exist_ok=True)
    target = BASELINES / f"{name}.jsonl"
    if target.exists():
        if not args.force:
            sys.exit(f"baseline {target} already exists; use --force to overwrite")
    ensure_bin()
    dump_path = populate(model_id, revision, args.lang)
    shutil.copy(dump_path, target)
    meta_target = BASELINES / f"{name}.meta.json"
    meta_target.write_text(json.dumps({
        "model_id": model_id, "revision": revision,
        "saved_at": int(time.time()),
        "lang": args.lang,
    }, indent=2))
    print(f"\nBaseline saved: {target}")


def cmd_diff_baseline(args):
    target = BASELINES / f"{args.name}.jsonl"
    if not target.exists():
        sys.exit(f"baseline {target} does not exist; list: {[p.stem for p in BASELINES.glob('*.jsonl')]}")
    ensure_bin()
    model_id, revision = resolve_model(args.model)
    dump_path = populate(model_id, revision, args.lang)
    render_diff(label_a=args.name, label_b=safe_name(model_id, revision).split("__")[-1] or args.model,
                dump_a=str(target), dump_b=dump_path, actor_filter=args.actor)


# ============================================================
# N-way comparison
# ============================================================

def render_multi(labels: list[str], dump_paths: list[str], actor_filter: str | None = None):
    """Compare N models on the same clip set at threshold=0.

    Produces (a) per-model pass totals, (b) the unique-pass cohort for each
    model (clips ONLY that model gets right), (c) clips no model gets right,
    and (d) per-pair pairwise pass-rate matrix.
    """
    dumps = [load_dump(p, actor_filter) for p in dump_paths]
    shared = set.intersection(*(set(d) for d in dumps))
    if not shared:
        print("No shared clips."); return
    n = len(labels)

    print(f"\n# {n}-way comparison ({len(shared)} clips)")
    if actor_filter:
        print(f"\n_Filter: actor = `{actor_filter}`_")

    # Per-model totals + PER, computed over the shared clip set so the
    # two metrics report on the same population.
    totals = [sum(1 for k in shared if d[k]["passed"]) for d in dumps]
    pers = [per_stats(d, actor_filter, keys=shared)[0] for d in dumps]
    print(f"\n## Per-model pass totals @ threshold=0\n")
    print(f"| model | pass | pct | PER |")
    print(f"|---|---|---|---|")
    # Rank by PER (lower is better); fall back to pass count for ties
    ranking = sorted(range(n), key=lambda i: (pers[i], -totals[i]))
    for rank, i in enumerate(ranking, 1):
        print(f"| {rank}. {labels[i]} | **{totals[i]}** | {100*totals[i]/len(shared):.1f}% | **{pers[i]:.2f}%** |")

    # Pairwise net deltas — interesting "swap" diagonal
    print(f"\n## Pairwise net (row vs col): row_pass − col_pass\n")
    header = "| | " + " | ".join(labels) + " |"
    sep = "|" + "---|" * (n + 1)
    print(header)
    print(sep)
    for i in range(n):
        row = [labels[i]]
        for j in range(n):
            if i == j:
                row.append("—")
            else:
                row.append(f"{totals[i] - totals[j]:+d}")
        print("| " + " | ".join(row) + " |")

    # Cohort analysis: clips passed only by one model, by all models, by none
    pass_sets = [set(k for k in shared if d[k]["passed"]) for d in dumps]
    all_pass = set.intersection(*pass_sets)
    none_pass = shared - set.union(*pass_sets)
    print(f"\n## Cohort sizes\n")
    print(f"- **all pass**: {len(all_pass)} clips (universally easy)")
    print(f"- **none pass**: {len(none_pass)} clips (universally hard)")
    for i, lab in enumerate(labels):
        only_i = pass_sets[i] - set.union(*(pass_sets[j] for j in range(n) if j != i))
        print(f"- **only {lab}**: {len(only_i)} clips (passes here, fails everywhere else)")

    # Categorize the universally-hard clips — that's the irreducible failure set
    if none_pass:
        from collections import defaultdict
        buckets = defaultdict(int)
        ref = dumps[0]
        for k in none_pass:
            exp = [normalize_phoneme(t) for t in (ref[k]["expected"] or []) if normalize_phoneme(t)]
            pred = [normalize_phoneme(t) for t in ref[k]["predicted"] if normalize_phoneme(t)]
            buckets[categorize_failure(exp, pred)] += 1
        print(f"\n## Universally-hard clips ({len(none_pass)}) by category (per model A)\n")
        print(f"| category | count |")
        print(f"|---|---|")
        for cat, c in sorted(buckets.items(), key=lambda x: -x[1]):
            print(f"| {cat} | {c} |")


def cmd_multi(args):
    if len(args.models) < 2:
        sys.exit("multi requires at least 2 models (use `compare` for 1-vs-baseline)")
    ensure_bin()
    models = [resolve_model(m) for m in args.models]
    dump_paths = []
    labels = []
    if not args.skip_deploy:
        for model_id, revision in models:
            dump_paths.append(populate(model_id, revision, args.lang))
            labels.append(model_id.split("/")[-1] +
                          (f"@{revision[:8]}" if revision != "main" else ""))
    else:
        for model_id, revision in models:
            sn = safe_name(model_id, revision)
            p = f"/tmp/per_clip_{sn}.jsonl"
            if not os.path.exists(p):
                sys.exit(f"--skip-deploy needs existing {p}")
            dump_paths.append(p)
            labels.append(model_id.split("/")[-1] +
                          (f"@{revision[:8]}" if revision != "main" else ""))
    render_multi(labels, dump_paths, actor_filter=args.actor)


# ============================================================
# Main compare
# ============================================================

def cmd_compare(args):
    ensure_bin()
    model_a, rev_a = resolve_model(args.model_a)
    model_b, rev_b = resolve_model(args.model_b)
    sn_a = safe_name(model_a, rev_a)
    sn_b = safe_name(model_b, rev_b)
    dump_a = f"/tmp/per_clip_{sn_a}.jsonl"
    dump_b = f"/tmp/per_clip_{sn_b}.jsonl"
    if not args.skip_deploy:
        populate(model_a, rev_a, args.lang)
        populate(model_b, rev_b, args.lang)
    else:
        missing = [p for p in (dump_a, dump_b) if not os.path.exists(p)]
        if missing:
            sys.exit(f"--skip-deploy needs existing dumps: {missing}")
    render_diff(label_a=model_a.split("/")[-1] + (f"@{rev_a[:8]}" if rev_a != "main" else ""),
                label_b=model_b.split("/")[-1] + (f"@{rev_b[:8]}" if rev_b != "main" else ""),
                dump_a=dump_a, dump_b=dump_b, actor_filter=args.actor)


# ============================================================
# CLI
# ============================================================

SUBCMDS = {"compare", "save-baseline", "diff-baseline", "multi"}


def main():
    # Default-to-compare: if first arg isn't a known subcommand, treat the
    # invocation as `compare ...` so the common case stays a one-liner.
    argv = sys.argv[1:]
    if argv and argv[0] not in SUBCMDS and not argv[0].startswith("-"):
        argv = ["compare", *argv]

    ap = argparse.ArgumentParser(description=__doc__)
    sp = ap.add_subparsers(dest="cmd", required=True)

    p_compare = sp.add_parser("compare", help="Compare two models (default)")
    p_compare.add_argument("model_a")
    p_compare.add_argument("model_b")
    p_compare.add_argument("--lang", default="generate-data/data/fra")
    p_compare.add_argument("--actor", help="Filter to one actor (e.g. Jean-Cavard)")
    p_compare.add_argument("--skip-deploy", action="store_true",
                           help="Reuse existing dumps without redeploying")
    p_compare.set_defaults(func=cmd_compare)

    p_save = sp.add_parser("save-baseline", help="Tag a model run as a named baseline")
    p_save.add_argument("name", help="Baseline name (no extension)")
    p_save.add_argument("model", help="Model id, optionally with @<sha>")
    p_save.add_argument("--lang", default="generate-data/data/fra")
    p_save.add_argument("--force", action="store_true", help="Overwrite existing")
    p_save.set_defaults(func=cmd_save_baseline)

    p_diff = sp.add_parser("diff-baseline", help="Diff a model run against a saved baseline")
    p_diff.add_argument("name", help="Baseline name")
    p_diff.add_argument("model", help="Model id to test, optionally with @<sha>")
    p_diff.add_argument("--lang", default="generate-data/data/fra")
    p_diff.add_argument("--actor", help="Filter to one actor")
    p_diff.set_defaults(func=cmd_diff_baseline)

    p_multi = sp.add_parser("multi", help="N-way model comparison (3+ models)")
    # nargs=2+ enforced post-parse (argparse has no native ≥2 syntax). Single-
    # model "multi" runs would crash in render_multi's set.union(*).
    p_multi.add_argument("models", nargs="+", help="2+ model ids, optionally with @<sha>")
    p_multi.add_argument("--lang", default="generate-data/data/fra")
    p_multi.add_argument("--actor", help="Filter to one actor")
    p_multi.add_argument("--skip-deploy", action="store_true",
                         help="Reuse existing dumps without redeploying")
    p_multi.set_defaults(func=cmd_multi)

    args = ap.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
