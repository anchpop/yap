//! Supertoken discovery, whitespace diagnostics, and sentence encoding

use language_utils::{Atom, Gram, Language, Literal, literals_to_atoms, predict_whitespace};
use omnigram::WhitespacePredictionSummary;
use omnigram::unigram::{Seq, UnigramTrainer, UnigramTrainerConfig};
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

    // Intern all atoms for fast hashing/comparison during training
    let mut rodeo = lasso::Rodeo::new();
    let interned_corpus: Vec<Vec<Atom<lasso::Spur>>> = sentences_with_atoms
        .iter()
        .map(|(_, atoms, _)| atoms.iter().map(|a| a.get_or_intern(&mut rodeo)).collect())
        .collect();
    let interned_seeds: Vec<Seq<Atom<lasso::Spur>>> = seed_grams
        .iter()
        .map(|g| Seq(g.0.iter().map(|a| a.get_or_intern(&mut rodeo)).collect()))
        .collect();
    let reader = rodeo.into_reader();

    // Count unique single atoms to determine target multiword count
    let unique_atoms: HashSet<_> = interned_corpus
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
        initial_candidate_multiplier: 20,
        merge_alpha: 0.0,
    };

    let trainer = UnigramTrainer::new(config);
    let model = trainer.train(&interned_corpus, &interned_seeds);

    // Write vocabulary file (maps token ID to atom sequence)
    write_vocabulary(&model, &reader, output_dir);

    // Write human-readable supertokens file
    write_supertokens_txt(&model, &reader, language, output_dir);

    // Encode all sentences and write to file
    write_encoded_sentences(&sentences_with_atoms, &interned_corpus, &model, output_dir);
}

/// Write the vocabulary file mapping token IDs to their atom sequences
fn write_vocabulary(
    model: &omnigram::unigram::UnigramModel<Atom<lasso::Spur>>,
    reader: &lasso::RodeoReader,
    output_dir: &Path,
) {
    let vocab_file = output_dir.join("vocabulary.jsonl");
    let file = File::create(&vocab_file).expect("Failed to create vocabulary file");
    let mut writer = BufWriter::new(file);

    // Write each vocabulary entry as a JSON line (in ID order so index = ID)
    for (id, (seq, count)) in model.get_vocab_in_id_order().enumerate() {
        let resolved_atoms: Vec<Atom<String>> = seq.0.iter().map(|a| a.resolve(reader)).collect();
        let entry = serde_json::json!({
            "id": id,
            "atoms": resolved_atoms,
            "frequency": count,
        });
        writeln!(writer, "{entry}").expect("Failed to write vocabulary entry");
    }

    writer.flush().expect("Failed to flush vocabulary file");
}

/// Write human-readable supertokens file
fn write_supertokens_txt(
    model: &omnigram::unigram::UnigramModel<Atom<lasso::Spur>>,
    reader: &lasso::RodeoReader,
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
                    Atom::Tok(word) => Some(word.resolve(reader)),
                    Atom::Control(_) => None,
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
    interned_corpus: &[Vec<Atom<lasso::Spur>>],
    model: &omnigram::unigram::UnigramModel<Atom<lasso::Spur>>,
    output_dir: &Path,
) {
    let encoded_file = output_dir.join("encoded_sentences.jsonl");
    let file = File::create(&encoded_file).expect("Failed to create encoded sentences file");
    let mut writer = BufWriter::new(file);

    for ((sentence_text, _, capitalize_first), interned_atoms) in
        sentences_with_atoms.iter().zip(interned_corpus.iter())
    {
        // Segment the sentence using the model
        let segments = model.segment(interned_atoms);

        // Convert segments to token IDs
        let token_ids: Vec<u32> = segments
            .iter()
            .filter_map(|seq| model.get_token_id(seq))
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
