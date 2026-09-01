# Clip sidecar schema (format 2)

One JSON per served video clip, stored next to the mp4 (`<id>.json` beside
`<id>.mp4`), immutable. Cut generously (neighbor sentences as context when the
gap is ≤ ~2s, total ≤ ~15s); the sidecar — not the file boundary — defines what
the clip *is*. The app seeks to `critical.start_ms` by default and offers the
padding as opt-in context.

Principles:

- **Two time domains.** Everything the app consumes is clip-relative ms; the
  `source` block keeps film-absolute times + provenance digests so any future
  re-cut (different padding/resolution/codec) regenerates without re-running
  alignment or transcription.
- **Include everything cheap.** The whole `Clip` verdict block goes in
  verbatim, uses imagined or not — a few hundred bytes against a megabyte of
  video. Omitting is the only expensive mistake.
- **Both witnesses, independently timed.** Official subtitle cues with their
  own timings, and the full Scribe transcript with nested character spans.
  (Scribe is requested at character granularity; char spans live in the cached
  raw responses and are recovered by re-parse, not re-transcription — see
  `transcript.rs`.)
- **Context is display-only.** Neighbor cues never passed the gates:
  `context_verified: false` is categorical.
- Phoneme alignment comes from the cached frame matrices; `target_ipa`
  reproduces only under the `corpus-v1-labels` espeak build — pin it.
- Loudnorm is measured over the critical span, applied to the whole file, so a
  loud neighbor line can't duck the target sentence. Force a keyframe at or
  just before `critical.start_ms` at encode time (seeking is keyframe-accurate).
- **Id**: `imdb_id - sha256(NFC sentence text)[:8] - occurrence index`.
  The index counts occurrences of the sentence in the *official subtitle
  file*, in cue order — all occurrences, aligned or not — so it is fixed by
  `film.subtitle_digest` alone. Never index over passing clips: a gate tweak
  would renumber later occurrences and silently reuse ids for different audio.
  Gaps in the served set are fine. (Timing-based ids were rejected: repair /
  alignment changes shift `start_ms` across rebuilds.)

Caching: every artifact skips on **provenance equality, never existence**.
`clips.jsonl` re-maps when its provenance line (inputs digests, model, gate,
G2P backend identity) differs; a G2P preflight canary fails a film loudly
before anything is written. Exported clip dirs skip only when the sidecar's
`export` stamp (sidecar format, encode recipe, clips-provenance digest,
source-video identity) equals the one computed now — else delete and
re-render; full-language runs sweep orphaned ids. Upload markers store the
three files' content hashes and re-upload on any byte change. Better to
recalculate than to trust a cache whose inputs may have moved.

```jsonc
{
  "format": 2,
  "export": {                          // the export stamp (see above) — resume key
    "sidecar_format": 2,
    "recipe": "hi h264 crf19 medium …",
    "clips_provenance": "9f2c41d08ab313e7",
    "video": { "filename": "….mkv", "bytes": 31882123456, "audio_stream": 2 }
  },
  "id": "tt0101700-3fa2c81d-0",        // imdb id + sentence hash + occurrence index (see above)
  "language": "fra",

  "film": {
    "imdb_id": "tt0101700",
    "subtitle_digest": "…",            // Provenance digests: pins exactly which
    "transcript_digest": "…"           //   subtitle + transcript this was built from
  },
  "source": {                          // FILM-ABSOLUTE milliseconds
    "sentence_start_ms": 4123500,      // Clip.start_ms / end_ms (unpadded, repaired)
    "sentence_end_ms": 4126200,
    "cut_start_ms": 4121800,           // what the mp4 actually contains, incl. context
    "cut_end_ms": 4129400,
    "pad_before_ms": 300, "pad_after_ms": 150,
    "repaired_before_ms": 0, "repaired_after_ms": 120
  },

  // ---- everything below is CLIP-RELATIVE milliseconds ----
  "critical": { "start_ms": 1400, "end_ms": 4400 },  // seek target; keeps the ~300ms lead-in

  "sentence": {
    "text": "Comme je casse tout, j'ai tout en double.",
    "course_sentence": true,             // passes should_include_sentence — in the course
    "speaker": "speaker_1@247",        // Clip.speaker (chunk-scoped id, film-local only)
    "words": [                         // ClipWord stamps, shifted clip-relative
      { "text": "Comme", "at_ms": 1700, "until_ms": 1950 }
    ]
  },

  "subtitles": [                       // ALL official cues overlapping the cut, timed
    { "text": "…", "at_ms": 0, "until_ms": 1500, "role": "context-before" },
    { "text": "Comme je casse tout…", "at_ms": 1500, "until_ms": 4300, "role": "sentence" },
    { "text": "…", "at_ms": 4600, "until_ms": 7200, "role": "context-after" }
  ],
  "context_verified": false,

  "transcript": {                      // ElevenLabs Scribe, full detail
    "words": [
      { "text": "Comme", "at_ms": 1700, "until_ms": 1950,
        "speaker": "speaker_1@247", "logprob": -0.02,
        "chars": [ { "c": "C", "at_ms": 1700, "until_ms": 1760 } ] }
    ]
  },

  "phonemes": {
    "target_ipa": ["k", "ɔ", "m"],     // espeak target (corpus-v1-labels build)
    "heard_ipa":  ["k", "ɔ", "m"],     // model's free reading
    "oov": [],
    "alignment": [                     // forced alignment from the cached frame matrix
      { "ph": "k", "at_ms": 1710, "until_ms": 1780, "logp": -0.11 }
    ]
  },

  "verification": {                    // the Clip verdict block, verbatim
    "passed": true, "reject": null,
    "transcript_wer": 0.0, "ratio": 1.9, "logp_target_per_phoneme": -0.4,
    "edge_logp_start": -0.3, "edge_logp_end": -0.5,
    "lead_speech": 0.02, "tail_speech": 0.0, "lead_rms": 0.1, "voiced": 0.61,
    "audio_event_overlap": false, "clear_before_ms": 900, "clear_after_ms": 640,
    "provenance": { "format": 3, "model": "…", "min_ratio": 0.0,
                    "min_clear_ms": 0, "min_edge_logp": 0.0,
                    "max_pad_speech": 0.0, "max_lead_rms": 1.0,
                    "min_voiced": 0.25 }
  },

  "media": {
    "duration_ms": 7600,
    "loudnorm": { "measured_i": -24.3, "gain_db": 6.3,
                  "measured_over": "critical" },
    "keyframe_at_critical": true
  }
}
```
