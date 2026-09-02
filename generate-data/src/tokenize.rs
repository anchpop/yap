//! Supertoken discovery, whitespace diagnostics, and sentence encoding

use language_utils::{
    Atom, EncodedSentence, Gram, GramInterners, GramVocabEntry, Language, Literal, SpurGram,
    literals_to_atoms, predict_whitespace,
};
use omnigram::WhitespacePredictionSummary;
use omnigram::unigram::{Seq, UnigramTrainer, UnigramTrainerConfig};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The trained unigram model, kept alive so sentences minted after training
/// (e.g. homophone practice) can be encoded with the exact same machinery as
/// the corpus.
pub struct SentenceEncoder {
    model: omnigram::unigram::UnigramModel<Atom<lasso::Spur>>,
}

impl SentenceEncoder {
    /// Encode a sentence's words. `None` if the sentence is not expressible
    /// in the gram system: an atom's text was never interned, an atom has no
    /// single-token vocabulary entry (e.g. an X-tagged digit — the corpus
    /// filters those sentences out before training, so they never earn a
    /// fallback entry), or a segment has no vocabulary key.
    pub fn encode(
        &self,
        words: &[Literal<String>],
        language: Language,
        interners: &GramInterners,
    ) -> Option<EncodedSentence> {
        use lasso::Key;
        let (atoms, capitalize_first) = literals_to_atoms(words, language);
        let interned: Vec<Atom<lasso::Spur>> = atoms
            .iter()
            .map(|a| a.get_interned(&interners.strings))
            .collect::<Option<Vec<_>>>()?;
        let tokens: Vec<SpurGram> = self
            .model
            .segment(&interned)?
            .iter()
            .map(|seq| {
                self.model
                    .get_token_id(seq)
                    .and_then(|id| SpurGram::try_from_usize(id as usize))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(EncodedSentence {
            tokens,
            capitalize_first,
        })
    }
}

/// What supertoken training produces, in memory: the vocabulary (index =
/// encoded token key), the interners behind it, every input sentence's
/// encoding, and an encoder for sentences minted later. The on-disk files
/// (vocabulary.jsonl, encoded_sentences.jsonl, supertokens.txt,
/// whitespace_diagnostics.md) are pure outputs — nothing re-reads them in
/// the same run.
pub struct TrainedEncoding {
    pub gram_vocabulary: Vec<GramVocabEntry<String>>,
    pub interners: GramInterners,
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

    // Build the in-memory vocabulary (index = token id) and the gram rodeo.
    // Interning in id order makes SpurGram keys and vocabulary indices
    // coincide — the assert keeps that loud (it would only fire if the
    // trainer ever emitted duplicate vocab entries).
    use lasso::Key;
    let mut gram_rodeo: lasso::Rodeo<Gram<lasso::Spur>> = lasso::Rodeo::new();
    let gram_vocabulary: Vec<GramVocabEntry<String>> = model
        .get_vocab_in_id_order()
        .enumerate()
        .map(|(id, (seq, count))| {
            let key = gram_rodeo.get_or_intern(Gram::from(seq.0.clone()));
            assert_eq!(key.into_usize(), id, "duplicate gram in trainer vocabulary");
            GramVocabEntry {
                atoms: Gram::from(
                    seq.0
                        .iter()
                        .map(|a| a.resolve(&reader))
                        .collect::<Vec<Atom<String>>>(),
                ),
                frequency: count,
            }
        })
        .collect();
    let interners = GramInterners {
        strings: reader,
        grams: gram_rodeo.into_reader(),
    };

    let encoded_sentences: BTreeMap<String, EncodedSentence> = sentences_with_atoms
        .iter()
        .zip(interned_corpus.iter())
        .map(|((sentence_text, _, capitalize_first), interned_atoms)| {
            let tokens: Vec<SpurGram> = model
                .segment(interned_atoms)
                .expect("every training-corpus atom has a single-token vocabulary entry")
                .iter()
                .filter_map(|seq| {
                    let id = model.get_token_id(seq)?;
                    SpurGram::try_from_usize(id as usize)
                })
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
    write_supertokens_txt(&model, &interners.strings, language, output_dir);

    // Write encoded sentences to file
    write_encoded_sentences(&encoded_sentences, output_dir);

    TrainedEncoding {
        gram_vocabulary,
        interners,
        encoded_sentences,
        encoder: SentenceEncoder { model },
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

    use lasso::Key;
    for (sentence_text, encoded) in encoded_sentences {
        let entry = serde_json::json!({
            "text": sentence_text,
            "tokens": encoded
                .tokens
                .iter()
                .map(|k| k.into_usize() as u32)
                .collect::<Vec<u32>>(),
            "capitalize_first": encoded.capitalize_first,
        });
        writeln!(writer, "{entry}").expect("Failed to write encoded sentence");
    }

    writer
        .flush()
        .expect("Failed to flush encoded sentences file");
}
