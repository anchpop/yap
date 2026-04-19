use std::sync::Arc;

use language_utils::{
    Atom, Gram, Heteronym, Literal, PatternPosition, SentenceGram, SpurGram, TtsProvider,
    TtsRequest, WordType, atoms_to_literals, language_pack::LanguagePack, literals_to_text,
    transcription_challenge,
};
use lasso::Spur;

use crate::{
    AudioRequest, CardContent, CardIndicator, Challenge, ComprehensibleSentence, Deck, FlashCard,
    ReviewInfo, SentenceChallengeType, TranscribeComprehensibleSentence,
    TranslateComprehensibleSentence,
    dictionary::{compute_breakdown, compute_word_prefix_and_morphology},
};

/// Metadata computed from a CardIndicator and Deck, used to build FlashCardReview
pub struct CardContext {
    pub indicator: CardIndicator<SpurGram, Spur>,
    pub is_new: bool,
    pub times_type_seen: u32,
}

impl CardContext {
    pub fn new(deck: &Deck, indicator: CardIndicator<SpurGram, Spur>) -> Option<Self> {
        let is_new = deck.cards.get(&indicator)?.is_new();
        let times_type_seen = indicator
            .get_flashcard_type()
            .and_then(|ft| deck.stats.flashcard_type_seen_count.get(&ft).copied())
            .unwrap_or(0);
        Some(CardContext {
            indicator,
            is_new,
            times_type_seen,
        })
    }

    /// Wrap a FlashCard in a Challenge::FlashCardReview with this context's metadata
    pub fn wrap_flashcard(&self, deck: &Deck, flashcard: FlashCard) -> Challenge<Gram<String>> {
        let language_pack = &deck.context.language_pack;
        Challenge::FlashCardReview {
            indicator: self
                .indicator
                .resolve(&language_pack.string_rodeo, &language_pack.gram_rodeo),
            flashcard,
            is_new: self.is_new,
            times_type_seen: self.times_type_seen,
        }
    }
}

impl ReviewInfo {
    pub fn listening_gram_flashcard(&self, deck: &Deck, gram: SpurGram) -> FlashCard {
        let language_pack: &Arc<LanguagePack> = &deck.context.language_pack;

        let gram_atoms = language_pack.gram_rodeo.resolve(&gram);

        // Check if this is a single-heteronym gram (for homophone lookup)
        let single_heteronym: Option<Heteronym<Spur>> = match gram_atoms.atoms() {
            [Atom::Tok(word)] => match &word.word_type {
                WordType::Heteronym(het) => Some(*het),
                _ => None,
            },
            _ => None,
        };

        // Build possible_grams based on whether this is a single-atom or multi-atom gram
        let possible_grams: Vec<(
            bool,
            Vec<Literal<String>>,
            Vec<language_utils::GramDefinition>,
        )> = if let Some(heteronym) = single_heteronym {
            // Single-atom gram: find homophones (other grams with same pronunciation)
            let pronunciation = language_pack
                .word_to_pronunciation
                .get(&heteronym.word)
                .copied();

            if let Some(pronunciation) = pronunciation {
                // Get all words with the same pronunciation
                let homophone_words = language_pack
                    .pronunciation_to_words
                    .get(&pronunciation)
                    .cloned()
                    .unwrap_or_default();

                // For each word, find grams and check if user knows them
                homophone_words
                    .iter()
                    .flat_map(|word| {
                        // Get heteronyms for this word
                        language_pack
                            .words_to_heteronyms
                            .get(word)
                            .into_iter()
                            .flatten()
                    })
                    .flat_map(|het| {
                        // Get grams for this heteronym
                        language_pack
                            .heteronym_to_grams
                            .get(het)
                            .into_iter()
                            .flatten()
                            .copied()
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(|other_gram| {
                        // Check if user knows this gram (has a non-new WrittenGram card)
                        let gram_known = deck
                            .cards
                            .get(&CardIndicator::WrittenGram { gram: other_gram })
                            .is_some_and(|card_data| !card_data.is_new());

                        // Convert gram to literals (already String)
                        let gram_resolved = language_pack
                            .gram_rodeo
                            .resolve(&other_gram)
                            .resolve(&language_pack.string_rodeo);
                        let literals = atoms_to_literals(
                            gram_resolved.as_ref(),
                            deck.context.course.target_language,
                        );

                        let definitions = language_pack
                            .gram_definitions
                            .get(&other_gram)
                            .cloned()
                            .into_iter()
                            .collect();

                        (gram_known, literals, definitions)
                    })
                    .collect()
            } else {
                // No pronunciation found, just show target gram
                let gram_resolved = gram_atoms.resolve(&language_pack.string_rodeo);
                let literals =
                    atoms_to_literals(gram_resolved.as_ref(), deck.context.course.target_language);
                let definitions = language_pack
                    .gram_definitions
                    .get(&gram)
                    .cloned()
                    .into_iter()
                    .collect();
                vec![(true, literals, definitions)]
            }
        } else {
            // Multi-atom gram: no homophones, just show this one
            let gram_resolved = gram_atoms.resolve(&language_pack.string_rodeo);
            let literals =
                atoms_to_literals(gram_resolved.as_ref(), deck.context.course.target_language);
            let definitions = language_pack
                .gram_definitions
                .get(&gram)
                .cloned()
                .into_iter()
                .collect();
            vec![(true, literals, definitions)]
        };

        // Deduplicate by display text, preserving order, keeping known=true if any duplicate is known
        let possible_grams = {
            let mut seen: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            let mut deduped: Vec<(
                bool,
                Vec<Literal<String>>,
                Vec<language_utils::GramDefinition>,
            )> = Vec::new();
            for (known, literals, definitions) in possible_grams {
                let display = literals_to_text(&literals);
                if let Some(&idx) = seen.get(&display) {
                    deduped[idx].0 |= known;
                    deduped[idx].2.extend(definitions);
                } else {
                    seen.insert(display, deduped.len());
                    deduped.push((known, literals, definitions));
                }
            }
            deduped
        };

        let content = CardContent::Listening { possible_grams };

        // Get audio text from the gram (use the target gram)
        let gram_resolved = gram_atoms.resolve(&language_pack.string_rodeo);
        let audio_text = gram_resolved.to_display_string(deck.context.course.target_language);
        let audio = AudioRequest {
            request: TtsRequest {
                text: audio_text,
                language: deck.context.course.target_language,
                is_ssml: false,
                instructions: None,
                speed: 1.0,
            },
            provider: TtsProvider::Google,
        };

        FlashCard {
            content,
            audio: Some(audio),
        }
    }

    pub fn listening_gram_transcription_challenge(
        &self,
        deck: &Deck,
        gram: SpurGram,
    ) -> Option<Challenge<Gram<String>>> {
        let language_pack: &Arc<LanguagePack> = &deck.context.language_pack;
        let sentence = {
            let comprehensible_grams = deck.get_comprehensible_written_grams(false);
            let sentence = deck.get_comprehensible_sentence_containing(
                Some(&gram),
                comprehensible_grams,
                &deck.stats.sentences_reviewed,
                language_pack,
            )?;
            // Only use sentences where the gram is a regular sentence gram,
            // not just a multiword term. The transcription event handler only
            // reviews regular grams, so multiword-term-only matches would
            // never mark the card as reviewed.
            if !sentence
                .target_language_sentence_grams
                .grams
                .iter()
                .any(|g| g.learnable() == Some(&gram))
            {
                return None;
            }
            sentence
        };

        let sentence_grams = sentence.target_language_sentence_grams.to_literals(
            &language_pack.string_rodeo,
            &language_pack.gram_rodeo,
            deck.context.course.target_language,
        );

        let mut parts = Vec::<transcription_challenge::Part>::new();
        for sentence_gram in sentence_grams {
            match sentence_gram {
                SentenceGram::Learnable((sentence_gram, literals))
                    if sentence_gram == gram
                        || deck.is_listened_gram_comprehensible(&sentence_gram, false) =>
                {
                    if let Some(transcription_challenge::Part::AskedToTranscribe {
                        parts: existing_parts,
                    }) = parts.last_mut()
                    {
                        existing_parts.extend(literals)
                    } else {
                        parts.push(transcription_challenge::Part::AskedToTranscribe {
                            parts: literals,
                        });
                    }
                }
                SentenceGram::Obvious((_, literals)) | SentenceGram::Learnable((_, literals)) => {
                    for literal in literals {
                        parts.push(transcription_challenge::Part::Provided { part: literal });
                    }
                }
            }
        }

        // Get movie titles from sentence_sources and movie metadata
        let movie_titles = language_pack
            .sentence_sources
            .get(&sentence.target_language)
            .map(|source| {
                source
                    .movie_ids
                    .iter()
                    .filter_map(|movie_id| {
                        language_pack
                            .movies
                            .get(movie_id)
                            .map(|metadata| (movie_id.clone(), metadata.title.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Get proper noun definitions from all literals in the parts
        let proper_noun_definitions: Vec<(String, language_utils::ProperNounDefinition)> = parts
            .iter()
            .flat_map(|part| match part {
                transcription_challenge::Part::AskedToTranscribe { parts } => parts.iter(),
                transcription_challenge::Part::Provided { part } => {
                    std::slice::from_ref(part).iter()
                }
            })
            .filter_map(|literal| {
                if let language_utils::WordType::Other(other) = &literal.word.word_type
                    && other.other_tag == language_utils::OtherWordType::Propn
                {
                    let text_spur = language_pack.string_rodeo.get(&literal.word.text)?;
                    language_pack
                        .proper_noun_definitions
                        .get(&text_spur)
                        .map(|def| (literal.word.text.clone(), def.clone()))
                } else {
                    None
                }
            })
            .collect();

        let second_chance = deck.stats.wrong_sentences.iter().any(|(s, t)| {
            *s == sentence.target_language && *t == SentenceChallengeType::Transcription
        });

        Some(Challenge::TranscribeComprehensibleSentence(
            TranscribeComprehensibleSentence {
                target_language: language_pack
                    .string_rodeo
                    .resolve(&sentence.target_language)
                    .to_string(),
                native_language: language_pack
                    .string_rodeo
                    .resolve(sentence.native_languages.first()?)
                    .to_string(),
                parts,
                audio: AudioRequest {
                    request: TtsRequest {
                        text: language_pack
                            .string_rodeo
                            .resolve(&sentence.target_language)
                            .to_string(),
                        language: deck.context.course.target_language,
                        is_ssml: false,
                        instructions: None,
                        speed: 1.0,
                    },
                    provider: TtsProvider::Google,
                },
                movie_titles,
                proper_noun_definitions,
                second_chance,
            },
        ))
    }

    pub fn listening_gram_challenge(
        &self,
        deck: &Deck,
        ctx: &CardContext,
        gram: SpurGram,
    ) -> Challenge<Gram<String>> {
        if let Some(challenge) = self.listening_gram_transcription_challenge(deck, gram) {
            challenge
        } else {
            let flashcard = self.listening_gram_flashcard(deck, gram);
            ctx.wrap_flashcard(deck, flashcard)
        }
    }

    pub fn written_gram_flashcard(&self, deck: &Deck, gram: SpurGram) -> FlashCard {
        let language_pack: &Arc<LanguagePack> = &deck.context.language_pack;

        // Get definition and convert gram to literals
        let definition = language_pack
            .gram_definitions
            .get(&gram)
            .cloned()
            .unwrap_or_else(|| {
                let resolved = language_pack
                    .gram_rodeo
                    .resolve(&gram)
                    .resolve(&language_pack.string_rodeo);
                panic!(
                    "Gram {:?} (display: {:?}) has no definition",
                    resolved,
                    resolved.to_display_string(deck.context.course.target_language)
                )
            });

        let gram_resolved = language_pack
            .gram_rodeo
            .resolve(&gram)
            .resolve(&language_pack.string_rodeo);
        let literals =
            atoms_to_literals(gram_resolved.as_ref(), deck.context.course.target_language);

        let (prefix, _morphology) = compute_word_prefix_and_morphology(
            &gram_resolved,
            &definition,
            deck.context.course.target_language,
        );

        // Morpheme-level breakdown for single heteronyms, word-level for
        // multi-atom grams. Punctuation atoms in multi-word grams render with
        // `None` gloss.
        let breakdown = compute_breakdown(language_pack.gram_rodeo.resolve(&gram), language_pack);

        let content = CardContent::Gram {
            gram: literals,
            definition,
            prefix,
            breakdown,
        };

        let audio_text = gram_resolved.to_display_string(deck.context.course.target_language);
        let audio = AudioRequest {
            request: TtsRequest {
                text: audio_text,
                language: deck.context.course.target_language,
                is_ssml: false,
                instructions: None,
                speed: 1.0,
            },
            provider: TtsProvider::Google,
        };

        FlashCard {
            content,
            audio: Some(audio),
        }
    }

    pub fn translation_challenge(
        &self,
        deck: &Deck,
        gram: SpurGram,
    ) -> Option<Challenge<Gram<String>>> {
        let language_pack: &Arc<LanguagePack> = &deck.context.language_pack;
        let ComprehensibleSentence {
            target_language,
            target_language_sentence_grams,
            unique_target_language_phrases,
            native_languages,
        } = {
            let comprehensible_grams = deck.get_comprehensible_written_grams(false);
            deck.get_comprehensible_sentence_containing(
                Some(&gram),
                comprehensible_grams,
                &deck.stats.sentences_reviewed,
                language_pack,
            )?
        };

        // Convert sentence grams to literals, preserving gram group mapping
        let sentence_grams_with_literals = target_language_sentence_grams.to_literals(
            &language_pack.string_rodeo,
            &language_pack.gram_rodeo,
            deck.context.course.target_language,
        );

        let mut target_language_literals = Vec::new();
        let mut literal_gram_indices = Vec::new();
        let mut gram_definitions_for_lookup = Vec::new();

        for sentence_gram in sentence_grams_with_literals {
            let (gram_spur, literals) = match sentence_gram {
                SentenceGram::Learnable((g, l)) | SentenceGram::Obvious((g, l)) => (g, l),
            };

            let group_index = gram_definitions_for_lookup.len();
            let definition = language_pack.gram_definitions.get(&gram_spur).cloned();
            gram_definitions_for_lookup.push(definition);

            for literal in literals {
                literal_gram_indices.push(group_index);
                target_language_literals.push(literal);
            }
        }

        // Get movie titles from sentence_sources and movie metadata
        let movie_titles = language_pack
            .sentence_sources
            .get(&target_language)
            .map(|source| {
                source
                    .movie_ids
                    .iter()
                    .filter_map(|movie_id| {
                        language_pack
                            .movies
                            .get(movie_id)
                            .map(|metadata| (movie_id.clone(), metadata.title.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Get proper noun definitions by looking at the literals and checking the global definitions map
        let proper_noun_definitions: Vec<(String, language_utils::ProperNounDefinition)> =
            target_language_literals
                .iter()
                .filter_map(|literal| {
                    if let language_utils::WordType::Other(other) = &literal.word.word_type
                        && other.other_tag == language_utils::OtherWordType::Propn
                    {
                        let text_spur = language_pack.string_rodeo.get(&literal.word.text)?;
                        return language_pack
                            .proper_noun_definitions
                            .get(&text_spur)
                            .map(|def| (literal.word.text.clone(), def.clone()));
                    }
                    None
                })
                .collect();

        let second_chance = deck
            .stats
            .wrong_sentences
            .iter()
            .any(|(s, t)| *s == target_language && *t == SentenceChallengeType::Translation);

        Some(Challenge::TranslateComprehensibleSentence(
            TranslateComprehensibleSentence {
                target_language: language_pack
                    .string_rodeo
                    .resolve(&target_language)
                    .to_string(),
                target_language_literals,
                literal_gram_indices,
                gram_definitions_for_lookup,
                unique_target_language_phrases: unique_target_language_phrases
                    .iter()
                    .map(|p| {
                        language_pack
                            .gram_rodeo
                            .resolve(p)
                            .resolve(&language_pack.string_rodeo)
                    })
                    .collect(),
                phrase_definitions: unique_target_language_phrases
                    .iter()
                    .map(|p| language_pack.gram_definitions.get(p).cloned())
                    .collect(),
                native_translations: native_languages
                    .iter()
                    .map(|n| language_pack.string_rodeo.resolve(n).to_string())
                    .collect(),
                audio: AudioRequest {
                    request: TtsRequest {
                        text: language_pack
                            .string_rodeo
                            .resolve(&target_language)
                            .to_string(),
                        language: deck.context.course.target_language,
                        is_ssml: false,
                        instructions: None,
                        speed: 1.0,
                    },
                    provider: TtsProvider::ElevenLabs,
                },
                movie_titles,
                proper_noun_definitions,
                primary_expression: language_pack
                    .gram_rodeo
                    .resolve(&gram)
                    .resolve(&language_pack.string_rodeo),
                second_chance,
            },
        ))
    }

    pub fn written_challenge(
        &self,
        deck: &Deck,
        ctx: &CardContext,
        gram: SpurGram,
    ) -> Challenge<Gram<String>> {
        // If not new, try to create a translation challenge
        if !ctx.is_new
            && let Some(challenge) = self.translation_challenge(deck, gram)
        {
            return challenge;
        }

        // Fall back to flashcard
        let flashcard = self.written_gram_flashcard(deck, gram);
        ctx.wrap_flashcard(deck, flashcard)
    }

    pub fn pronunciation_challenge(
        &self,
        deck: &Deck,
        ctx: &CardContext,
        pattern: Spur,
        position: PatternPosition,
    ) -> Challenge<Gram<String>> {
        let language_pack = &deck.context.language_pack;
        let pattern_str = language_pack.string_rodeo.resolve(&pattern).to_string();
        let Some(guide) = language_pack
            .pronunciation_data
            .guides
            .iter()
            .find(|g| g.pattern == pattern_str && g.position == position)
            .cloned()
        else {
            panic!(
                "Pattern {pattern_str} with position {position:?} was in the deck, but was not found in pronunciation guides"
            );
        };

        let target_language = deck.context.course.target_language;
        let connector = target_language.pronunciation_connector();
        let audio_requests = guide
            .example_words
            .iter()
            .map(|example| AudioRequest {
                request: TtsRequest {
                    text: format!(
                        "<speak><break time=\"100ms\"/><say-as interpret-as=\"characters\">{}</say-as><break time=\"100ms\"/>{}<break time=\"200ms\"/>{}</speak>",
                        pattern_str, connector, example.target
                    ),
                    language: target_language,
                    is_ssml: true,
                    instructions: None,
                    speed: 1.0,
                },
                provider: TtsProvider::Google,
            })
            .collect();

        Challenge::PronunciationChallenge {
            indicator: ctx
                .indicator
                .resolve(&language_pack.string_rodeo, &language_pack.gram_rodeo),
            pattern: pattern_str,
            guide,
            audio_requests,
            is_new: ctx.is_new,
            times_type_seen: ctx.times_type_seen,
        }
    }
}
