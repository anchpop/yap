//! Diagnostic: why does `label_cues` find no positives in a film? Prints the
//! funnel from raw cues to labels and a few worked examples.
use std::collections::BTreeMap;
use std::path::PathBuf;

use subtitle_corpus::cues::{
    agreement_tokens, course_code_full, label_cues, load_transcript, parse_cues, tokenization_for,
    MAX_CUE_MS, MIN_CUE_MS, MIN_TOKENS,
};
use subtitle_corpus::library::read_plan;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/data/andrep/subtitle-corpus");
    let ids: Vec<String> = std::env::args().skip(1).collect();
    let plan = read_plan(&out)?;
    for movie in plan.iter().filter(|m| ids.contains(&m.imdb_id)) {
        let dir = out.join(&movie.imdb_id);
        let code = course_code_full(&movie.original_language).unwrap();
        let tokenization = tokenization_for(code);
        let srt = std::fs::read_to_string(dir.join("subtitle.srt"))?;
        let cues = parse_cues(&srt);
        let transcript = load_transcript(&dir.join("transcript.jsonl"))?;
        let (mut dur_ok, mut digit_free, mut enough) = (0, 0, 0);
        let mut shown = 0;
        for c in &cues {
            let d = c.end_ms - c.start_ms;
            if !(MIN_CUE_MS..=MAX_CUE_MS).contains(&d) {
                continue;
            }
            dur_ok += 1;
            let cleaned = movie_subtitles::cleanup_subtitle_text(&c.text);
            if cleaned.chars().any(|ch| ch.is_ascii_digit()) {
                continue;
            }
            digit_free += 1;
            let toks = agreement_tokens(&cleaned, tokenization);
            if toks.len() >= MIN_TOKENS {
                enough += 1;
            } else if shown < 5 {
                shown += 1;
                println!(
                    "  dropped: raw={:?} cleaned={:?} tokens={:?}",
                    c.text, cleaned, toks
                );
            }
        }
        let labels = label_cues(&cues, &transcript, tokenization);
        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for l in &labels {
            *by.entry(format!("{:?}", l.label)).or_default() += 1;
        }
        println!(
            "== {} {} [{code}] {:?}: {} cues → {dur_ok} in duration → {digit_free} digit-free → {enough} with ≥{MIN_TOKENS} tokens → {} labelled {:?}",
            movie.imdb_id,
            movie.title,
            tokenization,
            cues.len(),
            labels.len(),
            by
        );
        for l in labels.iter().take(6) {
            println!(
                "   {:?} wer={:.2} exact={:.2} cue={:?} heard={:?}",
                l.label,
                l.agreement_wer,
                l.exact_wer,
                l.cleaned_text.chars().take(30).collect::<String>(),
                l.heard_text.chars().take(50).collect::<String>()
            );
        }
    }
    Ok(())
}
