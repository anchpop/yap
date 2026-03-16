//! Supertoken discovery, whitespace diagnostics, and sentence encoding

use language_utils::{Atom, Gram, Language, Literal, literals_to_atoms, predict_whitespace};
use omnigram::WhitespacePredictionSummary;
use omnigram::unigram::{UnigramTrainer, UnigramTrainerConfig};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Train supertokens, encode sentences, and write all outputs for a language
pub fn train_supertokens_and_write_diagnostics(
    nlp_sentences: &BTreeMap<String, Vec<Literal<String>>>,
    language: Language,
    output_dir: &Path,
    seed_grams: &[Gram<String>],
) {
    // Analyze whitespace prediction accuracy
    let mut ws_summary = WhitespacePredictionSummary::new();
    for (sentence_text, words) in nlp_sentences.iter() {
        ws_summary.add_sentence(words, sentence_text, language);
    }

    // Write diagnostic report
    let diagnostics_file = output_dir.join("whitespace_diagnostics.md");
    let report = ws_summary.generate_report();
    std::fs::write(&diagnostics_file, &report).expect("Failed to write whitespace diagnostics");

    // Convert all sentences to atom sequences (preserving sentence text for output)
    let sentences_with_atoms: Vec<(&String, Vec<Atom<String>>, bool)> = nlp_sentences
        .iter()
        .map(|(text, words)| {
            let (atoms, capitalize_first) = literals_to_atoms(words, language);
            (text, atoms, capitalize_first)
        })
        .collect();

    let corpus: Vec<Vec<Atom<String>>> = sentences_with_atoms
        .iter()
        .map(|(_, atoms, _)| atoms.clone())
        .collect();

    // Count unique single atoms to determine target multiword count
    let unique_atoms: HashSet<_> = corpus
        .iter()
        .flat_map(|sentence| sentence.iter().cloned())
        .collect();
    let single_atom_count = unique_atoms.len();

    // Target multiword tokens = 33% of base token count
    let target_multiword_tokens = (single_atom_count * 33) / 100;

    let config = UnigramTrainerConfig {
        target_multiword_tokens,
        max_piece_length: 8,
        shrinking_factor: 0.75,
        min_frequency: 3,
        em_iterations: 10,
        merge_alpha: 0.0,
    };

    let trainer = UnigramTrainer::new(config);
    let model = trainer.train(&corpus, seed_grams);

    // Write vocabulary file (maps token ID to atom sequence)
    write_vocabulary(&model, language, output_dir);

    // Write human-readable supertokens file
    write_supertokens_txt(&model, language, output_dir);

    // Encode all sentences and write to file
    write_encoded_sentences(&sentences_with_atoms, &model, output_dir);
}

/// Write the vocabulary file mapping token IDs to their atom sequences
fn write_vocabulary(
    model: &omnigram::unigram::UnigramModel,
    _language: Language,
    output_dir: &Path,
) {
    let vocab_file = output_dir.join("vocabulary.jsonl");
    let file = File::create(&vocab_file).expect("Failed to create vocabulary file");
    let mut writer = BufWriter::new(file);

    // Write each vocabulary entry as a JSON line (in ID order so index = ID)
    for (id, (seq, count)) in model.get_vocab_in_id_order().enumerate() {
        let entry = serde_json::json!({
            "id": id,
            "atoms": seq.0,
            "frequency": count,
        });
        writeln!(writer, "{entry}").expect("Failed to write vocabulary entry");
    }

    writer.flush().expect("Failed to flush vocabulary file");
}

/// Write human-readable supertokens file
fn write_supertokens_txt(
    model: &omnigram::unigram::UnigramModel,
    language: Language,
    output_dir: &Path,
) {
    let supertokens_file = output_dir.join("supertokens.txt");
    let mut supertokens: Vec<(String, usize, u32)> = Vec::new();

    for (seq, count) in model.get_vocab_with_counts() {
        if seq.len() >= 2 {
            // Reconstruct text with proper whitespace
            let words: Vec<_> = seq
                .0
                .iter()
                .filter_map(|atom| match atom {
                    Atom::<String>::Tok(word) => Some(word.clone()),
                    Atom::<String>::Control(_) => None,
                })
                .collect();

            let mut text = String::new();
            for (i, word) in words.iter().enumerate() {
                text.push_str(&word.text);
                if i + 1 < words.len() {
                    let ws = predict_whitespace(word, Some(&words[i + 1]), language);
                    text.push_str(ws.to_str());
                }
            }

            supertokens.push((text, seq.len(), count));
        }
    }

    let mut file_content = format!(
        "# Supertokens for {} ({} total, sorted by frequency)\n\n",
        language,
        supertokens.len()
    );
    for (text, len, count) in &supertokens {
        file_content.push_str(&format!("{text} ({len} atoms, {count} occurrences)\n"));
    }
    std::fs::write(&supertokens_file, &file_content).expect("Failed to write supertokens file");
}

/// Encode all sentences using the trained model and write to file
fn write_encoded_sentences(
    sentences_with_atoms: &[(&String, Vec<Atom<String>>, bool)],
    model: &omnigram::unigram::UnigramModel,
    output_dir: &Path,
) {
    let encoded_file = output_dir.join("encoded_sentences.jsonl");
    let file = File::create(&encoded_file).expect("Failed to create encoded sentences file");
    let mut writer = BufWriter::new(file);

    for (sentence_text, atoms, capitalize_first) in sentences_with_atoms {
        // Segment the sentence using the model
        let supertokens = model.segment(atoms);

        // Convert supertokens to token IDs
        let token_ids: Vec<u32> = supertokens
            .iter()
            .filter_map(|st| model.get_token_id(st))
            .collect();

        let entry = serde_json::json!({
            "text": sentence_text,
            "tokens": token_ids,
            "capitalize_first": capitalize_first,
        });
        writeln!(writer, "{entry}").expect("Failed to write encoded sentence");
    }

    writer
        .flush()
        .expect("Failed to flush encoded sentences file");
}
