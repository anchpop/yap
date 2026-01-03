use futures::StreamExt as _;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::features::Morphology;
use language_utils::{DictionaryEntry, Heteronym, Language, PartOfSpeech};
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
    frequencies: &Vec<language_utils::FrequencyEntry<String>>,
) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
    // Process sentences to get unique words and track occurrences
    let mut target_language_heteronyms = BTreeMap::new();
    for entry in frequencies {
        if let Some(heteronym) = entry.lexeme.heteronym() {
            target_language_heteronyms
                .entry(heteronym.clone())
                .or_insert(entry.count);
        }
    }

    // Try Wiktionary first for supported languages
    let mut morphology =
        wiktionary_morphology::create_morphology_from_wiktionary(language, frequencies)
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

    pub async fn get_morphology(
        language: Language,
        heteronym: Heteronym<String>,
        chat_client: &ChatClient,
    ) -> anyhow::Result<Morphology> {
        use language_utils::features::{
            Case, FeatureSet, Gender, Mood, Number, Person, Polite, Tense,
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

        // Execute all futures concurrently
        let (gender, number, politeness, tense, person, case, mood) = futures::join!(
            gender_future,
            number_future,
            politeness_future,
            tense_future,
            person_future,
            case_future,
            mood_future
        );

        Ok(Morphology {
            gender,
            number,
            politeness,
            tense,
            person,
            case,
            mood,
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
        frequencies: &Vec<language_utils::FrequencyEntry<String>>,
    ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
        match language {
            Language::French => french::create_french_morphology(frequencies).await,
            Language::Spanish => spanish::create_spanish_morphology(frequencies).await,
            Language::German => german::create_german_morphology(frequencies).await,
            Language::Portuguese => portuguese::create_portuguese_morphology(frequencies).await,
            Language::Italian => italian::create_italian_morphology(frequencies).await,
            _ => {
                // Return empty for unsupported languages
                Ok(BTreeMap::new())
            }
        }
    }

    pub mod french {
        use super::*;
        use crate::wiktionary_conjugations::french::FrenchVerbConjugation;
        use language_utils::features::{Gender, Mood, Number, Person, Tense};
        use std::collections::HashSet;
        use std::path::Path;

        pub async fn create_french_morphology(
            frequencies: &Vec<language_utils::FrequencyEntry<String>>,
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb lemmas from frequencies (including auxiliaries)
            let mut verb_lemmas = HashSet::new();
            for entry in frequencies {
                if let Some(heteronym) = entry.lexeme.heteronym() {
                    if heteronym.pos == PartOfSpeech::Verb || heteronym.pos == PartOfSpeech::Aux {
                        verb_lemmas.insert(heteronym.lemma.clone());
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();

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
                        },
                    );
                }
            }

            morphology
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
            frequencies: &Vec<language_utils::FrequencyEntry<String>>,
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            for entry in frequencies {
                if let Some(heteronym) = entry.lexeme.heteronym() {
                    if heteronym.pos == PartOfSpeech::Verb {
                        verb_lemmas.insert(heteronym.lemma.clone());
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();

            // Step 2: Fetch Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/spanish");

            let conjugations = fetch_spanish_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology = conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
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
            frequencies: &Vec<language_utils::FrequencyEntry<String>>,
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            for entry in frequencies {
                if let Some(heteronym) = entry.lexeme.heteronym() {
                    if heteronym.pos == PartOfSpeech::Verb {
                        verb_lemmas.insert(heteronym.lemma.clone());
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();

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
            frequencies: &Vec<language_utils::FrequencyEntry<String>>,
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            // Step 1: Extract all verb lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            for entry in frequencies {
                if let Some(heteronym) = entry.lexeme.heteronym() {
                    if heteronym.pos == PartOfSpeech::Verb {
                        verb_lemmas.insert(heteronym.lemma.clone());
                    }
                }
            }

            let verb_lemmas_vec: Vec<String> = verb_lemmas.into_iter().collect();

            // Step 2: Fetch Wiktionary pages with HTML caching
            let cache_dir = Path::new(".cache/wiktionary/italian");

            let conjugations = fetch_italian_verb_conjugations(&verb_lemmas_vec, cache_dir).await?;

            // Step 3: Convert conjugations to morphology entries
            let mut morphology = BTreeMap::new();

            for (infinitive, conjugation) in conjugations.iter() {
                let verb_morphology = conjugation_to_morphology(infinitive, conjugation);
                morphology.extend(verb_morphology);
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
            frequencies: &Vec<language_utils::FrequencyEntry<String>>,
        ) -> anyhow::Result<BTreeMap<Heteronym<String>, Vec<Morphology>>> {
            let mut morphology = BTreeMap::new();

            // Step 1: Extract all verb and noun lemmas from frequencies
            let mut verb_lemmas = HashSet::new();
            let mut noun_lemmas = HashSet::new();

            for entry in frequencies {
                if let Some(heteronym) = entry.lexeme.heteronym() {
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

            // Singular forms
            add_morph(
                &declension.nominative_singular,
                Morphology {
                    gender: Some(gender),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: None,
                    person: None,
                    case: Some(Case::Nominative),
                    mood: None,
                },
            );

            add_morph(
                &declension.genitive_singular,
                Morphology {
                    gender: Some(gender),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: None,
                    person: None,
                    case: Some(Case::Genitive),
                    mood: None,
                },
            );

            add_morph(
                &declension.dative_singular,
                Morphology {
                    gender: Some(gender),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: None,
                    person: None,
                    case: Some(Case::Dative),
                    mood: None,
                },
            );

            add_morph(
                &declension.accusative_singular,
                Morphology {
                    gender: Some(gender),
                    number: Some(Number::Singular),
                    politeness: None,
                    tense: None,
                    person: None,
                    case: Some(Case::Accusative),
                    mood: None,
                },
            );

            // Plural forms (optional - some nouns are uncountable/sg-only)
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
                    },
                );
            }

            morphology
        }
    }
}
