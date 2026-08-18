//! Supertoken discovery, whitespace diagnostics, and sentence encoding

use language_utils::{
    Atom, EncodedSentence, Gram, GramVocabEntry, Language, Literal, literals_to_atoms,
    predict_whitespace,
};
use omnigram::WhitespacePredictionSummary;
use omnigram::unigram::{Seq, UnigramTrainer, UnigramTrainerConfig};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The trained unigram model plus its interner, kept alive so sentences
/// minted after training (e.g. homophone practice) can be encoded with the
/// exact same machinery as the corpus.
pub struct SentenceEncoder {
    model: omnigram::unigram::UnigramModel<Atom<lasso::Spur>>,
    reader: lasso::RodeoReader,
}

impl SentenceEncoder {
    /// Encode a sentence's words. `None` if any atom was never seen in the
    /// training corpus (such a sentence is not expressible in the gram
    /// system) or a segment has no vocabulary id.
    pub fn encode(&self, words: &[Literal<String>], language: Language) -> Option<EncodedSentence> {
        let (atoms, capitalize_first) = literals_to_atoms(words, language);
        let interned: Vec<Atom<lasso::Spur>> = atoms
            .iter()
            .map(|a| a.get_interned(&self.reader))
            .collect::<Option<Vec<_>>>()?;
        let tokens: Vec<u32> = self
            .model
            .segment(&interned)
            .iter()
            .map(|seq| self.model.get_token_id(seq))
            .collect::<Option<Vec<_>>>()?;
        Some(EncodedSentence {
            tokens,
            capitalize_first,
        })
    }
}

/// What supertoken training produces, in memory: the vocabulary (index =
/// encoded token id), every input sentence's encoding, and an encoder for
/// sentences minted later. The on-disk files (vocabulary.jsonl,
/// encoded_sentences.jsonl, supertokens.txt, whitespace_diagnostics.md) are
/// pure outputs — nothing re-reads them in the same run.
pub struct TrainedEncoding {
    pub gram_vocabulary: Vec<GramVocabEntry<String>>,
    pub encoded_sentences: BTreeMap<String, EncodedSentence>,
    pub encoder: SentenceEncoder,
}

/// Train supertokens, encode sentences, and write all outputs for a language
pub fn train_supertokens_and_write_diagnostics(
    nlp_sentences: &BTreeMap<String, Vec<Literal<String>>>,
    language: Language,
    output_dir: &Path,
    seed_grams: &[Gram<String>],
) -> TrainedEncoding {
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

    // Build the in-memory vocabulary (index = token id) and encodings, then
    // write the files from them.
    let gram_vocabulary: Vec<GramVocabEntry<String>> = model
        .get_vocab_in_id_order()
        .map(|(seq, count)| GramVocabEntry {
            atoms: Gram::from(
                seq.0
                    .iter()
                    .map(|a| a.resolve(&reader))
                    .collect::<Vec<Atom<String>>>(),
            ),
            frequency: count,
        })
        .collect();

    let encoded_sentences: BTreeMap<String, EncodedSentence> = sentences_with_atoms
        .iter()
        .zip(interned_corpus.iter())
        .map(|((sentence_text, _, capitalize_first), interned_atoms)| {
            let tokens: Vec<u32> = model
                .segment(interned_atoms)
                .iter()
                .filter_map(|seq| model.get_token_id(seq))
                .collect();
            (
                (*sentence_text).clone(),
                EncodedSentence {
                    tokens,
                    capitalize_first: *capitalize_first,
                },
            )
        })
        .collect();

    // Write vocabulary file (maps token ID to atom sequence)
    write_vocabulary(&gram_vocabulary, output_dir);

    // Write human-readable supertokens file
    write_supertokens_txt(&model, &reader, language, output_dir);

    // Write encoded sentences to file
    write_encoded_sentences(&encoded_sentences, output_dir);

    TrainedEncoding {
        gram_vocabulary,
        encoded_sentences,
        encoder: SentenceEncoder { model, reader },
    }
}

/// Write the vocabulary file mapping token IDs to their atom sequences
fn write_vocabulary(gram_vocabulary: &[GramVocabEntry<String>], output_dir: &Path) {
    let vocab_file = output_dir.join("vocabulary.jsonl");
    let file = File::create(&vocab_file).expect("Failed to create vocabulary file");
    let mut writer = BufWriter::new(file);

    // Write each vocabulary entry as a JSON line (in ID order so index = ID)
    for (id, entry) in gram_vocabulary.iter().enumerate() {
        let entry = serde_json::json!({
            "id": id,
            "atoms": entry.atoms.0,
            "frequency": entry.frequency,
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

/// Write the encoded sentences to file
fn write_encoded_sentences(
    encoded_sentences: &BTreeMap<String, EncodedSentence>,
    output_dir: &Path,
) {
    let encoded_file = output_dir.join("encoded_sentences.jsonl");
    let file = File::create(&encoded_file).expect("Failed to create encoded sentences file");
    let mut writer = BufWriter::new(file);

    for (sentence_text, encoded) in encoded_sentences {
        let entry = serde_json::json!({
            "text": sentence_text,
            "tokens": encoded.tokens,
            "capitalize_first": encoded.capitalize_first,
        });
        writeln!(writer, "{entry}").expect("Failed to write encoded sentence");
    }

    writer
        .flush()
        .expect("Failed to flush encoded sentences file");
}
