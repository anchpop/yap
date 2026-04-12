use futures::StreamExt as _;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::features::Morphology;
use language_utils::{DictionaryEntry, GramFrequencyEntry, Heteronym, Language, PartOfSpeech};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

static CHAT_CLIENT_4O: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-4o")
        .unwrap()
        .with_cache_directory("./.cache")
});

static CHAT_CLIENT_5: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_service_tier("flex")
});

pub async fn create_morphology(
    language: Language,
    gram_frequencies: &[GramFrequencyEntry<String>],
) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
    // Process sentences to get unique words and track occurrences
    let mut target_language_heteronyms = BTreeMap::new();
    for entry in gram_frequencies {
        if let Some(heteronym) = entry.gram.heteronym() {
            target_language_heteronyms
                .entry(heteronym.clone())
                .or_insert(entry.count);
        }
    }

    // Try Wiktionary first for supported languages
    let mut morphology =
        wiktionary_morphology::create_morphology_from_wiktionary(language, gram_frequencies)
            .await
            .unwrap_or_default();

    // Filter out heteronyms that already have morphology from Wiktionary
    let mut remaining_heteronyms = BTreeMap::new();
    for (heteronym, count) in target_language_heteronyms {
        if !morphology.contains_key(&heteronym) {
            remaining_heteronyms.insert(heteronym, count);
        }
    }

    let count = remaining_heteronyms.len();

    if count == 0 {
        return Ok(morphology);
    }

    let pb = ProgressBar::new(count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} morphology entries ({per_sec}, ${msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let llm_morphology = futures::stream::iter(remaining_heteronyms.iter())
        .map(async |(heteronym, &freq)| {
            let cost = CHAT_CLIENT_5.cost().unwrap_or(0.0) + CHAT_CLIENT_4O.cost().unwrap_or(0.0);
            pb.set_message(format!(
                "{cost:.2} ({},{},{})",
                heteronym.word, heteronym.lemma, heteronym.pos
            ));

            let chat_client = if freq > 500 {
                &*CHAT_CLIENT_5
            } else {
                &*CHAT_CLIENT_4O
            };
            let morphology_response =
                llm_morphology::get_morphology(language, heteronym.clone(), chat_client).await;

            pb.inc(1);

            (heteronym, morphology_response)
        })
        .buffer_unordered(50)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(
            |(heteronym, morphology_result)| match morphology_result.ok() {
                Some(morph) => Some((heteronym.clone(), vec![morph])),
                None => None,
            },
        )
        .collect::<BTreeMap<Heteronym<String>, _>>();

    pb.finish_with_message(format!(
        "{:.2}",
        CHAT_CLIENT_5.cost().unwrap_or(0.0) + CHAT_CLIENT_4O.cost().unwrap_or(0.0)
    ));

    // Merge Wiktionary and LLM morphology
    morphology.extend(llm_morphology);

    Ok(morphology)
}

mod llm_morphology {

    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct GenderResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. gender")]
        gender: Option<language_utils::features::Gender>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct PoliteResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. politeness")]
        politeness: Option<language_utils::features::Polite>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct TenseResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. tense")]
        tense: Option<language_utils::features::Tense>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct PersonResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. person")]
        person: Option<language_utils::features::Person>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct CaseResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. case")]
        case: Option<language_utils::features::Case>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct NumberResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. number")]
        number: Option<language_utils::features::Number>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct MoodResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. mood")]
        mood: Option<language_utils::features::Mood>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct AspectResponse {
        #[serde(rename = "1. thoughts")]
        thoughts: String,
        #[serde(rename = "2. aspect")]
        aspect: Option<language_utils::features::Aspect>,
    }

    pub async fn get_morphology(
        language: Language,
        heteronym: Heteronym<String>,
        chat_client: &ChatClient,
    ) -> anyhow::Result<Morphology> {
        use language_utils::features::{
            Aspect, Case, FeatureSet, Gender, Mood, Number, Person, Polite, Tense,
        };

        let pos = heteronym.pos;

        // Determine which features apply to this word
        let gender_applies = Gender::applies_to(language, pos);
        let number_applies = Number::applies_to(language, pos);
        let politeness_applies = Polite::applies_to(language, pos);
        let tense_applies = Tense::applies_to(language, pos);
        let person_applies = Person::applies_to(language, pos);
        let case_applies = Case::applies_to(language, pos);
        let mood_applies = Mood::applies_to(language, pos);
        let aspect_applies = Aspect::applies_to(language, pos);

        // Issue concurrent requests for all applicable features
        let gender_future = async {
            if gender_applies {
                let result: Result<GenderResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the grammatical gender of the provided {language} word
Think about whether this word has a fixed grammatical gender. 
If it does, provide it. If the gender varies or is not applicable, return null.
Options are:
- Masculine
- Feminine
- Neuter (only applicable in languages that do have a neuter gender.)

Additionally, some languages do not distinguish masculine/feminine most of the time but they do distinguish neuter vs. non-neuter (Swedish neutrum / utrum). The non-neuter is called common gender. This is only applicable in languages that do not distinguish masculine/feminine.
- Common

If the gender of the word is not uniquely determined, return null. Neuter is only applicable in languages that have a neuter gender. Like Common, it is not a placeholder for when the gender is not known. If the grammatical gender is ambiguous or not specified, use `"2. gender": null`. (Respond with JSON, using "1. thoughts" then "2. gender".)"# ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.gender)
            } else {
                None
            }
        };

        let politeness_future = async {
            if politeness_applies {
                let result: Result<PoliteResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the morphological politeness of the provided {language} word.
Think about whether this word is morphologically formal, informal, elevated, or humble.
If it has a specific morphological politeness level, provide it. Otherwise, use `"2. politeness": null`. (Respond with JSON, using "1. thoughts" then "2. politeness".){}"#,
                if language.tv_politeness() {"\nPoliteness should only be non-null in the second person as this is a language with T-V distinction. Literary/archaic forms are not related to politeness."} else {""},
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.politeness)
            } else {
                None
            }
        };

        let tense_future = async {
            if tense_applies {
                let result: Result<TenseResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the tense of the provided {language} word.
Think about whether this word has a fixed tense. Options are:
- Past
- Present
- Future
- Imperfect
- Pluperfect

If one of these options is applicable, provide it. If the tense varies or is not applicable, use `"2. tense": null`. (Respond with JSON, using "1. thoughts" then "2. tense".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.tense)
            } else {
                None
            }
        };

        let person_future = async {
            if person_applies {
                let result: Result<PersonResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the grammatical person of the provided {language} word.
Think about whether this word has a fixed person (e.g., first person pronoun, third person verb).
If it does, provide it. If the person varies or is not applicable, return null.

Options are:
- First
- Second
- Third
Additionally, some language have more than three persons. So Zeroth and Fourth are also allowed. Most languages only have the three standard persons.

If one of these options is applicable, provide it. If the person varies or is not applicable, use `"2. person": null`. (Respond with JSON, using "1. thoughts" then "2. person".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.person)
            } else {
                None
            }
        };

        let case_future = async {
            if case_applies {
                let result: Result<CaseResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the grammatical case of the provided {language} word.
Think about whether this word has a fixed case marking. Case helps specify the role of a noun phrase in the sentence.

Common cases include:
- Nominative: subject form (base form)
- Accusative: direct object form
- Dative: indirect object form
- Genitive: possessive form ("of" or "'s")
- Vocative: form used for direct address
- Instrumental: means or instrument ("with/by means of")
- Locative: location in space or time ("in/at/on")
- Ablative: movement from/away ("from")

Other cases (mainly in specific language families):
- Absolutive, Ergative (Basque and others)
- Partitive (Finnish: indefinite/unfinished actions)
- Comitative (together with), Abessive (without)
- Causative (cause/purpose), Benefactive (for)
- Essive (temporary state), Translative (change of state)
- Various locational cases (Adessive, Allative, Elative, Illative, Inessive, etc.)
- And more specialized cases as needed

If this word has a fixed grammatical case, provide it. If case is not applicable or varies, use `"2. case": null`. (Respond with JSON, using "1. thoughts" then "2. case".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.case)
            } else {
                None
            }
        };

        let number_future = async {
            if number_applies {
                let result: Result<NumberResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the grammatical number of the provided {language} word.
Think about whether this word has a fixed number marking.

Common number values:
- Singular: one person, animal or thing
- Plural: several persons, animals or things

(For verbs, it should reflect whether the verb is clearly conjugated for a particular number. For example, some verbs are only used for the plural "they", and some are only conjugated for the singular "he". For nouns, it should reflect whether the noun is clearly plural or singular.)

Less common number values (use only if applicable):
- Dual: exactly two items
- Trial: exactly three items
- Paucal: a few items
- GreaterPaucal: more than several but not many
- GreaterPlural: many/all possible items
- Inverse: non-default for that particular noun
- Count: special plural form used after numerals
- PluraleTantum: only appears in plural form but denotes one thing (like "scissors", "pants")
- Collective: grammatical singular describing sets of objects (like "mankind", "furniture")

If this word has a fixed grammatical number, provide it. If number is not applicable, is ambiguous, or varies, use `"2. number": null`. (Respond with JSON, using "1. thoughts" then "2. number".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.number)
            } else {
                None
            }
        };

        let mood_future = async {
            if mood_applies {
                let result: Result<MoodResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the mood of the provided {language} verb.
Think about whether this verb has a fixed mood. Mood expresses modality and subclassifies finite verb forms.

Common moods:
- Indicative: default mood, states facts (something happens/happened/will happen)
- Imperative: commands or requests ("Go!", "Please come")
- Conditional: actions under certain conditions ("would go", "would have gone")
- Subjunctive: uncertain/subjective actions in subordinate clauses

Less common moods (use only if applicable):
- Potential: possible but not certain action (can, might, be able to)
- Jussive: desire that action happens (used in Arabic, Sanskrit)
- Purposive: "in order to" (Amazonian/Australian languages)
- Quotative: expressing direct speech of another person
- Optative: exclamations/wishes ("May you...", "If only...")
- Desiderative: want/wish to do something
- Necessitative: must/should/have to
- Interrogative: special form for yes-no questions (Turkic languages)
- Irrealis: action not known to have happened (roof term for conditional/potential/desiderative)
- Admirative: surprise/irony/doubt (Albanian, Balkan languages)

If this verb has a fixed mood, provide it. If mood is not applicable or varies, use `"2. mood": null`. (Respond with JSON, using "1. thoughts" then "2. mood".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.mood)
            } else {
                None
            }
        };

        let aspect_future = async {
            if aspect_applies {
                let result: Result<AspectResponse, _> = chat_client.chat_with_system_prompt(
                format!(
                    r#"Determine the grammatical aspect of the provided {language} word.
Aspect specifies the internal temporal structure of the action (duration, completion, habituality, etc.).

Common values:
- Imperfect: action took/takes/will take some time span with no information about completion
- Perfect: action has been / will have been completed
- Progressive: action is ongoing at the reference point (e.g. English "is eating", Hindi रहा/रही/रहे forms)
- Habitual: action takes place habitually or is a usual occurrence (e.g. Hindi -ता/-ती/-ते participles)
- Prospective: relative future — action expected to take place after the reference point
- Iterative: repeated action

If this word has a fixed grammatical aspect, provide it. If aspect is not applicable or varies, use `"2. aspect": null`. (Respond with JSON, using "1. thoughts" then "2. aspect".)"#,
                ),
                format!("{language} word: {} (lemma: {}) (POS: {pos:?})", heteronym.word, heteronym.lemma)
            ).await;
                result.ok().and_then(|r| r.aspect)
            } else {
                None
            }
        };

        // Execute all futures concurrently
        let (gender, number, politeness, tense, person, case, mood, aspect) = futures::join!(
            gender_future,
            number_future,
            politeness_future,
            tense_future,
            person_future,
            case_future,
            mood_future,
            aspect_future,
        );

        Ok(Morphology {
            gender,
            number,
            politeness,
            tense,
            person,
            case,
            mood,
            aspect,
        })
    }
}

/// Groups dictionary entries by their lemma and part of speech
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LemmaGroup {
    pub lemma: String,
    pub pos: PartOfSpeech,
    pub forms: Vec<WordForm>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WordForm {
    pub word: String,
    pub morphology: Vec<Morphology>,
}

/// Analyzes morphological coverage by grouping words by lemma and POS
pub fn analyze_morphology(
    dictionary: &BTreeMap<Heteronym<String>, DictionaryEntry>,
) -> Vec<LemmaGroup> {
    // Group dictionary entries by (lemma, pos)
    let mut lemma_map: BTreeMap<(String, PartOfSpeech), Vec<WordForm>> = BTreeMap::new();

    for (heteronym, entry) in dictionary {
        let key = (heteronym.lemma.clone(), heteronym.pos);
        lemma_map.entry(key).or_default().push(WordForm {
            word: heteronym.word.clone(),
            morphology: entry.morphology.clone(),
        });
    }

    // Convert to LemmaGroup structure
    let mut groups: Vec<LemmaGroup> = lemma_map
        .into_iter()
        .map(|((lemma, pos), forms)| LemmaGroup { lemma, pos, forms })
        .collect();

    // Sort by number of forms (descending) for easier analysis
    groups.sort_by(|a, b| b.forms.len().cmp(&a.forms.len()));

    groups
}

/// Writes conjugation/declension groups to a JSONL file
pub fn write_conjugations_jsonl(
    groups: &[LemmaGroup],
    output_path: &std::path::Path,
) -> std::io::Result<()> {
    let mut file = File::create(output_path)?;

    for group in groups {
        let json = serde_json::to_string(group).map_err(std::io::Error::other)?;
        writeln!(file, "{json}")?;
    }

    Ok(())
}

pub mod wiktionary_morphology {
    use super::*;

    pub async fn create_morphology_from_wiktionary(
        language: Language,
        gram_frequencies: &[GramFrequencyEntry<String>],
    ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
        match language {
            Language::French => french::create_french_morphology(gram_frequencies).await,
            Language::Spanish => spanish::create_spanish_morphology(gram_frequencies).await,
            Language::German => german::create_german_morphology(gram_frequencies).await,
            Language::Portuguese => {
                portuguese::create_portuguese_morphology(gram_frequencies).await
            }
            Language::Italian => italian::create_italian_morphology(gram_frequencies).await,
            Language::English => english::create_english_morphology(gram_frequencies).await,
            Language::Russian => russian::create_russian_morphology(gram_frequencies).await,
            Language::Hindi => hindi::create_hindi_morphology(gram_frequencies).await,
            _ => {
                // Return empty for unsupported languages
                Ok(BTreeMap::new())
            }
        }
    }

    /// Convert a NounGender (from Wiktionary) to morphology entries.
    /// For single-gender nouns, sets the gender. For dual-gender nouns, sets gender to None
    /// (the full gender info is preserved in the NounGender struct for downstream use).
    fn noun_gender_to_morphology(
        lemma: &str,
        noun_gender: &crate::wiktionary_conjugations::NounGender,
    ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
        use language_utils::features::Gender;

        let mut morphology = BTreeMap::new();

        // If there's exactly one gender, use it; otherwise None (ambiguous)
        let gender = if noun_gender.gender.genders.len() == 1 {
            match noun_gender.gender.genders[0] {
                Gender::Masculine => Some(Gender::Masculine),
                Gender::Feminine => Some(Gender::Feminine),
                Gender::Neuter => Some(Gender::Neuter),
                Gender::Common => Some(Gender::Common),
            }
        } else {
            None
        };

        let heteronym = Heteronym {
            word: lemma.to_string(),
            lemma: lemma.to_string(),
            pos: PartOfSpeech::Noun,
        };
        morphology
            .entry(heteronym)
            .or_insert_with(Vec::new)
            .push(Morphology {
                gender,
                number: None,
                politeness: None,
                tense: None,
                person: None,
                case: None,
                mood: None,
                aspect: None,
            });

        morphology
    }

    pub mod french {
        use super::*;
        use crate::wiktionary_conjugations::french::FrenchVerbConjugation;
        use language_utils::features::{Gender, Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_french_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb | PartOfSpeech::Aux => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();

            // Step 2: Fetch and parse Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/french");
            let conjugations =
                crate::wiktionary_conjugations::french::fetch_french_verb_conjugations(
                    &verb_lemmas_vec,
                    cache_dir,
                )
                .await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                // Create morphology for both VERB and AUX POS (some verbs like être/avoir are used as both)
                let verb_morphology =
                    conjugation_to_morphology(infinitive, conjugation, PartOfSpeech::Verb);
                morphology.extend(verb_morphology);

                let aux_morphology =
                    conjugation_to_morphology(infinitive, conjugation, PartOfSpeech::Aux);
                morphology.extend(aux_morphology);
            }

            // Step 4: Fetch noun genders
            let noun_genders = crate::wiktionary_conjugations::french::fetch_french_noun_genders(
                &noun_lemmas_vec,
                cache_dir,
            )
            .await?;

            // Step 5: Convert noun genders to morphology entries
            for (lemma, noun_gender) in noun_genders.iter() {
                let noun_morphology = super::noun_gender_to_morphology(lemma, noun_gender);
                morphology.extend(noun_morphology);
            }

            Ok(morphology)
        }

        pub fn conjugation_to_morphology(
            infinitive: &str,
            conjugation: &FrenchVerbConjugation,
            pos: PartOfSpeech,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Present participle (gerund)
            add_morph(
                &conjugation.present_participle,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Past participle - French past participles inflect for gender and number
            // Base form (masculine singular)
            let pp_base = &conjugation.past_participle;

            add_morph(
                pp_base,
                Morphology {
                    gender: Some(Gender::Masculine),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Feminine singular: only add 'e' if doesn't already end in 'e'
            let pp_fem_sg = if pp_base.ends_with('e') {
                pp_base.to_string()
            } else {
                format!("{pp_base}e")
            };
            add_morph(
                &pp_fem_sg,
                Morphology {
                    gender: Some(Gender::Feminine),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Masculine plural: only add 's' if doesn't already end in s/x
            let pp_masc_pl = if pp_base.ends_with('s') || pp_base.ends_with('x') {
                pp_base.to_string()
            } else {
                format!("{pp_base}s")
            };
            add_morph(
                &pp_masc_pl,
                Morphology {
                    gender: Some(Gender::Masculine),
                    number: Some(Number::Plural),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Feminine plural: feminine singular + 's' (handles all edge cases)
            let pp_fem_pl = format!("{pp_fem_sg}s");
            add_morph(
                &pp_fem_pl,
                Morphology {
                    gender: Some(Gender::Feminine),
                    number: Some(Number::Plural),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Indicative present (6 forms)
            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            for (i, form) in conjugation.indicative_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Indicative imperfect
            for (i, form) in conjugation.indicative_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Indicative past historic
            for (i, form) in conjugation.indicative_past_historic.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Indicative future
            for (i, form) in conjugation.indicative_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Indicative conditional
            for (i, form) in conjugation.indicative_conditional.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Conditional),
                        aspect: None,
                    },
                );
            }

            // Subjunctive present
            for (i, form) in conjugation.subjunctive_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Subjunctive imperfect
            for (i, form) in conjugation.subjunctive_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Imperative (3 forms: tu, nous, vous)
            // Some defective verbs (like pouvoir) don't have imperative forms
            if let Some(imperative) = &conjugation.imperative {
                let imperative_persons = [Person::Second, Person::First, Person::Second];
                let imperative_numbers = [Number::Singular, Number::Plural, Number::Plural];

                for (i, form) in imperative.iter().enumerate() {
                    add_morph(
                        form,
                        Morphology {
                            gender: None,
                            number: Some(imperative_numbers[i]),
                            politeness: None,
                            tense: None,
                            person: Some(imperative_persons[i]),
                            case: None,
                            mood: Some(Mood::Imperative),
                            aspect: None,
                        },
                    );
                }
            }

            morphology
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::wiktionary_conjugations::french::parse_french_verb_conjugation;

            #[test]
            fn test_boire_has_second_person_singular() {
                let html = std::fs::read_to_string("src/wiktionary-examples/fra/boire.txt")
                    .expect("Failed to read boire.txt");
                let conjugation = parse_french_verb_conjugation(&html, "boire")
                    .expect("Failed to parse boire conjugation");

                let morphology =
                    conjugation_to_morphology("boire", &conjugation, PartOfSpeech::Verb);

                // "bois" should have morphology entries for BOTH first and second person singular
                let bois_key = Heteronym {
                    word: "bois".to_string(),
                    lemma: "boire".to_string(),
                    pos: PartOfSpeech::Verb,
                };
                let bois_morphs = morphology
                    .get(&bois_key)
                    .expect("Expected morphology entries for 'bois'");

                let has_first_sg = bois_morphs.iter().any(|m| {
                    m.person == Some(Person::First)
                        && m.number == Some(Number::Singular)
                        && m.mood == Some(Mood::Indicative)
                        && m.tense == Some(Tense::Present)
                });
                let has_second_sg = bois_morphs.iter().any(|m| {
                    m.person == Some(Person::Second)
                        && m.number == Some(Number::Singular)
                        && m.mood == Some(Mood::Indicative)
                        && m.tense == Some(Tense::Present)
                });

                assert!(
                    has_first_sg,
                    "Expected first person singular present indicative for 'bois'"
                );
                assert!(
                    has_second_sg,
                    "Expected second person singular present indicative for 'bois'"
                );
            }
        }
    }

    mod spanish {
        use super::*;
        use crate::wiktionary_conjugations::spanish::{
            SpanishVerbConjugation, fetch_spanish_verb_conjugations,
        };
        use language_utils::features::{Gender, Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_spanish_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();

            // Step 2: Fetch Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/spanish");

            let conjugations = fetch_spanish_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology = conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
            }

            // Step 4: Fetch noun genders
            let noun_genders = crate::wiktionary_conjugations::spanish::fetch_spanish_noun_genders(
                &noun_lemmas_vec,
                cache_dir,
            )
            .await?;

            // Step 5: Convert noun genders to morphology entries
            for (lemma, noun_gender) in noun_genders.iter() {
                let noun_morphology = super::noun_gender_to_morphology(lemma, noun_gender);
                morphology.extend(noun_morphology);
            }

            Ok(morphology)
        }

        fn conjugation_to_morphology(
            infinitive: &str,
            conjugation: &SpanishVerbConjugation,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos: PartOfSpeech::Verb,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Gerund
            add_morph(
                &conjugation.gerund,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Past participles (masculine/feminine singular)
            add_morph(
                &conjugation.past_participle_masculine_singular,
                Morphology {
                    gender: Some(Gender::Masculine),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            add_morph(
                &conjugation.past_participle_feminine_singular,
                Morphology {
                    gender: Some(Gender::Feminine),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Indicative forms (6 forms: yo, tú, él, nosotros, vosotros, ellos)
            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            for (i, form) in conjugation.indicative_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_preterite.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_conditional.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Conditional),
                        aspect: None,
                    },
                );
            }

            // Subjunctive forms
            for (i, form) in conjugation.subjunctive_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.subjunctive_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.subjunctive_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Imperative (5 forms: tú, usted, nosotros, vosotros, ustedes)
            let imperative_persons = [
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let imperative_numbers = [
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            for (i, form) in conjugation.imperative.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(imperative_numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(imperative_persons[i]),
                        case: None,
                        mood: Some(Mood::Imperative),
                        aspect: None,
                    },
                );
            }

            morphology
        }
    }

    mod portuguese {
        use super::*;
        use crate::wiktionary_conjugations::portuguese::{
            PortugueseVerbConjugation, fetch_portuguese_verb_conjugations,
        };
        use language_utils::features::{Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_portuguese_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();

            // Step 2: Fetch Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/portuguese");

            let conjugations =
                fetch_portuguese_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology = conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
            }

            // Step 4: Fetch noun genders
            let noun_genders =
                crate::wiktionary_conjugations::portuguese::fetch_portuguese_noun_genders(
                    &noun_lemmas_vec,
                    cache_dir,
                )
                .await?;

            // Step 5: Convert noun genders to morphology entries
            for (lemma, noun_gender) in noun_genders.iter() {
                let noun_morphology = super::noun_gender_to_morphology(lemma, noun_gender);
                morphology.extend(noun_morphology);
            }

            Ok(morphology)
        }

        fn conjugation_to_morphology(
            infinitive: &str,
            conjugation: &PortugueseVerbConjugation,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos: PartOfSpeech::Verb,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Gerund
            add_morph(
                &conjugation.gerund,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Past participle (Portuguese uses only masculine singular, no gender distinction in this form)
            add_morph(
                &conjugation.past_participle,
                Morphology {
                    gender: None,
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Verb forms (6 forms: eu, tu, ele/você, nós, vós, eles/vocês)
            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            // Indicative forms
            for (i, form) in conjugation.indicative_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_preterite.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_pluperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Pluperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_conditional.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Conditional),
                        aspect: None,
                    },
                );
            }

            // Subjunctive forms
            for (i, form) in conjugation.subjunctive_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.subjunctive_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.subjunctive_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Imperative affirmative (5 forms: tu, você, nós, vós, vocês)
            let imperative_persons = [
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let imperative_numbers = [
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            for (i, form) in conjugation.imperative_affirmative.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(imperative_numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(imperative_persons[i]),
                        case: None,
                        mood: Some(Mood::Imperative),
                        aspect: None,
                    },
                );
            }

            morphology
        }
    }

    mod italian {
        use super::*;
        use crate::wiktionary_conjugations::italian::{
            ItalianVerbConjugation, fetch_italian_verb_conjugations,
        };
        use language_utils::features::{Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_italian_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();

            // Step 2: Fetch Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/italian");

            let conjugations = fetch_italian_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology = conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
            }

            // Step 4: Fetch noun genders
            let noun_genders = crate::wiktionary_conjugations::italian::fetch_italian_noun_genders(
                &noun_lemmas_vec,
                cache_dir,
            )
            .await?;

            // Step 5: Convert noun genders to morphology entries
            for (lemma, noun_gender) in noun_genders.iter() {
                let noun_morphology = super::noun_gender_to_morphology(lemma, noun_gender);
                morphology.extend(noun_morphology);
            }

            Ok(morphology)
        }

        fn conjugation_to_morphology(
            infinitive: &str,
            conjugation: &ItalianVerbConjugation,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos: PartOfSpeech::Verb,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Gerund
            add_morph(
                &conjugation.gerund,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Past participle
            add_morph(
                &conjugation.past_participle,
                Morphology {
                    gender: None,
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Verb forms (6 forms: io, tu, lui/lei, noi, voi, loro)
            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            // Indicative forms
            for (i, form) in conjugation.indicative_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_past_historic.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_future.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Future),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.indicative_conditional.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Conditional),
                        aspect: None,
                    },
                );
            }

            // Subjunctive forms
            for (i, form) in conjugation.subjunctive_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            for (i, form) in conjugation.subjunctive_imperfect.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Imperfect),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Imperative (5 forms: tu, Lei, noi, voi, Loro)
            let imperative_persons = [
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let imperative_numbers = [
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            for (i, form) in conjugation.imperative.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(imperative_numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(imperative_persons[i]),
                        case: None,
                        mood: Some(Mood::Imperative),
                        aspect: None,
                    },
                );
            }

            morphology
        }
    }

    mod german {
        use super::*;
        use crate::wiktionary_conjugations::german::{
            GermanGender, GermanNounDeclension, GermanVerbConjugation,
            fetch_german_noun_declensions, fetch_german_verb_conjugations,
        };
        use language_utils::features::{Case, Gender, Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_german_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            let mut morphology = BTreeMap::new();

            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();

            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();

            // Step 2: Fetch verb conjugations
            let cache_dir = Path::new(".cache/wiktionary/german");
            let verb_conjugations =
                fetch_german_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert verb conjugations to morphology entries
            for (infinitive, conjugation) in verb_conjugations.iter() {
                let verb_morphology = verb_conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
            }

            // Step 4: Fetch noun declensions
            let noun_declensions =
                fetch_german_noun_declensions(&noun_lemmas_vec, cache_dir).await?;

            // Step 5: Convert noun declensions to morphology entries
            for (lemma, declension) in noun_declensions.iter() {
                let noun_morphology = noun_declension_to_morphology(lemma, declension);
                morphology.extend(noun_morphology);
            }

            Ok(morphology)
        }

        fn verb_conjugation_to_morphology(
            infinitive: &str,
            conjugation: &GermanVerbConjugation,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos: PartOfSpeech::Verb,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Present participle
            add_morph(
                &conjugation.present_participle,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Past participle
            add_morph(
                &conjugation.past_participle,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // German conjugation forms (6 forms: ich, du, er, wir, ihr, sie)
            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            // Indicative present
            for (i, form) in conjugation.indicative_present.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Indicative preterite (simple past)
            for (i, form) in conjugation.indicative_preterite.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }

            // Subjunctive I (Konjunktiv I)
            for (i, form) in conjugation.subjunctive_i.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Subjunctive II (Konjunktiv II)
            for (i, form) in conjugation.subjunctive_ii.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(numbers[i]),
                        politeness: None,
                        tense: Some(Tense::Past), // Konjunktiv II is formed from preterite stem
                        person: Some(persons[i]),
                        case: None,
                        mood: Some(Mood::Subjunctive),
                        aspect: None,
                    },
                );
            }

            // Imperative (2 forms: du, ihr)
            let imperative_persons = [Person::Second, Person::Second];
            let imperative_numbers = [Number::Singular, Number::Plural];

            for (i, form) in conjugation.imperative.iter().enumerate() {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: Some(imperative_numbers[i]),
                        politeness: None,
                        tense: None,
                        person: Some(imperative_persons[i]),
                        case: None,
                        mood: Some(Mood::Imperative),
                        aspect: None,
                    },
                );
            }

            morphology
        }

        fn noun_declension_to_morphology(
            lemma: &str,
            declension: &GermanNounDeclension,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            let gender = match declension.gender {
                GermanGender::Masculine => Gender::Masculine,
                GermanGender::Feminine => Gender::Feminine,
                GermanGender::Neuter => Gender::Neuter,
            };

            // Helper to add a morphology entry
            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: lemma.to_string(),
                    pos: PartOfSpeech::Noun,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Singular forms (optional - some pages lack a full declension table)
            if let Some(nom_sg) = &declension.nominative_singular {
                add_morph(
                    nom_sg,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Nominative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(gen_sg) = &declension.genitive_singular {
                add_morph(
                    gen_sg,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Genitive),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(dat_sg) = &declension.dative_singular {
                add_morph(
                    dat_sg,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Dative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(acc_sg) = &declension.accusative_singular {
                add_morph(
                    acc_sg,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Accusative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            // Plural forms (optional - some nouns are uncountable/sg-only or lack a declension table)
            if let Some(nom_pl) = &declension.nominative_plural {
                add_morph(
                    nom_pl,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Plural),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Nominative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(gen_pl) = &declension.genitive_plural {
                add_morph(
                    gen_pl,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Plural),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Genitive),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(dat_pl) = &declension.dative_plural {
                add_morph(
                    dat_pl,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Plural),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Dative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            if let Some(acc_pl) = &declension.accusative_plural {
                add_morph(
                    acc_pl,
                    Morphology {
                        gender: Some(gender),
                        number: Some(Number::Plural),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: Some(Case::Accusative),
                        mood: None,
                        aspect: None,
                    },
                );
            }

            morphology
        }
    }

    mod english {
        use super::*;
        use crate::wiktionary_conjugations::english::{
            EnglishVerbConjugation, fetch_english_verb_conjugations,
        };
        use language_utils::features::{Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_english_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            let mut morphology = BTreeMap::new();

            // Extract verb lemmas
            let mut verb_lemmas = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb | PartOfSpeech::Aux => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();

            // Fetch and parse Wiktionary pages
            let cache_dir = Path::new(".cache/wiktionary/english");
            let conjugations = fetch_english_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Convert conjugations to morphology entries
            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology =
                    conjugation_to_morphology(infinitive, conjugation, PartOfSpeech::Verb);
                morphology.extend(verb_morphology);

                let aux_morphology =
                    conjugation_to_morphology(infinitive, conjugation, PartOfSpeech::Aux);
                morphology.extend(aux_morphology);
            }

            Ok(morphology)
        }

        fn conjugation_to_morphology(
            infinitive: &str,
            conjugation: &EnglishVerbConjugation,
            pos: PartOfSpeech,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive / base form
            add_morph(
                infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Third-person singular present (e.g. "drinks")
            add_morph(
                &conjugation.third_person_singular,
                Morphology {
                    gender: None,
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: Some(Person::Third),
                    case: None,
                    mood: Some(Mood::Indicative),
                    aspect: None,
                },
            );

            // Present participle (e.g. "drinking")
            add_morph(
                &conjugation.present_participle,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Present),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            // Simple past (e.g. "drank")
            add_morph(
                &conjugation.simple_past,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: Some(Mood::Indicative),
                    aspect: None,
                },
            );

            // Past participle (e.g. "drunk")
            add_morph(
                &conjugation.past_participle,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            morphology
        }
    }

    mod russian {
        use super::*;
        use crate::wiktionary_conjugations::russian::{
            RussianAdjectiveDeclension, RussianGender, RussianNounDeclension,
            RussianVerbConjugation, fetch_russian_adjective_declensions,
            fetch_russian_noun_declensions, fetch_russian_verb_conjugations,
        };
        use language_utils::features::{Case, Gender, Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_russian_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            let mut morphology = BTreeMap::new();

            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();
            let mut adj_lemmas = HashSet::new();
            let mut det_lemmas = HashSet::new();

            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Adj => {
                            adj_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Det => {
                            det_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();
            let adj_lemmas_vec: Vec<String> = adj_lemmas.into_iter().collect();
            let det_lemmas_vec: Vec<String> = det_lemmas.into_iter().collect();

            let cache_dir = Path::new(".cache/wiktionary/russian");

            let verb_conjugations =
                fetch_russian_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            for (infinitive, conjugation) in verb_conjugations.iter() {
                let verb_morphology = verb_conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
            }

            let noun_declensions =
                fetch_russian_noun_declensions(&noun_lemmas_vec, cache_dir).await?;

            for (lemma, declension) in noun_declensions.iter() {
                let noun_morphology = noun_declension_to_morphology(lemma, declension);
                morphology.extend(noun_morphology);
            }

            let adj_declensions =
                fetch_russian_adjective_declensions(&adj_lemmas_vec, cache_dir).await?;

            for (lemma, declension) in adj_declensions.iter() {
                let adj_morphology =
                    adjective_like_declension_to_morphology(lemma, declension, PartOfSpeech::Adj);
                morphology.extend(adj_morphology);
            }

            // Determiners decline like adjectives in Russian
            let det_declensions =
                fetch_russian_adjective_declensions(&det_lemmas_vec, cache_dir).await?;

            for (lemma, declension) in det_declensions.iter() {
                let det_morphology =
                    adjective_like_declension_to_morphology(lemma, declension, PartOfSpeech::Det);
                morphology.extend(det_morphology);
            }

            Ok(morphology)
        }

        fn verb_conjugation_to_morphology(
            infinitive: &str,
            conjugation: &RussianVerbConjugation,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: infinitive.to_string(),
                    pos: PartOfSpeech::Verb,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Infinitive
            add_morph(
                &conjugation.infinitive,
                Morphology {
                    gender: None,
                    number: None,
                    politeness: None,
                    tense: None,
                    person: None,
                    case: None,
                    mood: None,
                    aspect: None,
                },
            );

            let persons = [
                Person::First,
                Person::Second,
                Person::Third,
                Person::First,
                Person::Second,
                Person::Third,
            ];
            let numbers = [
                Number::Singular,
                Number::Singular,
                Number::Singular,
                Number::Plural,
                Number::Plural,
                Number::Plural,
            ];

            // Present tense (imperfective only)
            if let Some(present) = &conjugation.present {
                for (i, form) in present.iter().enumerate() {
                    add_morph(
                        form,
                        Morphology {
                            gender: None,
                            number: Some(numbers[i]),
                            politeness: None,
                            tense: Some(Tense::Present),
                            person: Some(persons[i]),
                            case: None,
                            mood: Some(Mood::Indicative),
                            aspect: None,
                        },
                    );
                }
            }

            // Future tense (perfective synthetic future)
            if let Some(future) = &conjugation.future {
                for (i, form) in future.iter().enumerate() {
                    add_morph(
                        form,
                        Morphology {
                            gender: None,
                            number: Some(numbers[i]),
                            politeness: None,
                            tense: Some(Tense::Future),
                            person: Some(persons[i]),
                            case: None,
                            mood: Some(Mood::Indicative),
                            aspect: None,
                        },
                    );
                }
            }

            // Past tense (gendered singular, genderless plural)
            let past_genders = [
                (Some(Gender::Masculine), &conjugation.past_masculine),
                (Some(Gender::Feminine), &conjugation.past_feminine),
                (Some(Gender::Neuter), &conjugation.past_neuter),
            ];
            for (gender, form) in past_genders {
                add_morph(
                    form,
                    Morphology {
                        gender,
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: None,
                        case: None,
                        mood: Some(Mood::Indicative),
                        aspect: None,
                    },
                );
            }
            add_morph(
                &conjugation.past_plural,
                Morphology {
                    gender: None,
                    number: Some(Number::Plural),
                    politeness: None,
                    tense: Some(Tense::Past),
                    person: None,
                    case: None,
                    mood: Some(Mood::Indicative),
                    aspect: None,
                },
            );

            // Imperative
            if let Some(imperative) = &conjugation.imperative {
                let imp_numbers = [Number::Singular, Number::Plural];
                for (i, form) in imperative.iter().enumerate() {
                    add_morph(
                        form,
                        Morphology {
                            gender: None,
                            number: Some(imp_numbers[i]),
                            politeness: None,
                            tense: None,
                            person: Some(Person::Second),
                            case: None,
                            mood: Some(Mood::Imperative),
                            aspect: None,
                        },
                    );
                }
            }

            // Participles
            if let Some(form) = &conjugation.present_active_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(form) = &conjugation.past_active_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(form) = &conjugation.present_passive_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(form) = &conjugation.past_passive_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }

            // Adverbial participles (gerunds)
            if let Some(form) = &conjugation.present_adverbial_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Present),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(form) = &conjugation.past_adverbial_participle {
                add_morph(
                    form,
                    Morphology {
                        gender: None,
                        number: None,
                        politeness: None,
                        tense: Some(Tense::Past),
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }

            morphology
        }

        fn noun_declension_to_morphology(
            lemma: &str,
            declension: &RussianNounDeclension,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            let gender = match declension.gender {
                RussianGender::Masculine => Gender::Masculine,
                RussianGender::Feminine => Gender::Feminine,
                RussianGender::Neuter => Gender::Neuter,
            };

            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: lemma.to_string(),
                    pos: PartOfSpeech::Noun,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Helper for case/number forms
            let cases = [
                (
                    Case::Nominative,
                    &declension.nominative_singular,
                    &declension.nominative_plural,
                ),
                (
                    Case::Genitive,
                    &declension.genitive_singular,
                    &declension.genitive_plural,
                ),
                (
                    Case::Dative,
                    &declension.dative_singular,
                    &declension.dative_plural,
                ),
                (
                    Case::Accusative,
                    &declension.accusative_singular,
                    &declension.accusative_plural,
                ),
                (
                    Case::Instrumental,
                    &declension.instrumental_singular,
                    &declension.instrumental_plural,
                ),
                // Russian prepositional case maps to Locative in UD
                (
                    Case::Locative,
                    &declension.prepositional_singular,
                    &declension.prepositional_plural,
                ),
            ];

            for (case, sg, pl) in cases {
                if let Some(form) = sg {
                    add_morph(
                        form,
                        Morphology {
                            gender: Some(gender),
                            number: Some(Number::Singular),
                            politeness: None,
                            tense: None,
                            person: None,
                            case: Some(case),
                            mood: None,
                            aspect: None,
                        },
                    );
                }
                if let Some(form) = pl {
                    add_morph(
                        form,
                        Morphology {
                            gender: Some(gender),
                            number: Some(Number::Plural),
                            politeness: None,
                            tense: None,
                            person: None,
                            case: Some(case),
                            mood: None,
                            aspect: None,
                        },
                    );
                }
            }

            morphology
        }

        fn adjective_like_declension_to_morphology(
            lemma: &str,
            declension: &RussianAdjectiveDeclension,
            pos: PartOfSpeech,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut morphology = BTreeMap::new();

            let mut add_morph = |word: &str, morph: Morphology| {
                let heteronym = Heteronym {
                    word: word.to_string(),
                    lemma: lemma.to_string(),
                    pos,
                };
                morphology
                    .entry(heteronym)
                    .or_insert_with(Vec::new)
                    .push(morph);
            };

            // Helper to create a morphology with case, gender, number
            let mut add_case_form =
                |form: &Option<String>, case: Case, gender: Option<Gender>, number: Number| {
                    if let Some(word) = form {
                        add_morph(
                            word,
                            Morphology {
                                gender,
                                number: Some(number),
                                politeness: None,
                                tense: None,
                                person: None,
                                case: Some(case),
                                mood: None,
                                aspect: None,
                            },
                        );
                    }
                };

            // Nominative
            add_case_form(
                &declension.nominative_masculine,
                Case::Nominative,
                Some(Gender::Masculine),
                Number::Singular,
            );
            add_case_form(
                &declension.nominative_neuter,
                Case::Nominative,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.nominative_feminine,
                Case::Nominative,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.nominative_plural,
                Case::Nominative,
                None,
                Number::Plural,
            );

            // Genitive (masculine/neuter share form)
            add_case_form(
                &declension.genitive_masculine_neuter,
                Case::Genitive,
                Some(Gender::Masculine),
                Number::Singular,
            );
            // Also add as neuter
            add_case_form(
                &declension.genitive_masculine_neuter,
                Case::Genitive,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.genitive_feminine,
                Case::Genitive,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.genitive_plural,
                Case::Genitive,
                None,
                Number::Plural,
            );

            // Dative (masculine/neuter share form)
            add_case_form(
                &declension.dative_masculine_neuter,
                Case::Dative,
                Some(Gender::Masculine),
                Number::Singular,
            );
            add_case_form(
                &declension.dative_masculine_neuter,
                Case::Dative,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.dative_feminine,
                Case::Dative,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.dative_plural,
                Case::Dative,
                None,
                Number::Plural,
            );

            // Accusative — for animate masculine, use genitive-like form; for inanimate, use nominative-like
            // We store the animate form as the primary accusative masculine
            add_case_form(
                &declension.accusative_animate_masculine,
                Case::Accusative,
                Some(Gender::Masculine),
                Number::Singular,
            );
            // Also add the inanimate form if different
            if declension.accusative_inanimate_masculine != declension.accusative_animate_masculine
            {
                add_case_form(
                    &declension.accusative_inanimate_masculine,
                    Case::Accusative,
                    Some(Gender::Masculine),
                    Number::Singular,
                );
            }
            add_case_form(
                &declension.accusative_neuter,
                Case::Accusative,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.accusative_feminine,
                Case::Accusative,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.accusative_animate_plural,
                Case::Accusative,
                None,
                Number::Plural,
            );
            if declension.accusative_inanimate_plural != declension.accusative_animate_plural {
                add_case_form(
                    &declension.accusative_inanimate_plural,
                    Case::Accusative,
                    None,
                    Number::Plural,
                );
            }

            // Instrumental (masculine/neuter share form)
            add_case_form(
                &declension.instrumental_masculine_neuter,
                Case::Instrumental,
                Some(Gender::Masculine),
                Number::Singular,
            );
            add_case_form(
                &declension.instrumental_masculine_neuter,
                Case::Instrumental,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.instrumental_feminine,
                Case::Instrumental,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.instrumental_plural,
                Case::Instrumental,
                None,
                Number::Plural,
            );

            // Prepositional → Locative (same mapping as nouns)
            add_case_form(
                &declension.prepositional_masculine_neuter,
                Case::Locative,
                Some(Gender::Masculine),
                Number::Singular,
            );
            add_case_form(
                &declension.prepositional_masculine_neuter,
                Case::Locative,
                Some(Gender::Neuter),
                Number::Singular,
            );
            add_case_form(
                &declension.prepositional_feminine,
                Case::Locative,
                Some(Gender::Feminine),
                Number::Singular,
            );
            add_case_form(
                &declension.prepositional_plural,
                Case::Locative,
                None,
                Number::Plural,
            );

            // Short forms (no case — these are predicative)
            if let Some(word) = &declension.short_masculine {
                add_morph(
                    word,
                    Morphology {
                        gender: Some(Gender::Masculine),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(word) = &declension.short_neuter {
                add_morph(
                    word,
                    Morphology {
                        gender: Some(Gender::Neuter),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(word) = &declension.short_feminine {
                add_morph(
                    word,
                    Morphology {
                        gender: Some(Gender::Feminine),
                        number: Some(Number::Singular),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }
            if let Some(word) = &declension.short_plural {
                add_morph(
                    word,
                    Morphology {
                        gender: None,
                        number: Some(Number::Plural),
                        politeness: None,
                        tense: None,
                        person: None,
                        case: None,
                        mood: None,
                        aspect: None,
                    },
                );
            }

            morphology
        }
    }

    pub mod hindi {
        use super::*;
        use crate::wiktionary_conjugations::hindi::{HindiInflection, fetch_hindi_inflections};
        use language_utils::features::{Aspect, Case, Gender, Mood, Number, Person, Polite, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_hindi_morphology(
            gram_frequencies: &[GramFrequencyEntry<String>],
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract every Hindi lemma with an inflectable POS.
            // For Hindi, verbs, nouns, and adjectives all share the same
            // Wiktionary `form-of` markup, so we fetch them the same way.
            let mut verb_lemmas: HashSet<String> = HashSet::new();
            let mut noun_lemmas: HashSet<String> = HashSet::new();
            let mut adj_lemmas: HashSet<String> = HashSet::new();
            for entry in gram_frequencies {
                if let Some(heteronym) = entry.gram.heteronym() {
                    match heteronym.pos {
                        PartOfSpeech::Verb | PartOfSpeech::Aux => {
                            verb_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Noun => {
                            noun_lemmas.insert(heteronym.lemma.clone());
                        }
                        PartOfSpeech::Adj => {
                            adj_lemmas.insert(heteronym.lemma.clone());
                        }
                        _ => {}
                    }
                }
            }

            let cache_dir = Path::new(".cache/wiktionary/hindi");

            // Step 2: Fetch each POS bucket separately so we can record the POS with each entry
            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();
            let noun_lemmas_vec: Vec<String> = noun_lemmas.into_iter().collect();
            let adj_lemmas_vec: Vec<String> = adj_lemmas.into_iter().collect();

            let verb_inflections = fetch_hindi_inflections(&verb_lemmas_vec, cache_dir).await?;
            let noun_inflections = fetch_hindi_inflections(&noun_lemmas_vec, cache_dir).await?;
            let adj_inflections = fetch_hindi_inflections(&adj_lemmas_vec, cache_dir).await?;

            let mut morphology: BTreeMap<Heteronym<String>, Vec<Morphology>> = BTreeMap::new();

            for (lemma, inflection) in &verb_inflections {
                merge_in(
                    &mut morphology,
                    inflection_to_morphology(lemma, inflection, PartOfSpeech::Verb),
                );
                // Some Hindi verbs (होना above all) are also used as auxiliaries
                merge_in(
                    &mut morphology,
                    inflection_to_morphology(lemma, inflection, PartOfSpeech::Aux),
                );
            }
            for (lemma, inflection) in &noun_inflections {
                merge_in(
                    &mut morphology,
                    inflection_to_morphology(lemma, inflection, PartOfSpeech::Noun),
                );
            }
            for (lemma, inflection) in &adj_inflections {
                merge_in(
                    &mut morphology,
                    inflection_to_morphology(lemma, inflection, PartOfSpeech::Adj),
                );
            }

            Ok(morphology)
        }

        fn merge_in(
            into: &mut BTreeMap<Heteronym<String>, Vec<Morphology>>,
            from: BTreeMap<Heteronym<String>, Vec<Morphology>>,
        ) {
            for (k, v) in from {
                let entry = into.entry(k).or_default();
                for morph in v {
                    if !entry.contains(&morph) {
                        entry.push(morph);
                    }
                }
            }
        }

        /// Interpret a single Wiktionary tag set into one or more [`Morphology`] entries.
        ///
        /// Returns multiple entries when a single tag set is morphologically ambiguous —
        /// Hindi's future-tense paradigm has a famous "13" tag (both 1st-person-plural
        /// AND 3rd-person-plural share the same form), and we want both interpretations
        /// to be matchable downstream.
        pub fn tag_set_to_morphologies(tags: &[String]) -> Vec<Morphology> {
            let mut base = Morphology::default();
            // Track ambiguous person separately so we can expand it at the end.
            // `persons` is the set of person values this tag set can carry;
            // a single-item vec means unambiguous (or unspecified).
            let mut persons: Vec<Option<Person>> = vec![None];

            for t in tags {
                match t.as_str() {
                    "1" => base.person = Some(Person::First),
                    "2" => base.person = Some(Person::Second),
                    "3" => base.person = Some(Person::Third),
                    // "13" is Hindi Wiktionary's shorthand for "1st OR 3rd person plural":
                    // in the future tense, हम (we) and वे (they) take the same verb form.
                    "13" => persons = vec![Some(Person::First), Some(Person::Third)],
                    "s" => base.number = Some(Number::Singular),
                    "p" => base.number = Some(Number::Plural),
                    "m" => base.gender = Some(Gender::Masculine),
                    "f" => base.gender = Some(Gender::Feminine),
                    // Hindi's direct/oblique distinction is the classic two-case system
                    // that UD maps onto Nominative/Accusative.
                    "dir" => base.case = Some(Case::Nominative),
                    "obl" => base.case = Some(Case::Accusative),
                    "voc" => base.case = Some(Case::Vocative),
                    // तू (intimate), तुम (informal T-form), आप (formal V-form).
                    // We reserve `Intimate` for तू to distinguish it from European
                    // T-forms like French tu / German du, which map more naturally
                    // onto तुम (our `Informal`).
                    "intim" => base.politeness = Some(Polite::Intimate),
                    "fam" => base.politeness = Some(Polite::Informal),
                    "formal" => base.politeness = Some(Polite::Formal),
                    "pres" => base.tense = Some(Tense::Present),
                    "fut" => base.tense = Some(Tense::Future),
                    "ind" => base.mood = Some(Mood::Indicative),
                    "subj" => base.mood = Some(Mood::Subjunctive),
                    "imp" => base.mood = Some(Mood::Imperative),
                    // Counterfactual ("would have X'd") behaves as a subjunctive-like mood.
                    "cfact" => base.mood = Some(Mood::Subjunctive),
                    "hab" => base.aspect = Some(Aspect::Habitual),
                    "pfv" => base.aspect = Some(Aspect::Perfect),
                    "prospective" => base.aspect = Some(Aspect::Prospective),
                    // `perf|ind` is the finite perfective past: हुआ "became", किया "did"
                    "perf" => {
                        base.tense = Some(Tense::Past);
                        base.aspect = Some(Aspect::Perfect);
                        base.mood = Some(Mood::Indicative);
                    }
                    // `impf|ind` (imperfect indicative) only really appears for होना:
                    // था/थे/थी/थीं "was/were"
                    "impf" => {
                        base.tense = Some(Tense::Past);
                        base.aspect = Some(Aspect::Imperfect);
                        base.mood = Some(Mood::Indicative);
                    }
                    // Form-type markers with no morphological content of their own.
                    "part" | "stem" | "inf" | "conj" | "form" => {}
                    _ => {
                        // Silently ignore unknown tags; Wiktionary occasionally introduces new ones.
                    }
                }
            }

            if persons.len() > 1 {
                persons
                    .into_iter()
                    .map(|p| {
                        let mut m = base.clone();
                        m.person = p;
                        m
                    })
                    .collect()
            } else {
                vec![base]
            }
        }

        /// Convert a parsed [`HindiInflection`] into morphology entries for a given POS.
        /// Deduplicates identical (word, morphology) pairs so that a form which appears
        /// under multiple alternative tag sets does not get duplicated Morphology entries.
        pub fn inflection_to_morphology(
            lemma: &str,
            inflection: &HindiInflection,
            pos: PartOfSpeech,
        ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
            let mut out: BTreeMap<Heteronym<String>, Vec<Morphology>> = BTreeMap::new();
            for form in &inflection.forms {
                let heteronym = Heteronym {
                    word: form.word.clone(),
                    lemma: lemma.to_string(),
                    pos,
                };
                let entry = out.entry(heteronym).or_default();
                for tag_set in &form.tag_sets {
                    for morph in tag_set_to_morphologies(tag_set) {
                        if !entry.contains(&morph) {
                            entry.push(morph);
                        }
                    }
                }
            }
            out
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::wiktionary_conjugations::hindi::parse_hindi_inflection;

            fn load(word: &str) -> String {
                std::fs::read_to_string(format!("src/wiktionary-examples/hin/{word}.txt"))
                    .unwrap_or_else(|_| panic!("missing test fixture for {word}"))
            }

            fn build(
                word: &str,
                pos: PartOfSpeech,
            ) -> BTreeMap<Heteronym<String>, Vec<Morphology>> {
                let html = load(word);
                let inflection = parse_hindi_inflection(&html, word).unwrap();
                inflection_to_morphology(word, &inflection, pos)
            }

            fn has_morph(
                map: &BTreeMap<Heteronym<String>, Vec<Morphology>>,
                word: &str,
                lemma: &str,
                pos: PartOfSpeech,
                pred: impl Fn(&Morphology) -> bool,
            ) -> bool {
                let het = Heteronym {
                    word: word.to_string(),
                    lemma: lemma.to_string(),
                    pos,
                };
                map.get(&het).map(|ms| ms.iter().any(pred)).unwrap_or(false)
            }

            #[test]
            fn tag_13_expands_into_first_and_third_person() {
                let tags = vec![
                    "13".to_string(),
                    "p".to_string(),
                    "fut".to_string(),
                    "subj".to_string(),
                ];
                let morphs = tag_set_to_morphologies(&tags);
                assert_eq!(morphs.len(), 2);
                assert!(morphs.iter().any(|m| m.person == Some(Person::First)));
                assert!(morphs.iter().any(|m| m.person == Some(Person::Third)));
                for m in &morphs {
                    assert_eq!(m.number, Some(Number::Plural));
                    assert_eq!(m.tense, Some(Tense::Future));
                    assert_eq!(m.mood, Some(Mood::Subjunctive));
                }
            }

            #[test]
            fn perf_ind_maps_to_past_perfect_indicative() {
                let tags = vec![
                    "m".to_string(),
                    "s".to_string(),
                    "perf".to_string(),
                    "ind".to_string(),
                ];
                let morphs = tag_set_to_morphologies(&tags);
                assert_eq!(morphs.len(), 1);
                let m = &morphs[0];
                assert_eq!(m.tense, Some(Tense::Past));
                assert_eq!(m.aspect, Some(Aspect::Perfect));
                assert_eq!(m.mood, Some(Mood::Indicative));
                assert_eq!(m.gender, Some(Gender::Masculine));
                assert_eq!(m.number, Some(Number::Singular));
            }

            #[test]
            fn impf_ind_is_past_imperfect_indicative() {
                let tags = vec![
                    "f".to_string(),
                    "p".to_string(),
                    "impf".to_string(),
                    "ind".to_string(),
                ];
                let morphs = tag_set_to_morphologies(&tags);
                assert_eq!(morphs.len(), 1);
                let m = &morphs[0];
                assert_eq!(m.tense, Some(Tense::Past));
                assert_eq!(m.aspect, Some(Aspect::Imperfect));
                assert_eq!(m.mood, Some(Mood::Indicative));
                assert_eq!(m.gender, Some(Gender::Feminine));
                assert_eq!(m.number, Some(Number::Plural));
            }

            #[test]
            fn hona_yields_gendered_perfective_and_habitual() {
                let m = build("होना", PartOfSpeech::Verb);

                // Habitual masculine singular: होता
                assert!(has_morph(
                    &m,
                    "होता",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.aspect == Some(Aspect::Habitual)
                            && morph.gender == Some(Gender::Masculine)
                            && morph.number == Some(Number::Singular)
                    }
                ));

                // Perfective feminine singular: हुई
                assert!(has_morph(
                    &m,
                    "हुई",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.aspect == Some(Aspect::Perfect)
                            && morph.gender == Some(Gender::Feminine)
                            && morph.number == Some(Number::Singular)
                    }
                ));

                // Imperfect past copula feminine plural: थीं
                assert!(has_morph(
                    &m,
                    "थीं",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.aspect == Some(Aspect::Imperfect)
                            && morph.tense == Some(Tense::Past)
                            && morph.gender == Some(Gender::Feminine)
                            && morph.number == Some(Number::Plural)
                    }
                ));
            }

            #[test]
            fn hona_three_way_imperative_politeness() {
                let m = build("होना", PartOfSpeech::Verb);

                // तू (most intimate) → हो: maps to Polite::Intimate, not Informal,
                // because Informal is reserved for the ordinary T-form (तुम).
                assert!(has_morph(
                    &m,
                    "हो",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.mood == Some(Mood::Imperative)
                            && morph.politeness == Some(Polite::Intimate)
                            && morph.person == Some(Person::Second)
                    }
                ));

                // तुम (ordinary informal T-form) → होओ: maps to Polite::Informal.
                // This is the register you'd use with friends and peers; it lines up
                // with French tu / German du in everyday use.
                assert!(has_morph(
                    &m,
                    "होओ",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.mood == Some(Mood::Imperative)
                            && morph.politeness == Some(Polite::Informal)
                    }
                ));

                // Formal present imperative: होइये (आप form, homophonous with 3p.pres.imp)
                assert!(has_morph(
                    &m,
                    "होइये",
                    "होना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.mood == Some(Mood::Imperative)
                            && morph.politeness == Some(Polite::Formal)
                    }
                ));
            }

            #[test]
            fn future_13_form_carries_both_person_readings() {
                // करना's 13.p.m.fut.ind form is करेंगे, which means both "we will do"
                // (1p) and "they will do" (3p). Both morphologies must be attached.
                let m = build("करना", PartOfSpeech::Verb);
                let het = Heteronym {
                    word: "करेंगे".to_string(),
                    lemma: "करना".to_string(),
                    pos: PartOfSpeech::Verb,
                };
                let morphs = m.get(&het).expect("missing करेंगे morphology");
                assert!(
                    morphs.iter().any(|mm| mm.person == Some(Person::First)
                        && mm.number == Some(Number::Plural)
                        && mm.tense == Some(Tense::Future)),
                    "करेंगे should have a 1p.fut reading; got {morphs:?}"
                );
                assert!(
                    morphs
                        .iter()
                        .any(|mm| mm.person == Some(Person::Third)
                            && mm.number == Some(Number::Plural)),
                    "करेंगे should have a 3p.fut reading; got {morphs:?}"
                );
            }

            #[test]
            fn noun_larka_oblique_plural_is_accusative() {
                let m = build("लड़का", PartOfSpeech::Noun);
                // लड़कों is obl.p → Accusative (Hindi-style two-case system)
                assert!(has_morph(
                    &m,
                    "लड़कों",
                    "लड़का",
                    PartOfSpeech::Noun,
                    |morph| {
                        morph.case == Some(Case::Accusative) && morph.number == Some(Number::Plural)
                    }
                ));
                // लड़के is BOTH dir.p (Nominative) AND obl.s (Accusative) — both
                // tag sets should be kept on the same word.
                let het = Heteronym {
                    word: "लड़के".to_string(),
                    lemma: "लड़का".to_string(),
                    pos: PartOfSpeech::Noun,
                };
                let morphs = m.get(&het).expect("missing लड़के morphology");
                assert!(
                    morphs
                        .iter()
                        .any(|mm| mm.case == Some(Case::Nominative)
                            && mm.number == Some(Number::Plural)),
                    "लड़के should have dir.p reading"
                );
                assert!(
                    morphs.iter().any(|mm| mm.case == Some(Case::Accusative)
                        && mm.number == Some(Number::Singular)),
                    "लड़के should have obl.s reading"
                );
            }

            #[test]
            fn noun_vocative_plural_of_larka() {
                let m = build("लड़का", PartOfSpeech::Noun);
                assert!(has_morph(
                    &m,
                    "लड़को",
                    "लड़का",
                    PartOfSpeech::Noun,
                    |morph| {
                        morph.case == Some(Case::Vocative) && morph.number == Some(Number::Plural)
                    }
                ));
            }

            #[test]
            fn adjective_accha_12_cells_with_expected_distribution() {
                let m = build("अच्छा", PartOfSpeech::Adj);
                // All four masculine direct cases collapse into just two surface forms
                // but each cell should still have its own morphology.
                assert!(has_morph(
                    &m,
                    "अच्छा",
                    "अच्छा",
                    PartOfSpeech::Adj,
                    |morph| {
                        morph.gender == Some(Gender::Masculine)
                            && morph.number == Some(Number::Singular)
                            && morph.case == Some(Case::Nominative)
                    }
                ));
                assert!(has_morph(
                    &m,
                    "अच्छे",
                    "अच्छा",
                    PartOfSpeech::Adj,
                    |morph| {
                        morph.gender == Some(Gender::Masculine)
                            && morph.number == Some(Number::Plural)
                            && morph.case == Some(Case::Nominative)
                    }
                ));
                // Feminine forms: सारी अच्छी (same form in all cells but still mapped)
                assert!(has_morph(
                    &m,
                    "अच्छी",
                    "अच्छा",
                    PartOfSpeech::Adj,
                    |morph| {
                        morph.gender == Some(Gender::Feminine) && morph.case == Some(Case::Vocative)
                    }
                ));
            }

            #[test]
            fn jana_suppletive_perfective_is_tagged_correctly() {
                let m = build("जाना", PartOfSpeech::Verb);
                // गया (went — masc sg) must be tagged as perfective for जाना
                assert!(has_morph(
                    &m,
                    "गया",
                    "जाना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.aspect == Some(Aspect::Perfect)
                            && morph.gender == Some(Gender::Masculine)
                            && morph.number == Some(Number::Singular)
                    }
                ));
                assert!(has_morph(
                    &m,
                    "गईं",
                    "जाना",
                    PartOfSpeech::Verb,
                    |morph| {
                        morph.aspect == Some(Aspect::Perfect)
                            && morph.gender == Some(Gender::Feminine)
                            && morph.number == Some(Number::Plural)
                    }
                ));
            }
        }
    }
}
