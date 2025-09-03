use language_utils::{FrequencyEntry, Lexeme};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

pub fn compute_frequencies(lexemes: Vec<Lexeme<String>>) -> BTreeMap<Lexeme<String>, u32> {
    let mut frequencies: BTreeMap<Lexeme<String>, u32> = BTreeMap::new();
    for lexeme in lexemes {
        *frequencies.entry(lexeme).or_insert(0) += 1;
    }
    frequencies
}

pub fn write_frequencies_file(
    frequencies: BTreeMap<Lexeme<String>, u32>,
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut frequencies: Vec<FrequencyEntry<String>> = frequencies
        .into_iter()
        .map(|(lexeme, count)| FrequencyEntry { lexeme, count })
        .collect();

    frequencies.sort_by_key(|entry| Reverse(entry.count));

    let mut file = File::create(output_path)?;

    for entry in frequencies {
        let json = serde_json::to_string(&entry)?;
        writeln!(file, "{json}")?;
    }

    Ok(())
}
