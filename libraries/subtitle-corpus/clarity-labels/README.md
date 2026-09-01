# clarity-labels

Human audio-clarity labeling site for film clips, plus collected label data.
Live instance runs from `/data/andrep/subtitle-corpus/clarity-labels/` on the
NixOS box (`python server.py`, port 8765, exposed via Tailscale Funnel).
This directory is a snapshot for safekeeping — the live dir is the source of
truth while labeling is ongoing; re-copy `labels-*.json`, `labels-*.log.jsonl`
and `corrections-*.json` here to update.

- `index.html` / `server.py` — the labeling UI and its merge-on-POST server
  (POSTs merge into the file, incoming keys win; every POST journaled).
- `manifest.json` — the 300-clip labeling set. Hidden band composition
  (labelers don't see this): pass-random 90, pass-borderline-voice 70,
  whisper-reject 55, ratio-marginal 55, pad-reject 30.
- `labels-greg.json` — merged labels per labeler; keys are clip ids.
- `labels-greg.log.jsonl` — append-only journal of every POST (recovery).
- `corrections-greg.json` — analysis-time overrides; apply LAST, they win over
  the labels file (clients can re-post stale cached labels).
- `backups/` — point-in-time snapshots.

Label semantics: the `overall` axis (beginner/advanced/hard) is **audio-based
student suitability only** — linguistic difficulty must not lower it. Labels
from greg before the evening of 2026-08-31 may mix in word difficulty.
`easy`/`hard` values predate the beginner/advanced split; `noise` predates the
minor-noise/bothersome-noise split. `volume_db` records the playback gain the
labeler had set when labeling.

The mp3s themselves are not checked in: they are regenerated with
`subtitle-corpus clips` from the corpus and then loudness-normalized
(two-pass EBU R128 loudnorm, I=-18 linear). Originals live alongside the live
dir in `clips-orig/`.
