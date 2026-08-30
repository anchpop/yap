//! The synchronized subtitle corpus: one correctly-timed subtitle, an
//! extracted audio track and (once transcribed) a word-timed transcript per
//! film in the movie library, plus the gates that turn those into verified
//! sentence clips.
//!
//! The `subtitle-corpus` binary drives the pipeline; this library exposes the
//! pieces other tools build on (the pronunciation corpus extractor and eval
//! harness in `generate-data`).

pub mod clips;
pub mod cues;
pub mod library;
pub mod ocr;
pub mod pgs;
pub mod sync;
pub mod transcript;
pub mod vad;
pub mod vobsub;
