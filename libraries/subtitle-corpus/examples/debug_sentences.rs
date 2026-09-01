//! Stage-by-stage sentence counts for one subtitle file, to localize where
//! sentences disappear. Usage: debug_sentences <srt> <lang-code>

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let srt_path = args.next().expect("srt path");
    let code = args.next().expect("lang code");
    let language = language_utils::Language::from_code(&code).unwrap();
    let srt = std::fs::read_to_string(&srt_path)?;

    let cues = subtitle_corpus::cues::parse_cues(&srt);
    println!("parse_cues: {}", cues.len());
    let lines: Vec<movie_subtitles::SubtitleLine> = cues
        .iter()
        .filter_map(|cue| {
            let text = movie_subtitles::cleanup_subtitle_text(&cue.text);
            (!text.is_empty()).then_some(movie_subtitles::SubtitleLine {
                sentence: text,
                start_ms: cue.start_ms.max(0) as u32,
                end_ms: cue.end_ms.max(0) as u32,
            })
        })
        .collect();
    println!("after cleanup: {}", lines.len());
    let passages = movie_subtitles::segment::timed_passages(&lines);
    println!("passages: {}", passages.len());
    for p in passages.iter().take(5) {
        println!("  sample passage: {:?}", &p.text[..p.text.len().min(80)]);
    }
    let segmenter = movie_subtitles::segment::SubtitleSegmenter::for_language(language)?;
    let keyed = movie_subtitles::sentences::keyed_sentences(&lines, language, &segmenter);
    println!("keyed: {}", keyed.len());
    for k in keyed.iter().take(5) {
        println!("  keyed: {:?}", k.sentence);
    }
    Ok(())
}
