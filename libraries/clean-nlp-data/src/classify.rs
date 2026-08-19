use crate::polysemous_words;
use language_utils::{Language, NlpAnalyzedSentence, PartOfSpeechTag};
use token_corrections::{
    TH_PREVERBAL_AUX, TokenView, ZH_MODAL_VERBS, fix_chinese, fix_hindi, fix_japanese, fix_korean,
    fix_thai, is_hindi_aux_intervener, normalize_hindi_spelling,
};
use tysm::chat_completions::ChatClient;

/// Check a token against the polysemous word list for a language, returning a reason if it matches.
fn check_polysemous(language: Language, text_lower: &str) -> Option<String> {
    let words = polysemous_words::polysemous_words(language);
    for (surface_form, meanings) in words {
        if text_lower == *surface_form {
            let desc: Vec<String> = meanings
                .iter()
                .map(|(lemma, pos)| format!("{lemma}/{pos:?}"))
                .collect();
            return Some(format!(
                "'{}' is polysemous: {}. Please tag it appropriately.",
                surface_form,
                desc.join(", ")
            ));
        }
    }
    None
}

/// Classification result for a sentence
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentenceClassification {
    /// Sentence has no known issues
    Unknown,
    /// Sentence plausibly has an issue that should be reviewed
    #[allow(unused)]
    Suspicious { reasons: Vec<String> },
}

/// Result of word correction
#[derive(Debug, Clone)]
pub struct CorrectionResult {
    /// Whether any corrections were made
    pub corrected: bool,
    /// Description of what was corrected (if anything)
    #[allow(unused)]
    pub corrections: Vec<String>,
}

/// Trait for language-specific sentence classification rules
pub trait SentenceClassifier {
    /// Classify a sentence as Unknown or Suspicious
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification;

    /// Check the first-stage LLM output and decide if a double-check pass is needed.
    /// Returns None if the output looks fine, or Some(reasons) if it should be re-checked.
    fn needs_double_check(
        &self,
        _sentence: &str,
        _tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        None
    }
}

/// Trait for language-specific word correction rules
pub trait WordCorrector {
    /// If true, skip the LLM cleaning pass — the NLP analysis is trusted as-is.
    /// The shared validation and dependency passes still run, so passthrough gold
    /// carries dep/head like every other language's.
    fn passthrough(&self) -> bool {
        false
    }

    /// Correct tokens in a sentence, returning whether any corrections were made
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult;

    /// Apply post-processing corrections to simplified tokens
    fn post_corrections(&self, _tokens: &mut Vec<SimplifiedTokenPrime>) {
        // Default implementation does nothing
    }
}

/// Returns the expected contraction lemma for a given language, text, and POS.
/// If the word is a contraction, returns Some(expected_lemma). Otherwise None.
fn contraction_lemma(
    language: Language,
    text_lower: &str,
    pos: PartOfSpeechTag,
) -> Option<&'static str> {
    match language {
        Language::French => match text_lower {
            "au" => Some("au"),
            "aux" => Some("aux"),
            "du" => Some("du"),
            "des" => Some("des"),
            _ => None,
        },
        Language::Spanish => match text_lower {
            "al" => Some("al"),
            "del" => Some("del"),
            _ => None,
        },
        Language::Portuguese => match text_lower {
            "do" => Some("do"),
            "da" => Some("da"),
            "dos" => Some("dos"),
            "das" => Some("das"),
            "no" => Some("no"),
            "na" => Some("na"),
            "nos" if pos != PartOfSpeechTag::Pron => Some("nos"),
            "nas" => Some("nas"),
            "ao" => Some("ao"),
            "aos" => Some("aos"),
            "à" => Some("à"),
            "às" => Some("às"),
            "pelo" if pos != PartOfSpeechTag::Noun => Some("pelo"),
            "pela" => Some("pela"),
            "pelos" if pos != PartOfSpeechTag::Noun => Some("pelos"),
            "pelas" => Some("pelas"),
            "num" => Some("num"),
            "numa" => Some("numa"),
            "nuns" => Some("nuns"),
            "numas" => Some("numas"),
            // de + demonstrative/pronoun contractions
            "disso" => Some("disso"),
            "disto" => Some("disto"),
            "daquilo" => Some("daquilo"),
            "desse" => Some("desse"),
            "dessa" => Some("dessa"),
            "desses" => Some("desses"),
            "dessas" => Some("dessas"),
            "deste" => Some("deste"),
            "desta" => Some("desta"),
            "destes" => Some("destes"),
            "destas" => Some("destas"),
            "daquele" => Some("daquele"),
            "daquela" => Some("daquela"),
            "daqueles" => Some("daqueles"),
            "daquelas" => Some("daquelas"),
            "dele" => Some("dele"),
            "dela" => Some("dela"),
            "deles" => Some("deles"),
            "delas" => Some("delas"),
            // em + demonstrative/pronoun contractions
            "nisso" => Some("nisso"),
            "nisto" => Some("nisto"),
            "naquilo" => Some("naquilo"),
            "nesse" => Some("nesse"),
            "nessa" => Some("nessa"),
            "nesses" => Some("nesses"),
            "nessas" => Some("nessas"),
            "neste" => Some("neste"),
            "nesta" => Some("nesta"),
            "nestes" => Some("nestes"),
            "nestas" => Some("nestas"),
            "naquele" => Some("naquele"),
            "naquela" => Some("naquela"),
            "naqueles" => Some("naqueles"),
            "naquelas" => Some("naquelas"),
            "nele" => Some("nele"),
            "nela" => Some("nela"),
            "neles" => Some("neles"),
            "nelas" => Some("nelas"),
            // a + demonstrative contractions
            "àquele" => Some("àquele"),
            "àquela" => Some("àquela"),
            "àqueles" => Some("àqueles"),
            "àquelas" => Some("àquelas"),
            "àquilo" => Some("àquilo"),
            // de + adverb contractions
            "daqui" => Some("daqui"),
            "daí" => Some("daí"),
            "dali" => Some("dali"),
            _ => None,
        },
        Language::German => match text_lower {
            "im" => Some("im"),
            "am" => Some("am"),
            "zum" => Some("zum"),
            "zur" => Some("zur"),
            "vom" => Some("vom"),
            "beim" => Some("beim"),
            "ins" => Some("ins"),
            "ans" => Some("ans"),
            "aufs" => Some("aufs"),
            "durchs" => Some("durchs"),
            "fürs" => Some("fürs"),
            "ums" => Some("ums"),
            _ => None,
        },
        Language::Italian => match text_lower {
            "al" => Some("al"),
            "allo" => Some("allo"),
            "alla" => Some("alla"),
            "ai" => Some("ai"),
            "agli" => Some("agli"),
            "alle" => Some("alle"),
            "del" => Some("del"),
            "dello" => Some("dello"),
            "della" => Some("della"),
            "dei" => Some("dei"),
            "degli" => Some("degli"),
            "delle" => Some("delle"),
            "nel" => Some("nel"),
            "nello" => Some("nello"),
            "nella" => Some("nella"),
            "nei" => Some("nei"),
            "negli" => Some("negli"),
            "nelle" => Some("nelle"),
            "sul" => Some("sul"),
            "sullo" => Some("sullo"),
            "sulla" => Some("sulla"),
            "sui" => Some("sui"),
            "sugli" => Some("sugli"),
            "sulle" => Some("sulle"),
            "dal" => Some("dal"),
            "dallo" => Some("dallo"),
            "dalla" => Some("dalla"),
            "dai" => Some("dai"),
            "dagli" => Some("dagli"),
            "dalle" => Some("dalle"),
            "col" => Some("col"),
            _ => None,
        },
        _ => None,
    }
}

/// Get the classifier for a given language
pub fn get_classifier(language: Language) -> Box<dyn SentenceClassifier> {
    match language {
        Language::French => Box::new(FrenchClassifier),
        Language::German => Box::new(GermanClassifier),
        Language::Spanish => Box::new(SpanishClassifier),
        Language::Portuguese => Box::new(PortugueseClassifier),
        Language::Korean => Box::new(KoreanClassifier),
        Language::English => Box::new(EnglishClassifier),
        Language::Italian => Box::new(ItalianClassifier),
        Language::Russian => Box::new(RussianClassifier),
        Language::ChineseSimplified | Language::ChineseTraditional => Box::new(ChineseClassifier),
        Language::Japanese => Box::new(JapaneseClassifier),
        Language::Hindi => Box::new(HindiClassifier),
        Language::Thai => Box::new(ThaiClassifier),
    }
}

/// Get the corrector for a given language
pub fn get_corrector(language: Language) -> Box<dyn WordCorrector> {
    match language {
        Language::French => Box::new(FrenchCorrector),
        Language::German => Box::new(GermanCorrector),
        Language::Spanish => Box::new(SpanishCorrector),
        Language::Portuguese => Box::new(PortugueseCorrector),
        Language::Korean => Box::new(KoreanCorrector),
        Language::English => Box::new(EnglishCorrector),
        Language::Italian => Box::new(ItalianCorrector),
        Language::Russian => Box::new(RussianCorrector),
        Language::ChineseSimplified | Language::ChineseTraditional => Box::new(ChineseCorrector),
        Language::Japanese => Box::new(JapaneseCorrector),
        Language::Hindi => Box::new(HindiCorrector),
        Language::Thai => Box::new(ThaiCorrector),
    }
}

/// Spanish-specific classifier
struct SpanishClassifier;

impl SentenceClassifier for SpanishClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        // Check for Space tokens which indicate NLP parsing issues
        for token in &sentence.doc {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push(format!("Contains Space token: '{}'", sentence.sentence));
            }

            let text_lower = token.text.to_lowercase();

            // Check for lemmas containing spaces (parsing error)
            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for object/reflexive pronouns with subject pronoun lemmas
            if (text_lower == "me" && token.lemma == "yo")
                || (text_lower == "te" && token.lemma == "tú")
                || (text_lower == "lo" && token.lemma == "él")
                || (text_lower == "la" && token.lemma == "él")
                || (text_lower == "le" && token.lemma == "él")
                || (text_lower == "se" && token.lemma == "él")
                || (text_lower == "nos" && token.lemma == "yo")
                || (text_lower == "nosotros" && token.lemma == "yo")
                || (text_lower == "nosotras" && token.lemma == "yo")
            {
                reasons.push(format!(
                    "Pronoun '{}' has incorrect lemma '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for words that can be either DET or PRON depending on context
            // Rule: If it modifies a noun directly → DET. If it stands alone replacing a noun → PRON.
            let det_or_pron_words = [
                // Demonstratives
                "este", "esta", "estos", "estas", "ese", "esa", "esos", "esas", "aquel", "aquella",
                "aquellos", "aquellas", // Possessives (some forms can be both)
                "nuestro", "nuestra", "nuestros", "nuestras", "vuestro", "vuestra", "vuestros",
                "vuestras", // Indefinites/Quantifiers
                "uno", "una", "unos", "unas", "alguno", "alguna", "algunos", "algunas", "ninguno",
                "ninguna", "todo", "toda", "todos", "todas", "otro", "otra", "otros", "otras",
                "mucho", "mucha", "muchos", "muchas", "poco", "poca", "pocos", "pocas", "varios",
                "varias", "cierto", "cierta", "ciertos", "ciertas", "mismo", "misma", "mismos",
                "mismas", "tal", "tales", // Articles (can sometimes be pronouns)
                "el", "la", "los", "las",
            ];

            if det_or_pron_words.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (Rule: modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // Check common past-tense verbs are lemmatized to infinitive
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let expected_lemmas: Vec<(&str, &str)> = vec![
                    ("era", "ser"),
                    ("eran", "ser"),
                    ("estaba", "estar"),
                    ("estaban", "estar"),
                    ("tenía", "tener"),
                    ("tenían", "tener"),
                    ("hacía", "hacer"),
                    ("hacían", "hacer"),
                    ("decía", "decir"),
                    ("decían", "decir"),
                    ("iba", "ir"),
                    ("iban", "ir"),
                    ("venía", "venir"),
                    ("venían", "venir"),
                    ("veía", "ver"),
                    ("veían", "ver"),
                    ("podía", "poder"),
                    ("podían", "poder"),
                    ("quería", "querer"),
                    ("querían", "querer"),
                    ("sabía", "saber"),
                    ("sabían", "saber"),
                ];

                for (past_form, expected_infinitive) in expected_lemmas {
                    if text_lower == past_form && token.lemma != expected_infinitive {
                        reasons.push(format!(
                            "Past-tense verb '{}' has lemma '{}', but the dictionary form is '{}', look at the context to determine which is rigbt",
                            token.text, token.lemma, expected_infinitive
                        ));
                    }
                }
            }

            // Check for haber conjugations which can be either AUX or VERB depending on context
            // Rule: AUX when forming compound tenses (e.g., "he comido")
            //       VERB in impersonal constructions (e.g., "hay que ir")
            let haber_forms = [
                // Present
                "he",
                "has",
                "ha",
                "hemos",
                "habéis",
                "han",
                "hay", // Imperfect
                "había",
                "habías",
                "habíamos",
                "habíais",
                "habían", // Preterite
                "hube",
                "hubiste",
                "hubo",
                "hubimos",
                "hubisteis",
                "hubieron", // Future
                "habré",
                "habrás",
                "habrá",
                "habremos",
                "habréis",
                "habrán", // Conditional
                "habría",
                "habrías",
                "habríamos",
                "habríais",
                "habrían",
            ];

            let deber_forms = [
                // Present
                "debo",
                "debes",
                "debe",
                "debemos",
                "debéis",
                "deben", // Imperfect
                "debía",
                "debías",
                "debíamos",
                "debíais",
                "debían", // Preterite
                "debí",
                "debiste",
                "debió",
                "debimos",
                "debisteis",
                "debieron", // Future
                "deberé",
                "deberás",
                "deberá",
                "deberemos",
                "deberéis",
                "deberán", // Conditional
                "debería",
                "deberías",
                "deberíamos",
                "deberíais",
                "deberían",
            ];

            let poder_forms = [
                // Present
                "puedo",
                "puedes",
                "puede",
                "podemos",
                "podéis",
                "pueden", // Imperfect
                "podía",
                "podías",
                "podíamos",
                "podíais",
                "podían", // Preterite
                "pude",
                "pudiste",
                "pudo",
                "pudimos",
                "pudisteis",
                "pudieron", // Future
                "podré",
                "podrás",
                "podrá",
                "podremos",
                "podréis",
                "podrán", // Conditional
                "podría",
                "podrías",
                "podríamos",
                "podríais",
                "podrían",
            ];

            let saber_forms = [
                // Present
                "sé",
                "sabes",
                "sabe",
                "sabemos",
                "sabéis",
                "saben", // Imperfect
                "sabía",
                "sabías",
                "sabíamos",
                "sabíais",
                "sabían", // Preterite
                "supe",
                "supiste",
                "supo",
                "supimos",
                "supisteis",
                "supieron", // Future
                "sabré",
                "sabrás",
                "sabrá",
                "sabremos",
                "sabréis",
                "sabrán", // Conditional
                "sabría",
                "sabrías",
                "sabríamos",
                "sabríais",
                "sabrían",
            ];

            let ser_forms = [
                // Present
                "soy",
                "eres",
                "es",
                "somos",
                "sois",
                "son", // Imperfect
                "era",
                "eras",
                "éramos",
                "erais",
                "eran", // Preterite
                "fui",
                "fuiste",
                "fue",
                "fuimos",
                "fuisteis",
                "fueron", // Future
                "seré",
                "serás",
                "será",
                "seremos",
                "seréis",
                "serán", // Conditional
                "sería",
                "serías",
                "seríamos",
                "seríais",
                "serían",
            ];

            let estar_forms = [
                // Present
                "estoy",
                "estás",
                "está",
                "estamos",
                "estáis",
                "están", // Imperfect
                "estaba",
                "estabas",
                "estábamos",
                "estabais",
                "estaban", // Preterite
                "estuve",
                "estuviste",
                "estuvo",
                "estuvimos",
                "estuvisteis",
                "estuvieron", // Future
                "estaré",
                "estarás",
                "estará",
                "estaremos",
                "estaréis",
                "estarán", // Conditional
                "estaría",
                "estarías",
                "estaríamos",
                "estaríais",
                "estarían",
            ];

            if ser_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ser"
            {
                reasons.push(format!(
                    "'{}' (ser) can be either AUX or VERB depending on context. Rule: AUX when forming passive voice with past participles (e.g., 'fue construido'), VERB when used as a copula expressing identity/characteristics (e.g., 'es grande', 'soy profesor')",
                    token.text
                ));
            }

            if estar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "estar"
            {
                reasons.push(format!(
                    "'{}' (estar) can be either AUX or VERB depending on context. Rule: AUX when forming progressive tenses with gerund (e.g., 'estoy comiendo'), VERB when used as a copula expressing state/location (e.g., 'está bien', 'estoy en casa')",
                    token.text
                ));
            }

            if haber_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "haber"
            {
                reasons.push(format!(
                    "'{}' (haber) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses (e.g., 'he comido'), VERB in impersonal constructions (e.g., 'hay que ir', 'había mucha gente')",
                    token.text
                ));
            }

            if deber_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "deber"
            {
                reasons.push(format!(
                    "'{}' (deber) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation with infinitive (e.g., 'debo ir'), VERB when expressing owing (e.g., 'me debe dinero')",
                    token.text
                ));
            }

            if poder_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "poder"
            {
                reasons.push(format!(
                    "'{}' (poder) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'puedo hacerlo'), VERB when used standalone or as a noun",
                    token.text
                ));
            }

            if saber_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "saber"
            {
                reasons.push(format!(
                    "'{}' (saber) can be either AUX or VERB depending on context. Rule: AUX when expressing ability with infinitive (e.g., 'sé nadar'), VERB when expressing knowledge of facts (e.g., 'sé la respuesta')",
                    token.text
                ));
            }

            let tener_forms = [
                // Present
                "tengo",
                "tienes",
                "tiene",
                "tenemos",
                "tenéis",
                "tienen", // Imperfect
                "tenía",
                "tenías",
                "teníamos",
                "teníais",
                "tenían", // Preterite
                "tuve",
                "tuviste",
                "tuvo",
                "tuvimos",
                "tuvisteis",
                "tuvieron", // Future
                "tendré",
                "tendrás",
                "tendrá",
                "tendremos",
                "tendréis",
                "tendrán", // Conditional
                "tendría",
                "tendrías",
                "tendríamos",
                "tendríais",
                "tendrían",
            ];

            let ir_forms = [
                // Present
                "voy", "vas", "va", "vamos", "vais", "van", // Imperfect
                "iba", "ibas", "íbamos", "ibais", "iban", // Preterite (shared with ser)
                "fui", "fuiste", "fue", "fuimos", "fuisteis", "fueron", // Future
                "iré", "irás", "irá", "iremos", "iréis", "irán", // Conditional
                "iría", "irías", "iríamos", "iríais", "irían",
            ];

            let soler_forms = [
                // Present
                "suelo",
                "sueles",
                "suele",
                "solemos",
                "soléis",
                "suelen", // Imperfect
                "solía",
                "solías",
                "solíamos",
                "solíais",
                "solían",
            ];

            let acabar_forms = [
                // Present
                "acabo",
                "acabas",
                "acaba",
                "acabamos",
                "acabáis",
                "acaban", // Imperfect
                "acababa",
                "acababas",
                "acabábamos",
                "acababais",
                "acababan", // Preterite
                "acabé",
                "acabaste",
                "acabó",
                "acabamos",
                "acabasteis",
                "acabaron",
            ];

            let llevar_forms = [
                // Present
                "llevo",
                "llevas",
                "lleva",
                "llevamos",
                "lleváis",
                "llevan", // Imperfect
                "llevaba",
                "llevabas",
                "llevábamos",
                "llevabais",
                "llevaban",
            ];

            let andar_forms_es = [
                // Present
                "ando",
                "andas",
                "anda",
                "andamos",
                "andáis",
                "andan", // Imperfect
                "andaba",
                "andabas",
                "andábamos",
                "andabais",
                "andaban", // Preterite
                "anduve",
                "anduviste",
                "anduvo",
                "anduvimos",
                "anduvisteis",
                "anduvieron",
            ];

            if tener_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "tener"
            {
                reasons.push(format!(
                    "'{}' (tener) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation with 'que' + infinitive (e.g., 'tengo que ir'), VERB when expressing possession (e.g., 'tengo un perro')",
                    token.text
                ));
            }

            if ir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ir"
            {
                reasons.push(format!(
                    "'{}' (ir) can be either AUX or VERB depending on context. Rule: AUX when forming near future with 'a' + infinitive (e.g., 'voy a comer'), VERB when expressing movement (e.g., 'voy a Madrid')",
                    token.text
                ));
            }

            if soler_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "soler"
            {
                reasons.push(format!(
                    "'{}' (soler) can be either AUX or VERB depending on context. Rule: AUX when expressing habitual action with infinitive (e.g., 'suelo correr'), VERB usage is rare",
                    token.text
                ));
            }

            if acabar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "acabar"
            {
                reasons.push(format!(
                    "'{}' (acabar) can be either AUX or VERB depending on context. Rule: AUX when forming recent past with 'de' + infinitive (e.g., 'acabo de llegar'), VERB when meaning to finish (e.g., 'acabé el libro')",
                    token.text
                ));
            }

            if llevar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "llevar"
            {
                reasons.push(format!(
                    "'{}' (llevar) can be either AUX or VERB depending on context. Rule: AUX when expressing duration (e.g., 'llevo dos años aquí'), VERB when meaning to carry/wear (e.g., 'llevo una camisa')",
                    token.text
                ));
            }

            if andar_forms_es.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "andar"
            {
                reasons.push(format!(
                    "'{}' (andar) can be either AUX or VERB depending on context. Rule: AUX when forming progressive with gerund (e.g., 'ando buscando trabajo'), VERB when meaning to walk (e.g., 'ando por la calle')",
                    token.text
                ));
            }

            // Check for subject pronouns in lemma (e.g., "dormir él", "lavar tú")
            let subject_pronouns = [
                "yo", "tú", "él", "ella", "usted", "nosotros", "nosotras", "vosotros", "vosotras",
                "ellos", "ellas", "ustedes",
            ];
            for pronoun in &subject_pronouns {
                if token.lemma.ends_with(&format!(" {pronoun}")) {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which contains subject pronoun '{}' - subject pronouns should not be part of verb lemmas",
                        token.text, token.lemma, pronoun
                    ));
                    break;
                }
            }

            // Check for reflexive verb forms in lemma (should be separated)
            if token.lemma.ends_with("se") && token.lemma.len() > 2 {
                let base = &token.lemma[..token.lemma.len() - 2];
                // Check if it looks like an infinitive + se (e.g., "limitarse", "calmarse")
                if base.ends_with("ar") || base.ends_with("er") || base.ends_with("ir") {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which contains reflexive pronoun 'se' - reflexive pronouns should be separate tokens",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for object pronouns attached to infinitives in lemma (e.g., "hacerlo")
            let object_pronouns = ["lo", "la", "los", "las", "le", "les", "me", "te", "nos"];
            for pronoun in &object_pronouns {
                if token.lemma.ends_with(pronoun) && token.lemma.len() > pronoun.len() {
                    let base = &token.lemma[..token.lemma.len() - pronoun.len()];
                    if base.ends_with("ar") || base.ends_with("er") || base.ends_with("ir") {
                        reasons.push(format!(
                            "'{}' has lemma '{}' which contains object pronoun '{}' attached - pronouns should be separate tokens",
                            token.text, token.lemma, pronoun
                        ));
                        break;
                    }
                }
            }

            // Check for lemmas that look like conjugated forms rather than infinitives
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                // Check for common conjugation endings that shouldn't be in lemmas
                if lemma_lower.ends_with("ado")
                    || lemma_lower.ends_with("ido")
                    || lemma_lower.ends_with("ada")
                    || lemma_lower.ends_with("ida")
                    || lemma_lower.ends_with("ados")
                    || lemma_lower.ends_with("idos")
                    || lemma_lower.ends_with("adas")
                    || lemma_lower.ends_with("idas")
                {
                    // These are past participle forms, not infinitives
                    reasons.push(format!(
                        "'{}' has lemma '{}' which looks like a past participle rather than an infinitive",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for non-verb words being lemmatized as verbs (common error)
            if token.pos != PartOfSpeechTag::Verb && token.pos != PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                // Spanish infinitives end in -ar, -er, -ir
                if lemma_lower.ends_with("ar")
                    || lemma_lower.ends_with("er")
                    || lemma_lower.ends_with("ir")
                {
                    // This might be a verb lemma for a non-verb token
                    reasons.push(format!(
                        "'{}' (POS: {:?}) has verb-like lemma '{}' - verify this is correct",
                        token.text, token.pos, token.lemma
                    ));
                }
            }

            // Detect verbs with broken lemmas
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                // Spanish infinitives must end in -ar, -er, -ir, or -ír
                // If the lemma doesn't, the lemmatizer failed (e.g., clitic-attached imperatives
                // like "acostúmbrese" → lemma "acostúmbrese", or just garbage)
                if !lemma_lower.ends_with("ar")
                    && !lemma_lower.ends_with("er")
                    && !lemma_lower.ends_with("ir")
                    && !lemma_lower.ends_with("ír")
                    && lemma_lower != "ser"
                    && lemma_lower != "ir"
                    && lemma_lower != "ver"
                    && text_lower.len() > 3
                {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which doesn't look like a Spanish infinitive — likely a failed lemmatization",
                        token.text, token.lemma
                    ));
                }

                // Detect bogus infinitives with accents in the stem (e.g., "deberiar", "estár")
                if (lemma_lower.ends_with("ar")
                    || lemma_lower.ends_with("er")
                    || lemma_lower.ends_with("ir"))
                    && lemma_lower.len() > 2
                {
                    let stem = &lemma_lower[..lemma_lower.len() - 2];
                    if stem.contains('á')
                        || stem.contains('é')
                        || stem.contains('í')
                        || stem.contains('ó')
                        || stem.contains('ú')
                    {
                        reasons.push(format!(
                            "'{}' has lemma '{}' which looks like a bogus infinitive (accent in stem)",
                            token.text, token.lemma
                        ));
                    }
                }
            }

            // "siquiera" is always an adverb, not a noun
            if text_lower == "siquiera" && token.pos != PartOfSpeechTag::Adv {
                reasons.push(format!(
                    "'siquiera' tagged as {:?} but it's an adverb meaning 'even/at least'",
                    token.pos
                ));
            }

            // "menos" — lemma should be "menos", not "meno" (not a real Spanish word)
            if text_lower == "menos" && token.lemma == "meno" {
                reasons.push(
                    "'menos' has lemma 'meno' which is not a real Spanish word — lemma should be 'menos'"
                        .to_string(),
                );
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::Spanish, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            let is_ser = token.lemma == "ser"
                && (token.pos == PartOfSpeechTag::Aux || token.pos == PartOfSpeechTag::Verb);
            let is_estar = token.lemma == "estar"
                && (token.pos == PartOfSpeechTag::Aux || token.pos == PartOfSpeechTag::Verb);

            if (is_ser || is_estar) && token.pos == PartOfSpeechTag::Aux {
                let verb_name = if is_ser { "ser" } else { "estar" };
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' ({verb_name}) is tagged AUX but is followed by adjective '{next_text}' — if this is a copula ({verb_name} + adjective), it should be VERB, not AUX. ser is only AUX in passive with past participles (e.g., 'fue construido'); estar is only AUX in progressive with gerund (e.g., 'estoy comiendo').",
                        token.text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' ({verb_name}) is tagged AUX — please double-check: it should be VERB when used as a copula (e.g., 'es grande', 'está bien'), and only AUX when forming passive (ser) or progressive (estar) with participle/gerund.",
                        token.text
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Spanish-specific corrector
struct SpanishCorrector;

impl WordCorrector for SpanishCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        for token in &mut sentence.doc {
            let text_lower = token.text.to_lowercase();

            // Fix "no" POS - should always be Adv, not Part
            if text_lower == "no" && token.pos == PartOfSpeechTag::Part {
                corrections.push(format!("Fixed '{}' POS from Part to Adv", token.text));
                token.pos = PartOfSpeechTag::Adv;
                corrected = true;
            }

            // Fix "ella" lemma - should always be "ella", not "él"
            if text_lower == "ella" && token.lemma == "él" {
                corrections.push(format!("Fixed '{}' lemma from 'él' to 'ella'", token.text));
                token.lemma = "ella".to_string();
                corrected = true;
            }

            // Fix "menos" lemma - "meno" is not a real Spanish word
            if text_lower == "menos" && token.lemma == "meno" {
                corrections.push("Fixed 'menos' lemma from 'meno' to 'menos'".to_string());
                token.lemma = "menos".to_string();
                corrected = true;
            }

            // Normalize demonstrative pronoun lemmas to masculine form
            let demonstrative_fixes: &[(&str, &str)] = &[
                ("esto", "este"),
                ("eso", "ese"),
                ("aquello", "aquel"),
                ("esta", "este"),
                ("estas", "este"),
                ("estos", "este"),
                ("esa", "ese"),
                ("esas", "ese"),
                ("esos", "ese"),
                ("aquella", "aquel"),
                ("aquellas", "aquel"),
                ("aquellos", "aquel"),
            ];
            for &(form, expected_lemma) in demonstrative_fixes {
                if text_lower == form
                    && (token.pos == PartOfSpeechTag::Pron || token.pos == PartOfSpeechTag::Det)
                    && token.lemma != expected_lemma
                {
                    corrections.push(format!(
                        "Fixed demonstrative '{}' lemma from '{}' to '{}'",
                        token.text, token.lemma, expected_lemma
                    ));
                    token.lemma = expected_lemma.to_string();
                    corrected = true;
                    break;
                }
            }

            // Normalize indefinite article lemmas to "uno"
            if (text_lower == "un"
                || text_lower == "una"
                || text_lower == "unos"
                || text_lower == "unas")
                && token.pos == PartOfSpeechTag::Det
                && token.lemma != "uno"
            {
                corrections.push(format!(
                    "Fixed indefinite article '{}' lemma from '{}' to 'uno'",
                    token.text, token.lemma
                ));
                token.lemma = "uno".to_string();
                corrected = true;
            }

            // Contractions keep their contracted form as lemma
            if let Some(expected) = contraction_lemma(Language::Spanish, &text_lower, token.pos)
                && token.lemma != expected
            {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to '{expected}'",
                    token.text, token.lemma
                ));
                token.lemma = expected.to_string();
                corrected = true;
            }
        }

        CorrectionResult {
            corrected,
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Fix "no" POS
            if text_lower == "no" && token.pos == PartOfSpeechTag::Part {
                token.pos = PartOfSpeechTag::Adv;
            }

            // Normalize demonstrative lemmas
            let demonstrative_fixes: &[(&str, &str)] = &[
                ("esto", "este"),
                ("eso", "ese"),
                ("aquello", "aquel"),
                ("esta", "este"),
                ("estas", "este"),
                ("estos", "este"),
                ("esa", "ese"),
                ("esas", "ese"),
                ("esos", "ese"),
                ("aquella", "aquel"),
                ("aquellas", "aquel"),
                ("aquellos", "aquel"),
            ];
            for &(form, expected_lemma) in demonstrative_fixes {
                if text_lower == form
                    && (token.pos == PartOfSpeechTag::Pron || token.pos == PartOfSpeechTag::Det)
                    && token.lemma != expected_lemma
                {
                    token.lemma = expected_lemma.to_string();
                    break;
                }
            }

            // Normalize indefinite article lemmas
            if (text_lower == "un"
                || text_lower == "una"
                || text_lower == "unos"
                || text_lower == "unas")
                && token.pos == PartOfSpeechTag::Det
                && token.lemma != "uno"
            {
                token.lemma = "uno".to_string();
            }

            if let Some(expected) = contraction_lemma(Language::Spanish, &text_lower, token.pos)
                && token.lemma != expected
            {
                token.lemma = expected.to_string();
            }

            // Normalize clitic pronoun lemmas to singular base form
            if token.pos == PartOfSpeechTag::Pron {
                let expected = match text_lower.as_str() {
                    "los" => Some("lo"),
                    "las" => Some("la"),
                    "les" => Some("le"),
                    _ => None,
                };
                if let Some(expected) = expected
                    && token.lemma != expected
                {
                    token.lemma = expected.to_string();
                }
            }
        }
    }
}

/// Portuguese-specific classifier
struct PortugueseClassifier;

impl SentenceClassifier for PortugueseClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        // Check for Space tokens which indicate NLP parsing issues
        for token in &sentence.doc {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push(format!("Contains Space token: '{}'", sentence.sentence));
            }

            // Check for PROPN (proper noun) tags - often over-classified
            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun, but the legacy NLP pipeline often over-classifies things as proper nouns",
                    token.text
                ));
            }

            let text_lower = token.text.to_lowercase();

            // Check for words containing hyphens that should be split
            if token.text.contains('-') && token.text != "-" {
                reasons.push(format!(
                    "'{}' contains a hyphen and should likely be split into separate tokens (e.g., 'Deixe-me' → 'Deixe', '-', 'me')",
                    token.text
                ));
            }

            // Check for "à" with incorrect lemma "a o" (spaCy bug: should be "a a")
            if text_lower == "à" && token.lemma == "a o" {
                reasons.push(
                    "'à' has lemma 'a o' but should have lemma 'a a' — spaCy incorrectly treats à (a+a) the same as ao (a+o)".to_string()
                );
            }

            // Check for lemmas containing spaces (parsing error), excluding known cases handled above
            if token.lemma.contains(' ') && !(text_lower == "à" && token.lemma == "a o") {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for verbs/auxiliaries with themselves as lemma (no morphological analysis)
            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.text.to_lowercase() == token.lemma.to_lowercase()
            {
                reasons.push(format!(
                    "Verb/Aux '{}' should be lemmatized to infinitive",
                    token.text,
                ));
            }

            // Check for object/reflexive pronouns with subject pronoun lemmas
            if (text_lower == "me" && token.lemma == "eu")
                || (text_lower == "te" && token.lemma == "tu")
                || (text_lower == "o" && token.lemma == "ele" && token.pos == PartOfSpeechTag::Pron)
                || (text_lower == "a" && token.lemma == "ele" && token.pos == PartOfSpeechTag::Pron)
                || (text_lower == "lhe" && token.lemma == "ele")
                || (text_lower == "se" && token.lemma == "ele")
                || (text_lower == "nos" && token.lemma == "eu")
                || (text_lower == "vos" && token.lemma == "tu")
            {
                reasons.push(format!(
                    "Pronoun '{}' has incorrect lemma '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for "nos" with lemma "nós" - could be wrong if it's an object pronoun
            if text_lower == "nos" && token.lemma == "nós" && token.pos == PartOfSpeechTag::Pron {
                reasons.push(
                    "'nos' has lemma 'nós' - check if this is correct. If 'nos' is an object pronoun (e.g., 'ele nos disse'), it should not have lemma 'nós' (subject pronoun)".to_string()
                );
            }

            // Check for words that can be either DET or PRON depending on context
            // Rule: If it modifies a noun directly → DET. If it stands alone replacing a noun → PRON.
            let det_or_pron_words = [
                // Demonstratives
                "este", "esta", "estes", "estas", "esse", "essa", "esses", "essas", "aquele",
                "aquela", "aqueles", "aquelas", "isto", "isso", "aquilo",
                // Possessives
                "meu", "minha", "meus", "minhas", "teu", "tua", "teus", "tuas", "seu", "sua",
                "seus", "suas", "nosso", "nossa", "nossos", "nossas", "vosso", "vossa", "vossos",
                "vossas", // Indefinites/Quantifiers
                "um", "uma", "uns", "umas", "algum", "alguma", "alguns", "algumas", "nenhum",
                "nenhuma", "todo", "toda", "todos", "todas", "outro", "outra", "outros", "outras",
                "muito", "muita", "muitos", "muitas", "pouco", "pouca", "poucos", "poucas",
                "vários", "várias", "certo", "certa", "certos", "certas", "mesmo", "mesma",
                "mesmos", "mesmas", "tal", "tais",
                // Articles (can sometimes be pronouns)
                "o", "a", "os", "as",
            ];

            if det_or_pron_words.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (Rule: modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // Check common past-tense verbs are lemmatized to infinitive
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let expected_lemmas: Vec<(&str, &str)> = vec![
                    ("era", "ser"),
                    ("eram", "ser"),
                    ("estava", "estar"),
                    ("estavam", "estar"),
                    ("estávamos", "estar"),
                    ("tinha", "ter"),
                    ("tinham", "ter"),
                    ("fazia", "fazer"),
                    ("faziam", "fazer"),
                    ("dizia", "dizer"),
                    ("diziam", "dizer"),
                    ("ia", "ir"),
                    ("iam", "ir"),
                    ("vinha", "vir"),
                    ("vinham", "vir"),
                    ("via", "ver"),
                    ("viam", "ver"),
                    ("podia", "poder"),
                    ("podiam", "poder"),
                    ("queria", "querer"),
                    ("queriam", "querer"),
                    ("sabia", "saber"),
                    ("sabiam", "saber"),
                ];

                for (past_form, expected_infinitive) in expected_lemmas {
                    if text_lower == past_form && token.lemma != expected_infinitive {
                        reasons.push(format!(
                            "Past-tense verb '{}' has lemma '{}', but the dictionary form is '{}', look at the context to determine which is right",
                            token.text, token.lemma, expected_infinitive
                        ));
                    }
                }

                // "fosse" is the past subjunctive of BOTH "ser" (to be) and "ir" (to go)
                // spaCy often defaults to "ir" but it's frequently "ser" in context
                // e.g., "Se eu fosse rico" = ser, "Se eu fosse ao mercado" = ir
                if text_lower == "fosse"
                    || text_lower == "fôssemos"
                    || text_lower == "fossem"
                    || text_lower == "fosses"
                    || text_lower == "fôsseis"
                {
                    reasons.push(format!(
                        "'{}' is the past subjunctive of both 'ser' (to be) and 'ir' (to go). Currently lemmatized as '{}'. Check context: 'Se eu fosse rico' → ser; 'Se eu fosse ao mercado' → ir",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for ter conjugations which can be either AUX or VERB depending on context
            // Rule: AUX when forming compound tenses (e.g., "tenho comido")
            //       VERB when expressing possession (e.g., "tenho um livro")
            let ter_forms = [
                // Present
                "tenho",
                "tens",
                "tem",
                "temos",
                "tendes",
                "têm", // Imperfect
                "tinha",
                "tinhas",
                "tínhamos",
                "tínheis",
                "tinham", // Preterite
                "tive",
                "tiveste",
                "teve",
                "tivemos",
                "tivestes",
                "tiveram", // Future
                "terei",
                "terás",
                "terá",
                "teremos",
                "tereis",
                "terão", // Conditional
                "teria",
                "terias",
                "teríamos",
                "teríeis",
                "teriam",
            ];

            let haver_forms = [
                // Present
                "hei",
                "hás",
                "há",
                "havemos",
                "haveis",
                "hão",
                "há", // Imperfect
                "havia",
                "havias",
                "havíamos",
                "havíeis",
                "haviam", // Preterite
                "houve",
                "houveste",
                "houvemos",
                "houvestes",
                "houveram", // Future
                "haverei",
                "haverás",
                "haverá",
                "haveremos",
                "havereis",
                "haverão", // Conditional
                "haveria",
                "haverias",
                "haveríamos",
                "haveríeis",
                "haveriam",
            ];

            let dever_forms = [
                // Present
                "devo",
                "deves",
                "deve",
                "devemos",
                "deveis",
                "devem", // Imperfect
                "devia",
                "devias",
                "devíamos",
                "devíeis",
                "deviam", // Preterite
                "devi",
                "deveste",
                "deveu",
                "devemos",
                "devestes",
                "deveram", // Future
                "deverei",
                "deverás",
                "deverá",
                "deveremos",
                "devereis",
                "deverão", // Conditional
                "deveria",
                "deverias",
                "deveríamos",
                "deveríeis",
                "deveriam",
            ];

            let poder_forms = [
                // Present
                "posso",
                "podes",
                "pode",
                "podemos",
                "podeis",
                "podem", // Imperfect
                "podia",
                "podias",
                "podíamos",
                "podíeis",
                "podiam", // Preterite
                "pude",
                "pudeste",
                "pôde",
                "pudemos",
                "pudestes",
                "puderam", // Future
                "poderei",
                "poderás",
                "poderá",
                "poderemos",
                "podereis",
                "poderão", // Conditional
                "poderia",
                "poderias",
                "poderíamos",
                "poderíeis",
                "poderiam",
            ];

            let saber_forms = [
                // Present
                "sei",
                "sabes",
                "sabe",
                "sabemos",
                "sabeis",
                "sabem", // Imperfect
                "sabia",
                "sabias",
                "sabíamos",
                "sabíeis",
                "sabiam", // Preterite
                "soube",
                "soubeste",
                "soube",
                "soubemos",
                "soubestes",
                "souberam", // Future
                "saberei",
                "saberás",
                "saberá",
                "saberemos",
                "sabereis",
                "saberão", // Conditional
                "saberia",
                "saberias",
                "saberíamos",
                "saberíeis",
                "saberiam",
            ];

            let ser_forms = [
                // Present
                "sou",
                "és",
                "é",
                "somos",
                "sois",
                "são", // Imperfect
                "era",
                "eras",
                "éramos",
                "éreis",
                "eram", // Preterite
                "fui",
                "foste",
                "foi",
                "fomos",
                "fostes",
                "foram", // Future
                "serei",
                "serás",
                "será",
                "seremos",
                "sereis",
                "serão", // Conditional
                "seria",
                "serias",
                "seríamos",
                "seríeis",
                "seriam",
            ];

            let estar_forms = [
                // Present
                "estou",
                "estás",
                "está",
                "estamos",
                "estais",
                "estão", // Imperfect
                "estava",
                "estavas",
                "estávamos",
                "estáveis",
                "estavam", // Preterite
                "estive",
                "estiveste",
                "esteve",
                "estivemos",
                "estivestes",
                "estiveram", // Future
                "estarei",
                "estarás",
                "estará",
                "estaremos",
                "estareis",
                "estarão", // Conditional
                "estaria",
                "estarias",
                "estaríamos",
                "estaríeis",
                "estariam",
            ];

            if ser_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ser"
            {
                reasons.push(format!(
                    "'{}' (ser) can be either AUX or VERB depending on context. Rule: AUX when forming passive voice with past participles (e.g., 'foi construído'), VERB when used as a copula expressing identity/characteristics (e.g., 'é grande', 'sou professor')",
                    token.text
                ));
            }

            if estar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "estar"
            {
                reasons.push(format!(
                    "'{}' (estar) can be either AUX or VERB depending on context. Rule: AUX when forming progressive tenses with gerund (e.g., 'estou comendo'), VERB when used as a copula expressing state/location (e.g., 'está bem', 'estou em casa')",
                    token.text
                ));
            }

            if ter_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ter"
            {
                reasons.push(format!(
                    "'{}' (ter) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses (e.g., 'tenho comido'), VERB when expressing possession (e.g., 'tenho um livro', 'tem fome')",
                    token.text
                ));
            }

            if haver_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "haver"
            {
                reasons.push(format!(
                    "'{}' (haver) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses (e.g., 'hei de fazer'), VERB in impersonal constructions (e.g., 'há pessoas', 'havia tempo')",
                    token.text
                ));
            }

            if dever_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "dever"
            {
                reasons.push(format!(
                    "'{}' (dever) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation with infinitive (e.g., 'devo ir'), VERB when expressing owing (e.g., 'devo dinheiro')",
                    token.text
                ));
            }

            if poder_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "poder"
            {
                reasons.push(format!(
                    "'{}' (poder) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'posso fazê-lo'), VERB when used standalone or as a noun",
                    token.text
                ));
            }

            if saber_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "saber"
            {
                reasons.push(format!(
                    "'{}' (saber) can be either AUX or VERB depending on context. Rule: AUX when expressing ability with infinitive (e.g., 'sei nadar'), VERB when expressing knowledge of facts (e.g., 'sei a resposta')",
                    token.text
                ));
            }

            let ir_forms = [
                // Present
                "vou", "vais", "vai", "vamos", "ides", "vão", // Imperfect
                "ia", "ias", "íamos", "íeis", "iam", // Preterite (shared with ser)
                "fui", "foste", "foi", "fomos", "fostes", "foram", // Future
                "irei", "irás", "irá", "iremos", "ireis", "irão", // Conditional
                "iria", "irias", "iríamos", "iríeis", "iriam",
            ];

            let acabar_forms = [
                // Present
                "acabo",
                "acabas",
                "acaba",
                "acabamos",
                "acabais",
                "acabam", // Imperfect
                "acabava",
                "acabavas",
                "acabávamos",
                "acabáveis",
                "acabavam", // Preterite
                "acabei",
                "acabaste",
                "acabou",
                "acabámos",
                "acabastes",
                "acabaram",
            ];

            let andar_forms = [
                // Present
                "ando",
                "andas",
                "anda",
                "andamos",
                "andais",
                "andam", // Imperfect
                "andava",
                "andavas",
                "andávamos",
                "andáveis",
                "andavam", // Preterite
                "andei",
                "andaste",
                "andou",
                "andámos",
                "andastes",
                "andaram",
            ];

            let ficar_forms = [
                // Present
                "fico",
                "ficas",
                "fica",
                "ficamos",
                "ficais",
                "ficam", // Imperfect
                "ficava",
                "ficavas",
                "ficávamos",
                "ficáveis",
                "ficavam", // Preterite
                "fiquei",
                "ficaste",
                "ficou",
                "ficámos",
                "ficastes",
                "ficaram",
            ];

            if ir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ir"
            {
                reasons.push(format!(
                    "'{}' (ir) can be either AUX or VERB depending on context. Rule: AUX when forming near future with infinitive (e.g., 'vou comer'), VERB when expressing movement (e.g., 'vou a Lisboa')",
                    token.text
                ));
            }

            if acabar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "acabar"
            {
                reasons.push(format!(
                    "'{}' (acabar) can be either AUX or VERB depending on context. Rule: AUX when forming recent past with 'de' + infinitive (e.g., 'acabo de chegar'), VERB when meaning to finish (e.g., 'acabei o livro')",
                    token.text
                ));
            }

            if andar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "andar"
            {
                reasons.push(format!(
                    "'{}' (andar) can be either AUX or VERB depending on context. Rule: AUX when forming progressive with 'a' + infinitive (e.g., 'ando a estudar'), VERB when meaning to walk (e.g., 'ando pela rua')",
                    token.text
                ));
            }

            if ficar_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "ficar"
            {
                reasons.push(format!(
                    "'{}' (ficar) can be either AUX or VERB depending on context. Rule: AUX when expressing resultative state (e.g., 'fiquei surpreso'), VERB when meaning to stay/remain (e.g., 'fico em casa')",
                    token.text
                ));
            }

            // Detect verbs with broken lemmas — Portuguese infinitives must end in -ar, -er, -ir, -or (pôr)
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                if !lemma_lower.ends_with("ar")
                    && !lemma_lower.ends_with("er")
                    && !lemma_lower.ends_with("ir")
                    && !lemma_lower.ends_with("or")  // pôr and derivatives
                    && lemma_lower != "ir"
                    && lemma_lower != "ser"
                    && lemma_lower != "ter"
                    && lemma_lower != "ver"
                    && lemma_lower != "vir"
                    && lemma_lower != "pôr"
                    && text_lower.len() > 2
                {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which doesn't look like a Portuguese infinitive — likely a failed lemmatization",
                        token.text, token.lemma
                    ));
                }

                // Detect bogus infinitives with accents in the stem
                if lemma_lower.ends_with("ar")
                    || lemma_lower.ends_with("er")
                    || lemma_lower.ends_with("ir")
                {
                    // Get the stem by removing last 2 chars (char-safe)
                    let stem: String = lemma_lower
                        .chars()
                        .take(lemma_lower.chars().count().saturating_sub(2))
                        .collect();
                    if stem.contains('á')
                        || stem.contains('é')
                        || stem.contains('í')
                        || stem.contains('ó')
                        || stem.contains('ú')
                    {
                        reasons.push(format!(
                            "'{}' has lemma '{}' which looks like a bogus infinitive (accent in stem)",
                            token.text, token.lemma
                        ));
                    }
                }
            }

            // Imperatives mistagged as Noun or Intj
            // Sentence-initial capitalized words that look like verb conjugations
            // but get tagged as Noun/Intj due to capitalization
            if (token.pos == PartOfSpeechTag::Noun || token.pos == PartOfSpeechTag::Intj)
                && token.text.chars().next().is_some_and(|c| c.is_uppercase())
                && token.text.to_lowercase() == token.lemma.to_lowercase()
            {
                // If the surface form looks like it could be an imperative
                // (ends in common Portuguese verb endings: -a, -e, -i, -ai, -ei, -am, -em)
                let endings = ["a", "e", "i", "ai", "ei", "am", "em", "ão"];
                let could_be_verb = endings.iter().any(|e| text_lower.ends_with(e));
                if could_be_verb && text_lower.len() > 3 {
                    reasons.push(format!(
                        "'{}' tagged as {:?} with lemma '{}' but could be an imperative verb form. Capitalization at sentence start may be misleading the tagger.",
                        token.text, token.pos, token.lemma
                    ));
                }
            }

            // Uppercase lemmas should be normalized to lowercase for verbs
            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                reasons.push(format!(
                    "'{}' has uppercase lemma '{}' — verb lemmas should be lowercase",
                    token.text, token.lemma
                ));
            }

            // "Milhões" should have lemma "milhão" (singular), not itself
            if text_lower == "milhões" && token.lemma != "milhão" {
                reasons.push(format!(
                    "'milhões' has lemma '{}' but should be 'milhão' (singular form)",
                    token.lemma
                ));
            }

            // "quantas" should have lemma "quanto", like "quantos"
            if (text_lower == "quantas" || text_lower == "quanta") && token.lemma != "quanto" {
                reasons.push(format!(
                    "'{}' has lemma '{}' but should be 'quanto' (masculine singular base form)",
                    token.text, token.lemma
                ));
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::Portuguese, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // Check estar tagged AUX — might be a copula that should be VERB
            if token.lemma == "estar" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (estar) is tagged AUX but is followed by adjective '{}' — if this is a copula (estar + adjective describing a state), it should be VERB, not AUX. estar is only AUX when forming progressive tenses with gerund (e.g., 'estou comendo').",
                        token.text, next_text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (estar) is tagged AUX but the next word '{}' ({:?}) doesn't look like a gerund — please double-check whether this is really a progressive tense (AUX) or a copula (VERB).",
                        token.text,
                        next_text,
                        next_pos.unwrap_or(PartOfSpeechTag::X)
                    ));
                }
            }

            // Check ser tagged AUX — might be a copula that should be VERB
            if token.lemma == "ser" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (ser) is tagged AUX but is followed by adjective '{}' — if this is a copula (ser + adjective describing identity/characteristics), it should be VERB, not AUX. ser is only AUX when forming passive voice with past participles (e.g., 'foi construído').",
                        token.text, next_text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (ser) is tagged AUX but the next word '{}' ({:?}) doesn't look like a past participle — please double-check whether this is really a passive (AUX) or a copula (VERB).",
                        token.text,
                        next_text,
                        next_pos.unwrap_or(PartOfSpeechTag::X)
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Portuguese-specific corrector
struct PortugueseCorrector;

impl WordCorrector for PortugueseCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        // Use fold to build new token list, splitting hyphens as we go
        let original_tokens = std::mem::take(&mut sentence.doc);
        sentence.doc = original_tokens
            .into_iter()
            .fold(Vec::new(), |mut acc, mut token| {
                let text_lower = token.text.to_lowercase();

                // Fix "ela" lemma - should always be "ela", not "ele"
                if text_lower == "ela" && token.lemma != "ela" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'ela'",
                        token.text, token.lemma
                    ));
                    token.lemma = "ela".to_string();
                    corrected = true;
                }

                // Fix clitic pronoun lemma consistency
                let portuguese_pronoun_fixes: &[(&str, &str)] = &[
                    ("me", "me"),
                    ("te", "te"),
                    ("se", "se"),
                    ("lhe", "lhe"),
                    ("nos", "nos"),
                    ("vos", "vos"),
                    ("lhes", "lhes"),
                ];
                for &(form, expected_lemma) in portuguese_pronoun_fixes {
                    if text_lower == form
                        && token.pos == PartOfSpeechTag::Pron
                        && token.lemma != expected_lemma
                    {
                        // Don't fix "nos" if lemma is "nós" — that might be correct (subject pronoun)
                        // The classifier flags this for the LLM to review
                        if form == "nos" && token.lemma == "nós" {
                            break;
                        }
                        corrections.push(format!(
                            "Fixed pronoun '{}' lemma from '{}' to '{}'",
                            token.text, token.lemma, expected_lemma
                        ));
                        token.lemma = expected_lemma.to_string();
                        corrected = true;
                        break;
                    }
                }

                // Fix feminine noun lemmas — these should keep their feminine form
                let feminine_noun_fixes: &[(&str, &str)] = &[
                    ("irmã", "irmã"),
                    ("irmãs", "irmã"),
                    ("filha", "filha"),
                    ("filhas", "filha"),
                    ("mãe", "mãe"),
                    ("mães", "mãe"),
                    ("avó", "avó"),
                    ("avós", "avó"),
                ];
                for &(form, expected_lemma) in feminine_noun_fixes {
                    if text_lower == form
                        && token.pos == PartOfSpeechTag::Noun
                        && token.lemma != expected_lemma
                    {
                        corrections.push(format!(
                            "Fixed feminine noun '{}' lemma from '{}' to '{}'",
                            token.text, token.lemma, expected_lemma
                        ));
                        token.lemma = expected_lemma.to_string();
                        corrected = true;
                        break;
                    }
                }

                // Normalize uppercase verb lemmas to lowercase
                if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                    && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    let lower = token.lemma.to_lowercase();
                    corrections.push(format!(
                        "Normalized verb lemma '{}' to lowercase '{}'",
                        token.lemma, lower
                    ));
                    token.lemma = lower;
                    corrected = true;
                }

                // Fix "milhões" lemma
                if text_lower == "milhões" && token.lemma != "milhão" {
                    corrections.push(format!(
                        "Fixed 'milhões' lemma from '{}' to 'milhão'",
                        token.lemma
                    ));
                    token.lemma = "milhão".to_string();
                    corrected = true;
                }

                // Fix "quantas"/"quanta" lemma
                if (text_lower == "quantas" || text_lower == "quanta") && token.lemma != "quanto" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'quanto'",
                        token.text, token.lemma
                    ));
                    token.lemma = "quanto".to_string();
                    corrected = true;
                }

                // Contractions keep their contracted form as lemma
                if let Some(expected) =
                    contraction_lemma(Language::Portuguese, &text_lower, token.pos)
                    && token.lemma != expected
                {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to '{expected}'",
                        token.text, token.lemma
                    ));
                    token.lemma = expected.to_string();
                    corrected = true;
                }

                // Split words starting with hyphen (e.g., "-me" from "Deixe-me")
                if token.text.starts_with('-')
                    && token.text.len() > 1
                    && !acc.is_empty()
                    && acc.last().unwrap().whitespace.is_empty()
                {
                    // Remove hyphen from beginning of token
                    let original_text = token.text.clone();
                    token.text = token.text[1..].to_string();

                    corrections.push(format!(
                        "Split hyphen from beginning of '{original_text}' into separate token"
                    ));

                    // Create separate hyphen token
                    let hyphen_token = language_utils::DocToken {
                        text: "-".to_string(),
                        whitespace: String::new(), // No whitespace after hyphen
                        pos: PartOfSpeechTag::Punct,
                        lemma: "-".to_string(),
                        morph: std::collections::BTreeMap::new(),
                    };

                    acc.push(hyphen_token);
                    acc.push(token);
                    corrected = true;
                }
                // Split words ending in hyphen with no whitespace after (e.g., "Deixe-" from "Deixe-me")
                else if token.text.ends_with('-')
                    && token.whitespace.is_empty()
                    && token.text.len() > 1
                {
                    // Remove hyphen from original token
                    let original_text = token.text.clone();
                    let original_whitespace = token.whitespace.clone();
                    token.text.pop();
                    token.whitespace = String::new(); // No whitespace after word part

                    corrections.push(format!(
                        "Split hyphen from end of '{original_text}' into separate token"
                    ));

                    // Create separate hyphen token with the original whitespace
                    let hyphen_token = language_utils::DocToken {
                        text: "-".to_string(),
                        whitespace: original_whitespace,
                        pos: PartOfSpeechTag::Punct,
                        lemma: "-".to_string(),
                        morph: std::collections::BTreeMap::new(),
                    };

                    acc.push(token);
                    acc.push(hyphen_token);
                    corrected = true;
                } else {
                    acc.push(token);
                }

                acc
            });

        CorrectionResult {
            corrected,
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Clitic pronoun lemma consistency
            let portuguese_pronoun_fixes: &[(&str, &str)] = &[
                ("me", "me"),
                ("te", "te"),
                ("se", "se"),
                ("lhe", "lhe"),
                ("nos", "nos"),
                ("vos", "vos"),
                ("lhes", "lhes"),
            ];
            for &(form, expected_lemma) in portuguese_pronoun_fixes {
                if text_lower == form
                    && token.pos == PartOfSpeechTag::Pron
                    && token.lemma != expected_lemma
                {
                    if form == "nos" && token.lemma == "nós" {
                        break;
                    }
                    token.lemma = expected_lemma.to_string();
                    break;
                }
            }

            // Feminine noun lemma consistency
            let feminine_noun_fixes: &[(&str, &str)] = &[
                ("irmã", "irmã"),
                ("irmãs", "irmã"),
                ("filha", "filha"),
                ("filhas", "filha"),
                ("mãe", "mãe"),
                ("mães", "mãe"),
                ("avó", "avó"),
                ("avós", "avó"),
            ];
            for &(form, expected_lemma) in feminine_noun_fixes {
                if text_lower == form
                    && token.pos == PartOfSpeechTag::Noun
                    && token.lemma != expected_lemma
                {
                    token.lemma = expected_lemma.to_string();
                    break;
                }
            }

            if let Some(expected) = contraction_lemma(Language::Portuguese, &text_lower, token.pos)
                && token.lemma != expected
            {
                token.lemma = expected.to_string();
            }
        }
    }
}

/// Korean-specific classifier
struct KoreanClassifier;
impl SentenceClassifier for KoreanClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for (idx, token) in sentence.doc.iter().enumerate() {
            let text = &token.text;
            let lemma = &token.lemma;

            // --- Universal bad signals ---

            if token.pos == PartOfSpeechTag::Space {
                reasons.push(format!("Contains Space token: '{}'", sentence.sentence));
            }

            if token.pos == PartOfSpeechTag::X {
                reasons.push(format!("Token '{text}' has unknown POS (X)"));
            }

            // --- Lemmatization checks ---

            // Verb/Adj/Aux lemmas should end in -다 (dictionary form).
            // Exception: contracted forms use "+" notation (e.g., "것+은").
            if matches!(
                token.pos,
                PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Aux
            ) {
                let is_contraction_lemma = lemma.contains('+');
                if !is_contraction_lemma && !lemma.ends_with('다') {
                    reasons.push(format!(
                        "'{}' ({:?}) has lemma '{}' which doesn't end in -다 — lemma should be the dictionary form",
                        text, token.pos, lemma
                    ));
                }
            }

            // Verb/Adj with surface form as lemma (no lemmatization happened)
            if matches!(
                token.pos,
                PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Aux
            ) && text == lemma
                && !lemma.ends_with('다')
            {
                reasons.push(format!(
                    "'{}' ({:?}) has itself as lemma — should be lemmatized to -다 dictionary form",
                    text, token.pos
                ));
            }

            // Lemmas should not contain "+" for verbs/adjectives.
            // Only contracted noun/pronoun+particle forms use "+".
            if matches!(
                token.pos,
                PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Aux
            ) && lemma.contains('+')
            {
                reasons.push(format!(
                    "'{}' ({:?}) has lemma '{}' with '+' morpheme notation — verb/adj lemmas should be clean -다 dictionary forms",
                    text, token.pos, lemma
                ));
            }

            // --- Proper noun mangling ---

            if token.pos == PartOfSpeechTag::Propn && lemma.contains('+') {
                reasons.push(format!(
                    "Proper noun '{text}' has lemma '{lemma}' with morpheme decomposition — proper nouns should never be decomposed"
                ));
            }

            // Subtitle data often over-classifies common words as proper nouns
            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{text}' classified as a proper noun — subtitle data often over-classifies common words as proper nouns"
                ));
            }

            // --- SCONJ/CCONJ dumping ground detection ---

            // Standalone dictionary forms should be VERB/ADJ, not SCONJ/CCONJ/ADV
            if matches!(token.pos, PartOfSpeechTag::Sconj | PartOfSpeechTag::Cconj) {
                // If the lemma looks like a verb (ends in -다) but is tagged as conjunction
                let base_lemma = lemma.split('+').next().unwrap_or(lemma);
                if base_lemma.ends_with('다') && base_lemma.chars().count() >= 2 {
                    reasons.push(format!(
                        "'{}' (lemma '{}') tagged as {:?} but lemma ends in -다, suggesting it's a verb/adjective. Should likely be VERB or ADJ",
                        text, lemma, token.pos
                    ));
                }

                // Specific known misclassifications
                let sconj_victims = ["있잖아", "얘들아"];
                if sconj_victims.contains(&text.as_str()) {
                    reasons.push(format!(
                        "'{}' tagged as {:?} but this is not a conjunction",
                        text, token.pos
                    ));
                }
            }

            // Dictionary forms tagged as ADV
            if token.pos == PartOfSpeechTag::Adv
                && lemma.ends_with('다')
                && lemma.chars().count() >= 2
            {
                reasons.push(format!(
                    "'{text}' (lemma '{lemma}') tagged as ADV but lemma ends in -다 — likely a VERB or ADJ"
                ));
            }

            // --- Particle / tokenization checks ---

            // Particles that should have been split: noun+particle as single token
            // Check for common unsplit particle patterns
            let unsplit_particle_words = [("너한테", "너", "한테"), ("이것을", "이것", "을")];
            for (surface, _noun, _particle) in &unsplit_particle_words {
                if text.as_str() == *surface {
                    reasons.push(format!(
                        "'{surface}' should be split into two tokens: '{_noun}' + '{_particle}'"
                    ));
                }
            }

            // 나랑 tagged as CCONJ — should be split: 나 + 랑
            if text == "나랑" && token.pos == PartOfSpeechTag::Cconj {
                reasons.push(
                    "'나랑' tagged as CCONJ — should be split: '나' (PRON) + '랑' (ADP)"
                        .to_string(),
                );
            }

            // --- Copula not split ---

            // Detect copula forms that should be split: noun + copula
            // Common pattern: token is VERB with lemma containing +이+ or ending in copula form
            let copula_endings = [
                "입니다",
                "입니까",
                "이에요",
                "이야",
                "이다",
                "이라고",
                "이야",
            ];
            if token.pos == PartOfSpeechTag::Verb {
                // Check if the text ends with a copula form and the lemma has morpheme notation
                for ending in &copula_endings {
                    if text.ends_with(ending) && text.len() > ending.len() {
                        reasons.push(format!(
                            "'{text}' appears to contain copula 이다 — should be split into noun + copula (AUX, lemma '이다')"
                        ));
                        break;
                    }
                }

                // Copula contractions tagged as VERB: 거야, 남자지, etc.
                // These have lemma patterns like X+이+야
                if lemma.contains("+이+") && !lemma.starts_with("것+") {
                    reasons.push(format!(
                        "'{text}' (lemma '{lemma}') contains copula 이다 — should be split into noun/pronoun + copula (AUX)"
                    ));
                }
            }

            // Doubled copula 이 in lemma
            if lemma.contains("+이+이+") || lemma.contains("+이+이다") {
                reasons.push(format!(
                    "'{text}' has lemma '{lemma}' with doubled copula 이 — likely a morpheme boundary error"
                ));
            }

            // --- 하다-adjective checks ---

            let hada_adjectives = [
                "필요하다",
                "건강하다",
                "창피하다",
                "중요하다",
                "유명하다",
                "행복하다",
                "불행하다",
                "위험하다",
                "안전하다",
                "편리하다",
                "불편하다",
                "깨끗하다",
                "복잡하다",
                "간단하다",
                "정확하다",
                "불가능하다",
                "가능하다",
                "충분하다",
                "부족하다",
                "심각하다",
                "급하다",
                "조용하다",
                "시끄럽다",
                "친절하다",
                "불친절하다",
                "성실하다",
                "솔직하다",
                "자유하다",
                "특별하다",
                "평범하다",
                "단단하다",
                "궁금하다",
            ];

            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && hada_adjectives.contains(&lemma.as_str())
            {
                reasons.push(format!(
                        "'{}' (lemma '{}') tagged as {:?} but '{}' is a 하다-adjective (descriptive). Should be ADJ",
                        text, lemma, token.pos, lemma
                    ));
            }

            // --- Specific POS corrections ---

            // 어떻게 is always ADV
            if text == "어떻게" && token.pos != PartOfSpeechTag::Adv {
                reasons.push(format!(
                    "'어떻게' tagged as {:?} but it's an adverb meaning 'how'. Should be ADV",
                    token.pos
                ));
            }

            // 이렇게 is always ADV
            if text == "이렇게" && token.pos != PartOfSpeechTag::Adv {
                reasons.push(format!(
                    "'이렇게' tagged as {:?} but it's an adverb meaning 'like this'. Should be ADV",
                    token.pos
                ));
            }

            // 이걸 should be PRON (contracted 이것+을)
            if text == "이걸" && token.pos != PartOfSpeechTag::Pron {
                reasons.push(format!(
                    "'이걸' tagged as {:?} but it's a pronoun (contracted 이것+을). Should be PRON",
                    token.pos
                ));
            }

            // 그게 should be PRON (contracted 그것+이)
            if text == "그게" && token.pos != PartOfSpeechTag::Pron {
                reasons.push(format!(
                    "'그게' tagged as {:?} but it's a pronoun (contracted 그것+이). Should be PRON",
                    token.pos
                ));
            }

            // 제 before a noun should be DET (possessive "my"), not ADJ or PRON
            if text == "제"
                && token.pos == PartOfSpeechTag::Adj
                && let Some(next) = sentence.doc.get(idx + 1)
                && next.pos == PartOfSpeechTag::Noun
            {
                reasons.push(format!(
                            "'제' before noun '{}' is a possessive determiner ('my'). Should be DET, not ADJ",
                            next.text
                        ));
            }

            // 저 before a noun is demonstrative DET, not PRON
            if text == "저"
                && token.pos == PartOfSpeechTag::Pron
                && let Some(next) = sentence.doc.get(idx + 1)
                && next.pos == PartOfSpeechTag::Noun
            {
                reasons.push(format!(
                            "'저' before noun '{}' is likely a demonstrative determiner ('that'), not a pronoun. Should be DET",
                            next.text
                        ));
            }

            // 봐 with wrong lemma
            if text == "봐" && lemma != "보다" {
                reasons.push(format!(
                    "'봐' has lemma '{lemma}' but should have lemma '보다'"
                ));
            }

            // --- Auxiliary verb checks ---

            if token.pos == PartOfSpeechTag::Aux {
                // Main verbs mistagged as AUX
                let always_main_verbs = [
                    "찾다",
                    "잊다",
                    "넣다",
                    "먹다",
                    "쓰다",
                    "읽다",
                    "듣다",
                    "만들다",
                    "살다",
                    "죽다",
                    "잡다",
                    "놓다",
                    "열다",
                    "닫다",
                    "타다",
                ];
                if always_main_verbs.contains(&lemma.as_str()) {
                    reasons.push(format!(
                        "'{text}' (lemma '{lemma}') tagged as AUX but '{lemma}' is a main verb. Should be VERB"
                    ));
                }

                // 수 is a bound noun, not AUX
                if text == "수" || text == "수가" || text == "수는" || text == "수도" {
                    reasons.push(format!(
                        "'{text}' tagged as AUX but 수 is a bound noun (의존명사). Should be NOUN"
                    ));
                }
            }

            // --- Contraction lemma checks ---

            // 우린 should have lemma "우리+는", not "우+린"
            if text == "우린" && lemma == "우+린" {
                reasons.push(
                    "'우린' has lemma '우+린' but should be '우리+는' (우리 is the base pronoun)"
                        .to_string(),
                );
            }

            // 날 as pronoun contraction (나+를) — check if tagged wrong
            if text == "날" && token.pos == PartOfSpeechTag::Noun {
                // Look at context: if preceded by a verb or followed by a verb, likely 나+를 (PRON)
                let likely_pronoun = if idx > 0 {
                    // After a subject or at sentence start, 날 is often "me"
                    true
                } else {
                    false
                };
                if likely_pronoun {
                    reasons.push(
                        "'날' tagged as NOUN but may be a pronoun contraction (나+를, 'me'). Consider PRON with lemma '나+를'"
                            .to_string(),
                    );
                }
            }

            // --- Foreign name detection ---

            // Names being treated as Korean morphemes
            if token.pos == PartOfSpeechTag::Verb && !lemma.ends_with('다') && !lemma.contains('+')
            {
                // Might be a foreign name misclassified as verb
                let has_only_hangul = text.chars().all(|c| {
                    let code = c as u32;
                    (0xAC00..=0xD7A3).contains(&code)
                        || (0x3131..=0x318E).contains(&code)
                        || (0x1100..=0x11FF).contains(&code)
                });
                if has_only_hangul && text.chars().count() >= 3 {
                    reasons.push(format!(
                        "'{text}' tagged as VERB with lemma '{lemma}' — possible foreign name misclassified as verb"
                    ));
                }
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }
}

/// Korean-specific corrector
struct KoreanCorrector;

impl WordCorrector for KoreanCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let corrections = fix_korean(&mut sentence.doc);
        CorrectionResult {
            corrected: !corrections.is_empty(),
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        fix_korean(tokens);
    }
}

/// English-specific classifier
struct EnglishClassifier;

impl SentenceClassifier for EnglishClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for token in &sentence.doc {
            let text_lower = token.text.to_lowercase();

            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token".to_string());
            }

            // Check for be/have AUX vs VERB disambiguation
            let be_forms = ["am", "is", "are", "was", "were", "be", "been", "being"];

            let have_forms_en = ["have", "has", "had", "having"];

            if be_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "be"
            {
                reasons.push(format!(
                    "'{}' (be) can be either AUX or VERB depending on context. Rule: AUX when forming progressive (e.g., 'is running') or passive (e.g., 'was built'), VERB when used as a copula (e.g., 'she is tall', 'it is late') or existential (e.g., 'there is a problem')",
                    token.text
                ));
            }

            if have_forms_en.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "have"
            {
                reasons.push(format!(
                    "'{}' (have) can be either AUX or VERB depending on context. Rule: AUX when forming perfect tenses (e.g., 'have eaten'), VERB when expressing possession (e.g., 'I have a book') or other meanings (e.g., 'have lunch')",
                    token.text
                ));
            }

            let do_forms = ["do", "does", "did", "doing", "done"];

            let get_forms = ["get", "gets", "got", "gotten", "getting"];

            if do_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "do"
            {
                reasons.push(format!(
                    "'{}' (do) can be either AUX or VERB depending on context. Rule: AUX when used for emphasis (e.g., 'I do like it'), questions (e.g., 'do you know?'), or negation (e.g., 'I don't know'), VERB when meaning to perform/carry out (e.g., 'I did the work')",
                    token.text
                ));
            }

            if get_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "get"
            {
                reasons.push(format!(
                    "'{}' (get) can be either AUX or VERB depending on context. Rule: AUX when forming get-passive (e.g., 'got fired', 'getting married'), VERB when meaning to obtain/receive (e.g., 'I got a letter') or become (e.g., 'it got cold')",
                    token.text
                ));
            }

            // Check for "gon" which is likely from "gonna" being split
            if text_lower == "gon" {
                reasons.push(
                    "Token 'gon' is likely from 'gonna' being incorrectly split into 'gon' + 'na'"
                        .to_string(),
                );
            }

            // Check for "wan" which is likely from "wanna" being split
            if text_lower == "wan" {
                reasons.push(
                    "Token 'wan' is likely from 'wanna' being incorrectly split into 'wan' + 'na'"
                        .to_string(),
                );
            }

            // Check for "got" followed by "ta" (gotta)
            if text_lower == "ta" {
                reasons
                    .push("Token 'ta' might be from 'gotta' being incorrectly split".to_string());
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            if token.lemma == "be" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (be) is tagged AUX but is followed by adjective '{next_text}' — if this is a copula (e.g., 'she is tall'), it should be VERB, not AUX. 'be' is only AUX when forming progressive (e.g., 'is running') or passive (e.g., 'was built').",
                        token.text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (be) is tagged AUX — please double-check: it should be VERB when used as a copula (e.g., 'she is tall', 'it is late') or existential (e.g., 'there is a problem'), and only AUX when forming progressive or passive with a verb.",
                        token.text
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// English-specific corrector
struct EnglishCorrector;

impl WordCorrector for EnglishCorrector {
    fn passthrough(&self) -> bool {
        true
    }

    fn correct(&self, _sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        CorrectionResult {
            corrected: false,
            corrections: vec![],
        }
    }
}

/// French-specific classifier
struct FrenchClassifier;

impl SentenceClassifier for FrenchClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        // Check for Space tokens which indicate NLP parsing issues
        for (idx, token) in sentence.doc.iter().enumerate() {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token, which is usually not necessary due to the `whitespace` field".to_string());
            }
            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun, but the legacy NLP pipeline often over-classifies things as proper nouns",
                    token.text
                ));
            }

            let text_lower = token.text.to_lowercase();

            // Check for hyphen being parsed incorrectly (indicates parsing error)
            if text_lower == "-"
                && (token.pos == PartOfSpeechTag::Pron || token.pos == PartOfSpeechTag::X)
            {
                reasons.push(format!("Hyphen parsed as {:?}", token.pos));
            }

            // Check for "lui" pronoun with lemma "luire"
            if text_lower == "lui" && token.lemma == "luire" {
                reasons
                    .push("'lui' has lemma 'luire' - is that right in this context?".to_string());
            }

            // Check for "eux" with lemma "lui"
            if text_lower == "eux" && token.lemma == "lui" {
                reasons.push("'eux' has lemma 'lui'".to_string());
            }

            // Check for words that can be either DET or PRON depending on context
            // Rule: If it modifies a noun directly → DET. If it stands alone replacing a noun → PRON.
            let det_or_pron_words = [
                // Quantifiers/Indefinites that can be both
                "tout",
                "toute",
                "tous",
                "toutes",
                "certain",
                "certains",
                "certaine",
                "certaines",
                "aucun",
                "aucune",
                "plusieurs",
                "autre",
                "autres",
                "même",
                "mêmes",
                "tel",
                "telle",
                "tels",
                "telles",
                "chacun",
                "chacune",
                // Articles (can sometimes be pronouns in certain constructions)
                "le",
                "la",
                "les",
                "l'",
            ];

            if det_or_pron_words.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (Rule: modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // Check common past-tense verbs are lemmatized to infinitive
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let expected_lemmas: Vec<(&str, &str)> = vec![
                    ("était", "être"),
                    ("étaient", "être"),
                    ("avait", "avoir"),
                    ("avaient", "avoir"),
                    ("faisait", "faire"),
                    ("faisaient", "faire"),
                    ("disait", "dire"),
                    ("disaient", "dire"),
                    ("allait", "aller"),
                    ("allaient", "aller"),
                    ("venait", "venir"),
                    ("venaient", "venir"),
                    ("voyait", "voir"),
                    ("voyaient", "voir"),
                    ("pouvait", "pouvoir"),
                    ("pouvaient", "pouvoir"),
                    ("voulait", "vouloir"),
                    ("voulaient", "vouloir"),
                    ("savait", "savoir"),
                    ("savaient", "savoir"),
                ];

                for (past_form, expected_infinitive) in expected_lemmas {
                    if text_lower == past_form && token.lemma != expected_infinitive {
                        reasons.push(format!(
                            "Past-tense verb '{}' has lemma '{}', but the dictionary form is '{}', look at the context to determine which is rigbt",
                            token.text, token.lemma, expected_infinitive
                        ));
                    }
                }
            }

            // Check for avoir conjugations which can be either AUX or VERB depending on context
            // Rule: AUX when forming compound tenses with past participles (e.g., "j'ai mangé")
            //       VERB when expressing possession or other meanings (e.g., "j'ai un livre", "il a faim")
            let avoir_forms = [
                // Present
                "ai", "as", "a", "avons", "avez", "ont", // Imperfect
                "avais", "avait", "avions", "aviez", "avaient", // Future
                "aurai", "auras", "aura", "aurons", "aurez", "auront", // Conditional
                "aurais", "aurait", "aurions", "auriez", "auraient", // Passé simple
                "eus", "eut", "eûmes", "eûtes", "eurent",
            ];

            let devoir_forms = [
                // Present
                "dois",
                "doit",
                "devons",
                "devez",
                "doivent", // Imperfect
                "devais",
                "devait",
                "devions",
                "deviez",
                "devaient", // Future
                "devrai",
                "devras",
                "devra",
                "devrons",
                "devrez",
                "devront", // Conditional
                "devrais",
                "devrait",
                "devrions",
                "devriez",
                "devraient", // Passé simple
                "dus",
                "dut",
                "dûmes",
                "dûtes",
                "durent",
            ];

            let pouvoir_forms = [
                // Present
                "peux",
                "peut",
                "pouvons",
                "pouvez",
                "peuvent", // Imperfect
                "pouvais",
                "pouvait",
                "pouvions",
                "pouviez",
                "pouvaient", // Future
                "pourrai",
                "pourras",
                "pourra",
                "pourrons",
                "pourrez",
                "pourront", // Conditional
                "pourrais",
                "pourrait",
                "pourrions",
                "pourriez",
                "pourraient", // Passé simple
                "pus",
                "put",
                "pûmes",
                "pûtes",
                "purent",
            ];

            let savoir_forms = [
                // Present
                "sais",
                "sait",
                "savons",
                "savez",
                "savent", // Imperfect
                "savais",
                "savait",
                "savions",
                "saviez",
                "savaient", // Future
                "saurai",
                "sauras",
                "saura",
                "saurons",
                "saurez",
                "sauront", // Conditional
                "saurais",
                "saurait",
                "saurions",
                "sauriez",
                "sauraient", // Passé simple
                "sus",
                "sut",
                "sûmes",
                "sûtes",
                "surent",
            ];

            let etre_forms = [
                // Present
                "suis", "es", "est", "sommes", "êtes", "sont", // Imperfect
                "étais", "était", "étions", "étiez", "étaient", // Future
                "serai", "seras", "sera", "serons", "serez", "seront", // Conditional
                "serais", "serait", "serions", "seriez", "seraient", // Passé simple
                "fus", "fut", "fûmes", "fûtes", "furent",
            ];

            let falloir_forms = [
                // Present
                "faut",     // Imperfect
                "fallait",  // Future
                "faudra",   // Conditional
                "faudrait", // Passé simple
                "fallut",
            ];

            if avoir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "avoir"
            {
                reasons.push(format!(
                    "'{}' (avoir) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses with past participles (e.g., 'j'ai mangé'), VERB when expressing possession or other meanings (e.g., 'j'ai un livre', 'il a faim', 'on n'a pas beaucoup de temps', etc.)",
                    token.text
                ));
            }

            if devoir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "devoir"
            {
                reasons.push(format!(
                    "'{}' (devoir) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation/necessity with infinitive (e.g., 'je dois partir'), VERB when used standalone or with other complements (e.g., 'il me doit de l'argent')",
                    token.text
                ));
            }

            if pouvoir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "pouvoir"
            {
                reasons.push(format!(
                    "'{}' (pouvoir) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'je peux venir'), VERB when used standalone or as a noun",
                    token.text
                ));
            }

            if savoir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "savoir"
            {
                reasons.push(format!(
                    "'{}' (savoir) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/knowledge with infinitive (e.g., 'je sais nager'), VERB when expressing knowledge of facts (e.g., 'je sais la réponse')",
                    token.text
                ));
            }

            if etre_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "être"
            {
                reasons.push(format!(
                    "'{}' (être) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses with past participles (e.g., 'elle est partie'), VERB when used as a copula or existential verb (e.g., 'elle est belle', 'il est tard', 'c'est vrai')",
                    token.text
                ));

                // Nudge when être is tagged AUX — remind the model to double-check
                // whether it's really a compound tense or actually a copula.
                if token.pos == PartOfSpeechTag::Aux {
                    let next = sentence.doc.get(idx + 1);
                    let next_pos = next.map(|n| n.pos);
                    let next_text = next.map(|n| n.text.as_str()).unwrap_or("");
                    if next_pos == Some(PartOfSpeechTag::Adj) {
                        reasons.push(format!(
                            "'{}' is tagged AUX and followed by '{}' (tagged ADJ) — please double-check: if this is être + adjective (e.g., 'elle est belle'), it should be VERB (copula), not AUX. être is only AUX in compound tenses with past participles like 'elle est partie'",
                            token.text, next_text
                        ));
                    } else if next_pos != Some(PartOfSpeechTag::Verb) {
                        reasons.push(format!(
                            "'{}' is tagged AUX — please double-check whether this is really a compound tense (AUX) or a copula (VERB). Remember: 'c'est vrai' → VERB, 'il est tard' → VERB, 'c'est tout' → VERB",
                            token.text
                        ));
                    }
                }
            }

            if falloir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "falloir"
            {
                reasons.push(format!(
                    "'{}' (falloir) can be either AUX or VERB depending on context. Rule: AUX when expressing necessity with infinitive (e.g., 'il faut partir'), VERB when used with noun complements (e.g., 'il faut du temps')",
                    token.text
                ));
            }

            let aller_forms = [
                // Present
                "vais",
                "vas",
                "va",
                "allons",
                "allez",
                "vont", // Imperfect
                "allais",
                "allait",
                "allions",
                "alliez",
                "allaient", // Passé simple
                "allai",
                "allas",
                "alla",
                "allâmes",
                "allâtes",
                "allèrent", // Future
                "irai",
                "iras",
                "ira",
                "irons",
                "irez",
                "iront", // Conditional
                "irais",
                "irait",
                "irions",
                "iriez",
                "iraient",
            ];

            let venir_forms = [
                // Present
                "viens",
                "vient",
                "venons",
                "venez",
                "viennent", // Imperfect
                "venais",
                "venait",
                "venions",
                "veniez",
                "venaient", // Passé simple
                "vins",
                "vint",
                "vînmes",
                "vîntes",
                "vinrent", // Future
                "viendrai",
                "viendras",
                "viendra",
                "viendrons",
                "viendrez",
                "viendront", // Conditional
                "viendrais",
                "viendrait",
                "viendrions",
                "viendriez",
                "viendraient",
            ];

            let faire_forms = [
                // Present
                "fais",
                "fait",
                "faisons",
                "faites",
                "font", // Imperfect
                "faisais",
                "faisait",
                "faisions",
                "faisiez",
                "faisaient", // Passé simple
                "fis",
                "fit",
                "fîmes",
                "fîtes",
                "firent", // Future
                "ferai",
                "feras",
                "fera",
                "ferons",
                "ferez",
                "feront", // Conditional
                "ferais",
                "ferait",
                "ferions",
                "feriez",
                "feraient",
            ];

            let laisser_forms = [
                // Present
                "laisse",
                "laisses",
                "laissons",
                "laissez",
                "laissent", // Imperfect
                "laissais",
                "laissait",
                "laissions",
                "laissiez",
                "laissaient", // Passé simple
                "laissai",
                "laissas",
                "laissa",
                "laissâmes",
                "laissâtes",
                "laissèrent", // Future
                "laisserai",
                "laisseras",
                "laissera",
                "laisserons",
                "laisserez",
                "laisseront", // Conditional
                "laisserais",
                "laisserait",
                "laisserions",
                "laisseriez",
                "laisseraient",
            ];

            if aller_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "aller"
            {
                reasons.push(format!(
                    "'{}' (aller) can be either AUX or VERB depending on context. Rule: AUX when forming near future with infinitive (e.g., 'je vais manger'), VERB when expressing movement (e.g., 'je vais à Paris', 'comment allez-vous?')",
                    token.text
                ));
            }

            if venir_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "venir"
            {
                reasons.push(format!(
                    "'{}' (venir) can be either AUX or VERB depending on context. Rule: AUX when forming recent past with 'de' + infinitive (e.g., 'il vient de manger'), VERB when expressing coming/arriving (e.g., 'il vient demain')",
                    token.text
                ));
            }

            if faire_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "faire"
            {
                reasons.push(format!(
                    "'{}' (faire) can be either AUX or VERB depending on context. Rule: AUX when used as causative with infinitive (e.g., 'je fais réparer la voiture'), VERB when meaning to do/make (e.g., 'je fais un gâteau', 'il fait beau')",
                    token.text
                ));
            }

            if laisser_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "laisser"
            {
                reasons.push(format!(
                    "'{}' (laisser) can be either AUX or VERB depending on context. Rule: AUX when used as semi-auxiliary with infinitive (e.g., 'laisse-moi parler'), VERB when meaning to leave/let go (e.g., 'laisse-moi tranquille')",
                    token.text
                ));
            }

            if text_lower == "bois" {
                reasons.push(format!(
                    "'bois' can be: (1) Verb 'boire' (e.g., 'Je bois du café') → lemma 'boire', OR (2) Noun 'bois' (e.g., 'Le bois est dur') → lemma 'bois'. Current lemma: '{}'.",
                    token.lemma
                ));
            }

            if text_lower == "puis" {
                reasons.push(format!(
                    "'puis' can be: (1) Verb 'pouvoir' (1st person singular passé simple, meaning 'could') → lemma 'pouvoir', POS VERB (or POS AUX if it's functioning as an auxiliary verb), OR (2) Adverb meaning 'then' (e.g., 'Et puis il est parti') → lemma 'puis', POS ADV. Current lemma (double check this): '{}', POS: {:?}.",
                    token.lemma, token.pos
                ));
            }

            if text_lower == "passé" {
                reasons.push(format!(
                    "'passé' can be: (1) Past participle/Adjective from 'passer' (e.g., 'Le temps passé') → lemma 'passer', POS VERB/ADJ, OR (2) Preposition meaning 'past/beyond' (e.g., 'passé minuit') → lemma 'passé', POS ADP. Current lemma: '{}', POS: {:?}.",
                    token.lemma, token.pos
                ));
            }

            if text_lower == "soit" {
                reasons.push(format!(
                    "'soit' can be: (1) Subjunctive verb from 'être' (e.g., 'il faut qu'il soit', 'quoi qu'il en soit') → lemma 'être', POS VERB, (2) Jussive subjunctive, still a verb (e.g., 'soit x un nombre') → lemma 'être', POS VERB, (3) Coordinating conjunction in 'soit... soit...' constructions → lemma 'soit', POS CCONJ, OR (4) Explanatory adverb meaning 'that is'/'namely' (e.g., 'soit dix euros') → lemma 'soit', POS ADV. Current lemma: '{}', POS: {:?}.",
                    token.lemma, token.pos
                ));
            }

            // Check for "s'" which can be either "se" (reflexive) or "si" (conjunction)
            // In "s'il te plaît", "s'il vous plaît", "s'il y a", etc., s' = si (if), not se (reflexive)
            if text_lower == "s'" || text_lower == "s\u{2019}" {
                // Look ahead for "il" to detect "s'il" constructions
                let next_text = sentence
                    .doc
                    .get(idx + 1)
                    .map(|t| t.text.to_lowercase())
                    .unwrap_or_default();
                if next_text == "il" || next_text == "ils" {
                    if token.lemma != "si" || token.pos != PartOfSpeechTag::Sconj {
                        reasons.push(format!(
                            "'s'' before '{}' is a contraction of 'si' (if), not 'se' (reflexive). Current lemma: '{}', POS: {:?}. Should be lemma 'si', POS SCONJ.",
                            next_text, token.lemma, token.pos
                        ));
                    }
                } else {
                    // Before other words, s' is typically the reflexive pronoun "se"
                    if token.lemma != "se" {
                        reasons.push(format!(
                            "'s'' has lemma '{}' but is likely the reflexive pronoun 'se'. Current POS: {:?}.",
                            token.lemma, token.pos
                        ));
                    }
                }
            }

            // Check for "là" mistagged as Noun — it's almost always an adverb
            // "là" = "there" (demonstrative/locative adverb), e.g., "là-bas", "celui-là", "ce jour-là"
            if text_lower == "là"
                && token.pos != PartOfSpeechTag::Adv
                && token.pos != PartOfSpeechTag::Punct
            {
                reasons.push(format!(
                    "'là' tagged as {:?} with lemma '{}', but 'là' is almost always an adverb meaning 'there'. Should be POS ADV, lemma 'là'.",
                    token.pos, token.lemma
                ));
            }

            // Check for reflexive pronouns in lemma (should be separated)
            if token.lemma.starts_with("s'") || token.lemma.starts_with("se ") {
                reasons.push(format!(
                    "'{}' has lemma '{}' which contains reflexive pronoun - the lemma should generally just be one word without indicating the reflexive pronoun.",
                    token.text, token.lemma
                ));
            }

            // Check for lemmas that look like conjugated forms rather than infinitives
            // Common patterns: past participles ending in é/ée/és/ées, imperfect forms ending in -ait
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                if lemma_lower.ends_with("ait")
                    || lemma_lower.ends_with("aient")
                    || (lemma_lower.ends_with("é")
                        && !lemma_lower.ends_with("er")
                        && lemma_lower != "été")
                    || (lemma_lower.ends_with("ée")
                        || lemma_lower.ends_with("és")
                        || lemma_lower.ends_with("ées"))
                {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which looks like a conjugated form rather than an infinitive",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for non-verb words being lemmatized as verbs (common error)
            if token.pos != PartOfSpeechTag::Verb && token.pos != PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                // French infinitives end in -er, -ir, -re, -oir
                if lemma_lower.ends_with("er")
                    || lemma_lower.ends_with("ir")
                    || lemma_lower.ends_with("re")
                    || lemma_lower.ends_with("oir")
                {
                    // This might be a verb lemma for a non-verb token
                    reasons.push(format!(
                        "'{}' (POS: {:?}) has verb-like lemma '{}' - verify this is correct",
                        token.text, token.pos, token.lemma
                    ));
                }
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::French, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // Check être tagged AUX — might be a copula that should be VERB
            if token.lemma == "être" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                // If followed by adjective, very likely copula
                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (être) is tagged AUX but is followed by adjective '{}' — if this is a copula (être + adjective describing a state/quality), it should be VERB, not AUX. être is only AUX in compound tenses like 'elle est partie'.",
                        token.text, next_text
                    ));
                }
                // If not followed by a verb (i.e., not a compound tense), also suspicious
                else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (être) is tagged AUX but the next word '{}' ({:?}) doesn't look like a past participle — please double-check whether this is really a compound tense (AUX) or a copula (VERB). Remember: 'c'est vrai' → VERB, 'c'est tout' → VERB.",
                        token.text,
                        next_text,
                        next_pos.unwrap_or(PartOfSpeechTag::X)
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// German-specific classifier
struct GermanClassifier;

impl SentenceClassifier for GermanClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        // Check for Space tokens which indicate NLP parsing issues
        for (idx, token) in sentence.doc.iter().enumerate() {
            let is_first_word = idx == 0;
            let _is_last_word = idx == sentence.doc.len() - 1;

            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains SPACE token, but the `whitespace` field should be used instead (SPACE tokens are not usually necessary)".to_string());
            }
            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun, but the legacy NLP pipeline often over-classifies things as proper nouns",
                    token.text
                ));
            }

            if is_first_word && token.text == "Sie" {
                reasons.push(
                    "Sie could either have lemma 'Sie' (formal you) or 'sie' (she/they)"
                        .to_string(),
                );
            }

            let text_lower = token.text.to_lowercase();

            // Check for "will" which is often miscategorized
            // In German, "will" is a form of "wollen" (to want), but often gets confused
            if text_lower == "will" {
                reasons.push(
                    "Contains 'will' which is often miscategorized as it has multiple meanings ('werden', 'wollen', the name, etc)"
                        .to_string(),
                );
            }

            // Check for words that can be either DET or PRON depending on context
            // Rule: If it modifies a noun directly → DET. If it stands alone replacing a noun → PRON.
            let det_or_pron_words = [
                // Possessives
                "mein",
                "meine",
                "meinen",
                "meinem",
                "meiner",
                "meines",
                "dein",
                "deine",
                "deinen",
                "deinem",
                "deiner",
                "deines",
                "deins",
                "sein",
                "seine",
                "seinen",
                "seinem",
                "seiner",
                "seines",
                "seins",
                "ihr",
                "ihre",
                "ihren",
                "ihrem",
                "ihrer",
                "ihres",
                "unser",
                "unsere",
                "unseren",
                "unserem",
                "unserer",
                "unseres",
                "unsres",
                "euer",
                "eure",
                "euren",
                "eurem",
                "eurer",
                "eures",
                "eurer",
                // Demonstratives
                "dieser",
                "diese",
                "dieses",
                "diesen",
                "diesem",
                "dieser",
                "jener",
                "jene",
                "jenes",
                "jenen",
                "jenem",
                "jener",
                "derselbe",
                "dieselbe",
                "dasselbe",
                "denselben",
                "demselben",
                "derselben",
                // Indefinites
                "einer",
                "eine",
                "eines",
                "einen",
                "einem",
                "keiner",
                "keine",
                "keines",
                "keinen",
                "keinem",
                // Quantifiers
                "alle",
                "aller",
                "allen",
                "allem",
                "beide",
                "beider",
                "beiden",
                "beidem",
                "einige",
                "einiger",
                "einigen",
                "einigem",
                "mehrere",
                "mehrerer",
                "mehreren",
                "mehrerem",
                "viele",
                "vieler",
                "vielen",
                "vielem",
                "wenige",
                "weniger",
                "wenigen",
                "wenigem",
                // Definite articles that can be relative/demonstrative pronouns
                "der",
                "die",
                "das",
                "den",
                "dem",
                "des",
            ];

            if det_or_pron_words.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (Rule: modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // Check for reflexive pronouns with lemma "sich"
            if (text_lower == "mich" || text_lower == "dich")
                && token.lemma == "sich"
                && token.pos == PartOfSpeechTag::Pron
            {
                reasons.push(format!("'{}' has lemma 'sich'", token.text));
            }

            // Check for "den" article with incorrect lemma "die"
            // Could be wrong (should be "der" for masc. acc.) or correct (dative plural)
            if text_lower == "den" && token.lemma == "die" && token.pos == PartOfSpeechTag::Det {
                reasons.push(
                    "'den' has lemma 'die' (could be wrong if accusative masculine)".to_string(),
                );
            }

            // Check for words that should be pronouns but are tagged as nouns
            // Common indefinite pronouns: alles, jemand, jemanden, jemandem, niemand, etc.
            if token.pos == PartOfSpeechTag::Noun {
                let indefinite_pronouns = [
                    "alles",
                    "etwas",
                    "nichts",
                    "jemand",
                    "jemanden",
                    "jemandem",
                    "jemands",
                    "niemand",
                    "niemanden",
                    "niemandem",
                    "niemands",
                ];
                if indefinite_pronouns.contains(&text_lower.as_str()) {
                    reasons.push(format!(
                        "'{}' tagged as NOUN but should likely be PRON",
                        token.text
                    ));
                }
            }

            // Check for capitalized lemma on non-nouns (nouns are capitalized in German)
            if token.pos != PartOfSpeechTag::Noun
                && token.pos != PartOfSpeechTag::Propn
                && token.pos != PartOfSpeechTag::Punct
                && let Some(first_char) = token.lemma.chars().next()
                && first_char.is_uppercase()
            {
                reasons.push(format!(
                    "Non-noun '{}' has capitalized lemma '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for nouns with lowercase lemmas (nouns are capitalized in German)
            if (token.pos == PartOfSpeechTag::Noun || token.pos == PartOfSpeechTag::Propn)
                && let Some(first_char) = token.lemma.chars().next()
                && first_char.is_lowercase()
            {
                reasons.push(format!(
                    "Noun '{}' has lowercase lemma '{}'",
                    token.text, token.lemma
                ));
            }

            // Check common past-tense verbs are lemmatized to infinitive
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let expected_lemmas: Vec<(&str, &str)> = vec![
                    ("war", "sein"),
                    ("waren", "sein"),
                    ("hatte", "haben"),
                    ("hatten", "haben"),
                    ("machte", "machen"),
                    ("machten", "machen"),
                    ("sagte", "sagen"),
                    ("sagten", "sagen"),
                    ("ging", "gehen"),
                    ("gingen", "gehen"),
                    ("kam", "kommen"),
                    ("kamen", "kommen"),
                    ("sah", "sehen"),
                    ("sahen", "sehen"),
                    ("konnte", "können"),
                    ("konnten", "können"),
                    ("wollte", "wollen"),
                    ("wollten", "wollen"),
                    ("wusste", "wissen"),
                    ("wussten", "wissen"),
                ];

                for (past_form, expected_infinitive) in expected_lemmas {
                    if text_lower == past_form && token.lemma != expected_infinitive {
                        reasons.push(format!(
                            "Past-tense verb '{}' has lemma '{}', but the dictionary form is '{}', look at the context to determine which is rigbt",
                            token.text, token.lemma, expected_infinitive
                        ));
                    }
                }
            }

            // Check for haben conjugations which can be either AUX or VERB depending on context
            // Rule: AUX when forming compound tenses with past participles (e.g., "ich habe gegessen")
            //       VERB when expressing possession or other meanings (e.g., "ich habe Zeit")
            let haben_forms = [
                // Present
                "habe",
                "hast",
                "hat",
                "haben",
                "habt", // Past
                "hatte",
                "hattest",
                "hatten",
                "hattet", // Future
                "werde haben",
                "wirst haben",
                "wird haben",
                "werden haben",
                "werdet haben",
            ];

            let müssen_forms = [
                // Present
                "muss", "musst", "müssen", "müsst", // Past
                "musste", "musstest", "mussten", "musstet",
            ];

            let können_forms = [
                // Present
                "kann", "kannst", "können", "könnt", // Past
                "konnte", "konntest", "konnten", "konntet",
            ];

            let wissen_forms = [
                // Present
                "weiß", "weißt", "wissen", "wisst", // Past
                "wusste", "wusstest", "wussten", "wusstet",
            ];

            let sollen_forms = [
                // Present
                "soll", "sollst", "sollen", "sollt", // Past
                "sollte", "solltest", "sollten", "solltet",
            ];

            let wollen_forms = [
                // Present
                "will", "willst", "wollen", "wollt", // Past
                "wollte", "wolltest", "wollten", "wolltet",
            ];

            let dürfen_forms = [
                // Present
                "darf", "darfst", "dürfen", "dürft", // Past
                "durfte", "durftest", "durften", "durftet",
            ];

            let mögen_forms = [
                // Present
                "mag",
                "magst",
                "mögen",
                "mögt", // Past (including möchte)
                "mochte",
                "mochtest",
                "mochten",
                "mochtet",
                "möchte",
                "möchtest",
                "möchten",
                "möchtet",
            ];

            let sein_forms = [
                // Present
                "bin", "bist", "ist", "sind", "seid", // Past
                "war", "warst", "waren", "wart",
            ];

            let werden_forms = [
                // Present
                "werde", "wirst", "wird", "werden", "werdet", // Past
                "wurde", "wurdest", "wurden", "wurdet", "ward", // archaic past
            ];

            if sein_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "sein"
            {
                reasons.push(format!(
                    "'{}' (sein) can be either AUX or VERB depending on context. Rule: AUX when forming perfect tenses with past participles (e.g., 'ich bin gegangen'), VERB when used as a copula expressing identity/state (e.g., 'er ist groß', 'ich bin müde')",
                    token.text
                ));
            }

            if werden_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "werden"
            {
                reasons.push(format!(
                    "'{}' (werden) can be either AUX or VERB depending on context. Rule: AUX when forming future tense (e.g., 'ich werde gehen') or passive voice (e.g., 'es wird gemacht'), VERB when meaning 'to become' (e.g., 'er wird alt', 'es wird kalt')",
                    token.text
                ));
            }

            if haben_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "haben"
            {
                reasons.push(format!(
                    "'{}' (haben) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses with past participles (e.g., 'ich habe gegessen'), VERB when expressing possession or other meanings (e.g., 'ich habe Zeit', 'er hat Hunger')",
                    token.text
                ));
            }

            if müssen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "müssen"
            {
                reasons.push(format!(
                    "'{}' (müssen) can be either AUX or VERB depending on context. Rule: AUX when expressing necessity/obligation with infinitive (e.g., 'ich muss gehen'), VERB when used standalone",
                    token.text
                ));
            }

            if können_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "können"
            {
                reasons.push(format!(
                    "'{}' (können) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'ich kann schwimmen'), VERB when used standalone",
                    token.text
                ));
            }

            if wissen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "wissen"
            {
                reasons.push(format!(
                    "'{}' (wissen) can be either AUX or VERB depending on context. Rule: Usually VERB expressing knowledge (e.g., 'ich weiß es'), but can be AUX in some constructions",
                    token.text
                ));
            }

            if sollen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "sollen"
            {
                reasons.push(format!(
                    "'{}' (sollen) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation/expectation with infinitive (e.g., 'du sollst gehen'), VERB when used standalone",
                    token.text
                ));
            }

            if wollen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "wollen"
            {
                reasons.push(format!(
                    "'{}' (wollen) can be either AUX or VERB depending on context. Rule: AUX when expressing desire/intention with infinitive (e.g., 'ich will gehen'), VERB when used standalone",
                    token.text
                ));
            }

            if dürfen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "dürfen"
            {
                reasons.push(format!(
                    "'{}' (dürfen) can be either AUX or VERB depending on context. Rule: AUX when expressing permission/allowance with infinitive (e.g., 'du darfst gehen'), VERB when used standalone",
                    token.text
                ));
            }

            if mögen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "mögen"
            {
                reasons.push(format!(
                    "'{}' (mögen) can be either AUX or VERB depending on context. Rule: AUX when expressing desire with infinitive (e.g., 'ich möchte gehen'), VERB when expressing liking (e.g., 'ich mag Pizza')",
                    token.text
                ));
            }

            let lassen_forms = [
                // Present
                "lasse", "lässt", "lässt", "lassen", "lasst", // Past
                "ließ", "ließt", "ließen", "ließt",
            ];

            if lassen_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "lassen"
            {
                reasons.push(format!(
                    "'{}' (lassen) can be either AUX or VERB depending on context. Rule: AUX when used as causative with infinitive (e.g., 'ich lasse ihn arbeiten'), VERB when meaning to leave/let go (e.g., 'lass das!')",
                    token.text
                ));
            }

            // Check for modal/auxiliary verbs where the lemma is the inflected form itself
            // instead of the proper infinitive. This catches errors like magst→magen, willst→willen,
            // kannst→kannst, musst→musst, etc.
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let modal_forms_to_infinitive: &[(&[&str], &str)] = &[
                    (
                        &[
                            "mag",
                            "magst",
                            "mögt",
                            "mochte",
                            "mochtest",
                            "mochten",
                            "mochtet",
                            "möchte",
                            "möchtest",
                            "möchten",
                            "möchtet",
                        ],
                        "mögen",
                    ),
                    (
                        &[
                            "will", "willst", "wollt", "wollte", "wolltest", "wollten", "wolltet",
                        ],
                        "wollen",
                    ),
                    (
                        &[
                            "kann", "kannst", "könnt", "konnte", "konntest", "konnten", "konntet",
                        ],
                        "können",
                    ),
                    (
                        &[
                            "muss", "musst", "müsst", "musste", "musstest", "mussten", "musstet",
                        ],
                        "müssen",
                    ),
                    (
                        &[
                            "soll", "sollst", "sollt", "sollte", "solltest", "sollten", "solltet",
                        ],
                        "sollen",
                    ),
                    (
                        &[
                            "darf", "darfst", "dürft", "durfte", "durftest", "durften", "durftet",
                        ],
                        "dürfen",
                    ),
                    (
                        &[
                            "bin", "bist", "ist", "sind", "seid", "war", "warst", "waren", "wart",
                        ],
                        "sein",
                    ),
                    (
                        &[
                            "habe", "hast", "hat", "habt", "hatte", "hattest", "hatten", "hattet",
                        ],
                        "haben",
                    ),
                    (
                        &[
                            "werde", "wirst", "wird", "werdet", "wurde", "wurdest", "wurden",
                            "wurdet",
                        ],
                        "werden",
                    ),
                    (
                        &["lasse", "lässt", "lasst", "ließ", "ließt", "ließen"],
                        "lassen",
                    ),
                ];
                for (forms, expected_infinitive) in modal_forms_to_infinitive {
                    if forms.contains(&text_lower.as_str()) && token.lemma != *expected_infinitive {
                        reasons.push(format!(
                            "Verb/Aux '{}' has lemma '{}' but should likely be '{}'. Modal and auxiliary verbs must be lemmatized to their dictionary infinitive form.",
                            token.text, token.lemma, expected_infinitive
                        ));
                    }
                }

                // Check for verbs where the lemma equals the inflected form (no lemmatization happened)
                // German infinitives end in -en, -ern, -eln, or -n. If the lemma doesn't end in
                // one of these, it's likely the inflected form was kept as-is.
                let lemma_lower = token.lemma.to_lowercase();
                if lemma_lower == text_lower
                    && !lemma_lower.ends_with("en")
                    && !lemma_lower.ends_with("ern")
                    && !lemma_lower.ends_with("eln")
                    && !lemma_lower.ends_with('n')
                    && lemma_lower.len() > 2
                {
                    reasons.push(format!(
                        "Verb/Aux '{}' has itself as lemma '{}' which doesn't look like a German infinitive (should end in -en/-ern/-eln/-n). The lemma should be the infinitive form.",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for common irregular verb lemmatization errors
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let irregular_forms: &[(&str, &str)] = &[
                    ("biss", "beißen"),
                    ("bissen", "beißen"),
                    ("lief", "laufen"),
                    ("liefen", "laufen"),
                    ("fiel", "fallen"),
                    ("fielen", "fallen"),
                    ("hielt", "halten"),
                    ("hielten", "halten"),
                    ("rief", "rufen"),
                    ("riefen", "rufen"),
                    ("schlief", "schlafen"),
                    ("schliefen", "schlafen"),
                    ("trug", "tragen"),
                    ("trugen", "tragen"),
                    ("fand", "finden"),
                    ("fanden", "finden"),
                    ("gab", "geben"),
                    ("gaben", "geben"),
                    ("nahm", "nehmen"),
                    ("nahmen", "nehmen"),
                    ("sprach", "sprechen"),
                    ("sprachen", "sprechen"),
                    ("trank", "trinken"),
                    ("tranken", "trinken"),
                    ("aß", "essen"),
                    ("aßen", "essen"),
                    ("las", "lesen"),
                    ("lasen", "lesen"),
                    ("saß", "sitzen"),
                    ("saßen", "sitzen"),
                    ("stand", "stehen"),
                    ("standen", "stehen"),
                    ("lag", "liegen"),
                    ("lagen", "liegen"),
                    ("schlug", "schlagen"),
                    ("schlugen", "schlagen"),
                    ("fuhr", "fahren"),
                    ("fuhren", "fahren"),
                    ("schrieb", "schreiben"),
                    ("schrieben", "schreiben"),
                    ("schwamm", "schwimmen"),
                    ("schwammen", "schwimmen"),
                    ("begann", "beginnen"),
                    ("begannen", "beginnen"),
                    ("gewann", "gewinnen"),
                    ("gewannen", "gewinnen"),
                    ("vergaß", "vergessen"),
                    ("vergaßen", "vergessen"),
                    ("verließ", "verlassen"),
                    ("verließen", "verlassen"),
                ];
                for (form, expected) in irregular_forms {
                    if text_lower == *form && token.lemma != *expected {
                        reasons.push(format!(
                            "Irregular verb '{}' has lemma '{}' but should be '{}'",
                            token.text, token.lemma, expected
                        ));
                    }
                }
            }

            // Check for predicative adjectives mistagged as ADV
            // In German, adjectives in predicative position (after sein/werden/bleiben)
            // are often mistagged as ADV. e.g. "Er ist reich" → reich should be ADJ not ADV
            if token.pos == PartOfSpeechTag::Adv {
                let common_predicative_adjs = [
                    "reich",
                    "arm",
                    "alt",
                    "jung",
                    "groß",
                    "klein",
                    "gut",
                    "schlecht",
                    "schön",
                    "hässlich",
                    "schnell",
                    "langsam",
                    "wütend",
                    "traurig",
                    "glücklich",
                    "müde",
                    "krank",
                    "gesund",
                    "leer",
                    "voll",
                    "warm",
                    "kalt",
                    "heiß",
                    "nass",
                    "trocken",
                    "sauber",
                    "schmutzig",
                    "stark",
                    "schwach",
                    "laut",
                    "leise",
                    "hell",
                    "dunkel",
                    "neu",
                    "fertig",
                    "bereit",
                    "sicher",
                    "gefährlich",
                    "möglich",
                    "unmöglich",
                    "nötig",
                    "wichtig",
                    "richtig",
                    "falsch",
                    "frei",
                    "offen",
                    "geschlossen",
                    "kaputt",
                    "zufrieden",
                    "eifersüchtig",
                ];
                if common_predicative_adjs.contains(&text_lower.as_str()) {
                    reasons.push(format!(
                        "'{}' is tagged as ADV but could be ADJ in predicative position (e.g., after sein/werden/bleiben). Check context: if it describes a state/quality of the subject, it should be ADJ.",
                        token.text
                    ));
                }
            }

            // Check for possessive determiners mistagged as ADV
            // e.g. "euer" in "Ich verstehe euer Französisch" should be DET, not ADV
            let possessive_forms = ["euer", "eure", "euren", "eurem", "eurer", "eures"];
            if possessive_forms.contains(&text_lower.as_str())
                && token.pos != PartOfSpeechTag::Det
                && token.pos != PartOfSpeechTag::Pron
            {
                reasons.push(format!(
                    "'{}' is tagged as {:?} but is likely a possessive determiner (DET) or pronoun (PRON)",
                    token.text, token.pos
                ));
            }

            // Check for dative/accusative pronouns with wrong lemma
            // In German, object pronouns should lemmatize to the nominative subject form:
            // mich/mir → ich, dich/dir → du, ihn/ihm → er, sie/ihr → sie,
            // uns → wir, euch → ihr, ihnen/Ihnen → sie/Sie
            let pronoun_lemma_checks: &[(&str, &[&str])] = &[
                ("mich", &["ich"]),
                ("mir", &["ich"]),
                ("dich", &["du"]),
                ("dir", &["du"]),
                ("ihn", &["er"]),
                ("ihm", &["er"]),
                ("uns", &["wir"]),
                ("euch", &["ihr"]),
                ("ihnen", &["sie", "Sie"]),
            ];
            if token.pos == PartOfSpeechTag::Pron {
                for (form, expected_lemmas) in pronoun_lemma_checks {
                    if text_lower == *form && !expected_lemmas.contains(&token.lemma.as_str()) {
                        reasons.push(format!(
                            "Pronoun '{}' has lemma '{}' but should be one of {:?}. Object/dative pronouns should lemmatize to the nominative form.",
                            token.text, token.lemma, expected_lemmas
                        ));
                    }
                }
            }

            // Check for plural nouns where the lemma is the plural form instead of singular
            // Common German plural patterns: -e, -er, -en, -n, -s, umlaut+e, etc.
            if token.pos == PartOfSpeechTag::Noun {
                let lemma = &token.lemma;
                // Nouns ending in common plural suffixes where the lemma matches the text
                // (suggesting no lemmatization happened)
                if lemma == &token.text && token.text.len() > 3 {
                    // Check for umlaut plurals (Äpfel, Bücher, Häuser, etc.)
                    let has_umlaut = token.text.contains('ä')
                        || token.text.contains('ö')
                        || token.text.contains('ü');
                    let ends_with_plural_suffix = token.text.ends_with('n')
                        || token.text.ends_with("er")
                        || token.text.ends_with('e')
                        || token.text.ends_with('s');
                    if has_umlaut && ends_with_plural_suffix {
                        reasons.push(format!(
                            "Noun '{}' has itself as lemma but contains an umlaut and a plural suffix — check if this is a plural form that should be lemmatized to its singular (e.g., Äpfel→Apfel, Bücher→Buch).",
                            token.text
                        ));
                    }
                    // Check for compound nouns with case endings (e.g., Holzgriffen → Holzgriff)
                    if token.text.ends_with("en")
                        || token.text.ends_with("ern")
                        || token.text.ends_with("eln")
                    {
                        reasons.push(format!(
                            "Noun '{}' has itself as lemma but ends in a common case/plural ending (-en/-ern/-eln) — check if this should be lemmatized to the base form (e.g., Holzgriffen→Holzgriff).",
                            token.text
                        ));
                    }
                }
            }

            // Check for möchte lemmatized to "möchten" instead of "mögen"
            if (text_lower == "möchte"
                || text_lower == "möchtest"
                || text_lower == "möchten"
                || text_lower == "möchtet")
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma != "mögen"
            {
                reasons.push(format!(
                    "'{}' has lemma '{}' but conventional German lemmatization uses 'mögen' (not 'möchten'). The Konjunktiv II form 'möchte' is lemmatized to its parent verb 'mögen'.",
                    token.text, token.lemma
                ));
            }

            // Check for "als" which can be SCONJ, ADP, or CCONJ depending on context
            if text_lower == "als" {
                reasons.push(format!(
                    "'als' can be: (1) SCONJ in temporal clauses ('als ich jung war'), (2) comparative particle after comparatives ('größer als'), (3) SCONJ meaning 'as/in the capacity of' ('als Lehrer'). Current POS: {:?}, lemma: '{}'.",
                    token.pos, token.lemma
                ));
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::German, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            if token.lemma == "sein" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (sein) is tagged AUX but is followed by adjective '{next_text}' — if this is a copula (e.g., 'er ist groß'), it should be VERB, not AUX. sein is only AUX when forming perfect tenses with past participles (e.g., 'ich bin gegangen').",
                        token.text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (sein) is tagged AUX — please double-check: it should be VERB when used as a copula (e.g., 'er ist groß', 'ich bin müde') and only AUX when forming perfect tenses with past participles (e.g., 'ich bin gegangen').",
                        token.text
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// German-specific corrector
struct GermanCorrector;

impl WordCorrector for GermanCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        for token in &mut sentence.doc {
            let text_lower = token.text.to_lowercase();

            // Fix personal pronouns that aren't properly lemmatized
            if token.pos == PartOfSpeechTag::Pron {
                // 2nd person plural: euch → ihr
                if text_lower == "euch" && token.lemma != "ihr" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'ihr'",
                        token.text, token.lemma
                    ));
                    token.lemma = "ihr".to_string();
                    corrected = true;
                }

                // 2nd person singular: dir, dich → du
                if (text_lower == "dir" || text_lower == "dich") && token.lemma != "du" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'du'",
                        token.text, token.lemma
                    ));
                    token.lemma = "du".to_string();
                    corrected = true;
                }

                // 1st person singular: mir, mich → ich
                if (text_lower == "mir" || text_lower == "mich") && token.lemma != "ich" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'ich'",
                        token.text, token.lemma
                    ));
                    token.lemma = "ich".to_string();
                    corrected = true;
                }
            }

            // Lowercase sentence-initial lemmas for non-noun, non-propn words
            // German nouns are always capitalized, but verbs/aux/det/adv/adj should have lowercase lemmas
            if token.pos != PartOfSpeechTag::Noun
                && token.pos != PartOfSpeechTag::Propn
                && token.pos != PartOfSpeechTag::Punct
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                let lower = token.lemma.to_lowercase();
                if lower != token.lemma {
                    corrections.push(format!(
                        "Lowercased {:?} lemma '{}' to '{}'",
                        token.pos, token.lemma, lower
                    ));
                    token.lemma = lower;
                    corrected = true;
                }
            }

            // Fix punctuation with lemma "--"
            if token.pos == PartOfSpeechTag::Punct && token.lemma == "--" {
                corrections.push(format!(
                    "Fixed punctuation '{}' lemma from '--' to itself",
                    token.text
                ));
                token.lemma = token.text.clone();
                corrected = true;
            }

            // Contractions keep their contracted form as lemma
            if let Some(expected) = contraction_lemma(Language::German, &text_lower, token.pos)
                && token.lemma != expected
            {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to '{expected}'",
                    token.text, token.lemma
                ));
                token.lemma = expected.to_string();
                corrected = true;
            }
        }

        CorrectionResult {
            corrected,
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Lowercase non-noun, non-propn lemmas
            if token.pos != PartOfSpeechTag::Noun
                && token.pos != PartOfSpeechTag::Propn
                && token.pos != PartOfSpeechTag::Punct
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                token.lemma = token.lemma.to_lowercase();
            }

            // Capitalize noun lemmas (German nouns are always capitalized)
            if token.pos == PartOfSpeechTag::Noun
                && token.lemma.chars().next().is_some_and(|c| c.is_lowercase())
            {
                let mut chars = token.lemma.chars();
                if let Some(first) = chars.next() {
                    token.lemma = first.to_uppercase().to_string() + chars.as_str();
                }
            }

            if let Some(expected) = contraction_lemma(Language::German, &text_lower, token.pos)
                && token.lemma != expected
            {
                token.lemma = expected.to_string();
            }
        }
    }
}

/// French-specific corrector
struct FrenchCorrector;

impl WordCorrector for FrenchCorrector {
    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Fix "non" POS
            if (text_lower == "ne" || text_lower == "n'" || text_lower == "non")
                && token.pos == PartOfSpeechTag::Part
            {
                token.pos = PartOfSpeechTag::Adv;
            }

            if text_lower == "ça" && token.lemma != "cela" {
                token.lemma = "cela".to_string();
            }

            // Pronoun lemma consistency
            let french_pronoun_fixes: &[(&str, &str)] = &[
                ("me", "me"),
                ("m'", "me"),
                ("te", "te"),
                ("t'", "te"),
                ("moi", "moi"),
                ("toi", "toi"),
                ("lui", "lui"),
                ("soi", "soi"),
                ("elles", "elle"),
                ("les", "le"),
            ];
            for &(form, expected_lemma) in french_pronoun_fixes {
                if text_lower == form
                    && token.pos == PartOfSpeechTag::Pron
                    && token.lemma != expected_lemma
                {
                    token.lemma = expected_lemma.to_string();
                    break;
                }
            }

            if let Some(expected) = contraction_lemma(Language::French, &text_lower, token.pos)
                && token.lemma != expected
            {
                token.lemma = expected.to_string();
            }
        }
    }

    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        // Use fold to build new token list, splitting hyphens as we go
        let original_tokens = std::mem::take(&mut sentence.doc);
        sentence.doc = original_tokens
            .into_iter()
            .fold(Vec::new(), |mut acc, mut token| {
                let text_lower = token.text.to_lowercase();

                // Fix "ne", "n'", and "non" - should always be Adv, not Part
                if (text_lower == "ne" || text_lower == "n'" || text_lower == "non")
                    && token.pos == PartOfSpeechTag::Part
                {
                    corrections.push(format!("Fixed '{}' POS from Part to Adv", token.text));
                    token.pos = PartOfSpeechTag::Adv;
                    corrected = true;
                }

                // Fix "ça" lemma - should always be "cela"
                if text_lower == "ça" && token.lemma != "cela" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'cela'",
                        token.text, token.lemma
                    ));
                    token.lemma = "cela".to_string();
                    corrected = true;
                }

                // Fix "elle" lemma - should always be "elle"
                if text_lower == "elle" && token.lemma != "elle" {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to 'elle'",
                        token.text, token.lemma
                    ));
                    token.lemma = "elle".to_string();
                    corrected = true;
                }

                // Fix pronoun lemma consistency: clitic and disjunctive pronouns keep their own form
                let french_pronoun_fixes: &[(&str, &str)] = &[
                    ("me", "me"),
                    ("m'", "me"),
                    ("te", "te"),
                    ("t'", "te"),
                    ("moi", "moi"),
                    ("toi", "toi"),
                    ("lui", "lui"),
                    ("soi", "soi"),
                ];
                for &(form, expected_lemma) in french_pronoun_fixes {
                    if text_lower == form
                        && token.pos == PartOfSpeechTag::Pron
                        && token.lemma != expected_lemma
                    {
                        corrections.push(format!(
                            "Fixed pronoun '{}' lemma from '{}' to '{}'",
                            token.text, token.lemma, expected_lemma
                        ));
                        token.lemma = expected_lemma.to_string();
                        corrected = true;
                        break;
                    }
                }

                // Fix contractions with themselves as lemma
                if text_lower == "j'" && token.lemma == "j'" {
                    corrections.push(format!("Fixed '{}' lemma from 'j'' to 'je'", token.text));
                    token.lemma = "je".to_string();
                    corrected = true;
                }

                if text_lower == "l'" && token.lemma == "l'" {
                    // Default to "le" if we can't determine gender
                    corrections.push(format!("Fixed '{}' lemma from 'l'' to 'le'", token.text));
                    token.lemma = "le".to_string();
                    corrected = true;
                }

                // Fix "-ce" (in "qu'est-ce que" etc.) with itself as lemma
                if text_lower == "-ce" && token.lemma == "-ce" {
                    corrections.push(format!("Fixed '{}' lemma from '-ce' to 'ce'", token.text));
                    token.lemma = "ce".to_string();
                    corrected = true;
                }

                // Fix "-là" (in "celles-là", "celui-là", etc.) with itself as lemma
                if text_lower == "-là" && token.lemma == "-là" {
                    corrections.push(format!("Fixed '{}' lemma from '-là' to 'là'", token.text));
                    token.lemma = "là".to_string();
                    corrected = true;
                }

                // Contractions keep their contracted form as lemma
                if let Some(expected) = contraction_lemma(Language::French, &text_lower, token.pos)
                    && token.lemma != expected
                {
                    corrections.push(format!(
                        "Fixed '{}' lemma from '{}' to '{expected}'",
                        token.text, token.lemma
                    ));
                    token.lemma = expected.to_string();
                    corrected = true;
                }

                // Fix "a" in "il y a" construction - should always be Verb
                if text_lower == "a" && token.pos != PartOfSpeechTag::Verb && acc.len() >= 2 {
                    // Check if preceded by "y" and "il"
                    let prev_token = &acc[acc.len() - 1];
                    let prev_prev_token = &acc[acc.len() - 2];

                    if prev_token.text.to_lowercase() == "y"
                        && prev_prev_token.text.to_lowercase() == "il"
                    {
                        corrections.push(format!(
                            "Fixed '{}' in 'il y a' construction from {:?} to Verb",
                            token.text, token.pos
                        ));
                        token.pos = PartOfSpeechTag::Verb;
                        // Also ensure lemma is "avoir"
                        if token.lemma != "avoir" {
                            corrections.push(format!(
                                "Fixed '{}' lemma in 'il y a' from '{}' to 'avoir'",
                                token.text, token.lemma
                            ));
                            token.lemma = "avoir".to_string();
                        }
                        corrected = true;
                    }
                }

                // Normalize possessive adjectives to masculine singular form
                if token.pos == PartOfSpeechTag::Det {
                    let possessive_normalizations = [
                        ("ta", "ton"),
                        ("ma", "mon"),
                        ("sa", "son"),
                        ("tes", "ton"),
                        ("mes", "mon"),
                        ("ses", "son"),
                        ("nos", "notre"),
                        ("vos", "votre"),
                        ("leurs", "leur"),
                    ];

                    for (form, normalized) in possessive_normalizations {
                        if text_lower == form && token.lemma != normalized {
                            corrections.push(format!(
                                "Normalized possessive '{}' lemma from '{}' to '{}'",
                                token.text, token.lemma, normalized
                            ));
                            token.lemma = normalized.to_string();
                            corrected = true;
                            break;
                        }
                    }

                    // Normalize definite articles to masculine singular form
                    if (text_lower == "la" || text_lower == "les") && token.lemma != "le" {
                        corrections.push(format!(
                            "Normalized article '{}' lemma from '{}' to 'le'",
                            token.text, token.lemma
                        ));
                        token.lemma = "le".to_string();
                        corrected = true;
                    }
                }

                if token.text.starts_with('-')
                    && token.text.len() > 1
                    && !acc.is_empty()
                    && acc.last().unwrap().whitespace.is_empty()
                {
                    // Remove hyphen from beginning of token
                    let original_text = token.text.clone();
                    token.text = token.text[1..].to_string();

                    corrections.push(format!(
                        "Split hyphen from beginning of '{original_text}' into separate token"
                    ));

                    // Create separate hyphen token
                    let hyphen_token = language_utils::DocToken {
                        text: "-".to_string(),
                        whitespace: String::new(), // No whitespace after hyphen
                        pos: PartOfSpeechTag::Punct,
                        lemma: "-".to_string(),
                        morph: std::collections::BTreeMap::new(),
                    };

                    acc.push(hyphen_token);
                    acc.push(token);
                    corrected = true;
                }
                // Split words ending in hyphen with no whitespace after
                else if token.text.ends_with('-')
                    && token.whitespace.is_empty()
                    && token.text.len() > 1
                {
                    // Remove hyphen from original token
                    let original_text = token.text.clone();
                    let original_whitespace = token.whitespace.clone();
                    token.text.pop();
                    token.whitespace = String::new(); // No whitespace after word part

                    corrections.push(format!(
                        "Split hyphen from end of '{original_text}' into separate token"
                    ));

                    // Create separate hyphen token with the original whitespace
                    let hyphen_token = language_utils::DocToken {
                        text: "-".to_string(),
                        whitespace: original_whitespace,
                        pos: PartOfSpeechTag::Punct,
                        lemma: "-".to_string(),
                        morph: std::collections::BTreeMap::new(),
                    };

                    acc.push(token);
                    acc.push(hyphen_token);
                    corrected = true;
                } else {
                    acc.push(token);
                }

                acc
            });

        CorrectionResult {
            corrected,
            corrections,
        }
    }
}

/// Italian-specific classifier
struct ItalianClassifier;

impl SentenceClassifier for ItalianClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        // Check for Space tokens which indicate NLP parsing issues
        for (idx, token) in sentence.doc.iter().enumerate() {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push(format!("Contains Space token: '{}'", sentence.sentence));
            }

            // Check for PROPN (proper noun) tags - often over-classified
            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun, but the legacy NLP pipeline often over-classifies things as proper nouns",
                    token.text
                ));
            }

            let text_lower = token.text.to_lowercase();

            // Check for lemmas containing spaces (parsing error)
            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    token.text, token.lemma
                ));
            }

            // Check for verbs/auxiliaries with themselves as lemma (no morphological analysis)
            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.text.to_lowercase() == token.lemma.to_lowercase()
            {
                reasons.push(format!(
                    "Check whether Verb/Aux '{}' should be lemmatized to an infinitive",
                    token.text,
                ));
            }

            // Check for object/reflexive pronouns with subject pronoun lemmas
            // Italian: io (I), tu (you), lui/lei (he/she), noi (we), voi (you pl), loro (they)
            // Object pronouns: mi, ti, lo, la, ci, vi, li, le
            if (text_lower == "mi" && token.lemma == "io")
                || (text_lower == "ti" && token.lemma == "tu")
                || (text_lower == "lo" && token.lemma == "lui")
                || (text_lower == "la"
                    && token.lemma == "lei"
                    && token.pos == PartOfSpeechTag::Pron)
                || (text_lower == "ci" && token.lemma == "noi")
                || (text_lower == "vi" && token.lemma == "voi")
                || (text_lower == "li" && token.lemma == "loro")
                || (text_lower == "le"
                    && token.lemma == "loro"
                    && token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "Check whether object/reflexive pronoun '{}' should have lemma '{}' (currently has subject pronoun lemma)",
                    token.text, token.lemma
                ));
            }

            // Check for specific Italian pronoun lemmatization issues
            // "gli" (to him/to them) should not be lemmatized to "il" (the)
            if text_lower == "gli" && token.lemma == "il" && token.pos == PartOfSpeechTag::Pron {
                reasons.push(
                    "Check whether pronoun 'gli' should be lemmatized to 'il' (article lemma)"
                        .to_string(),
                );
            }

            // "ne" (of it/of them) is a pronoun, not a conjunction
            if text_lower == "ne"
                && token.pos != PartOfSpeechTag::Pron
                && token.pos != PartOfSpeechTag::Adv
            {
                reasons.push(format!(
                    "Check whether 'ne' is really {:?} (often a pronoun or adverb)",
                    token.pos
                ));
            }

            // Check for essere conjugations which can be either AUX or VERB depending on context
            let essere_forms = [
                // Present
                "sono",
                "sei",
                "è",
                "siamo",
                "siete", // Imperfect
                "ero",
                "eri",
                "era",
                "eravamo",
                "eravate",
                "erano", // Passato remoto
                "fui",
                "fosti",
                "fu",
                "fummo",
                "foste",
                "furono", // Future
                "sarò",
                "sarai",
                "sarà",
                "saremo",
                "sarete",
                "saranno", // Conditional
                "sarei",
                "saresti",
                "sarebbe",
                "saremmo",
                "sareste",
                "sarebbero",
            ];

            let avere_forms = [
                // Present
                "ho",
                "hai",
                "ha",
                "abbiamo",
                "avete",
                "hanno", // Imperfect
                "avevo",
                "avevi",
                "aveva",
                "avevamo",
                "avevate",
                "avevano", // Passato remoto
                "ebbi",
                "avesti",
                "ebbe",
                "avemmo",
                "aveste",
                "ebbero", // Future
                "avrò",
                "avrai",
                "avrà",
                "avremo",
                "avrete",
                "avranno", // Conditional
                "avrei",
                "avresti",
                "avrebbe",
                "avremmo",
                "avreste",
                "avrebbero",
            ];

            let potere_forms = [
                // Present
                "posso",
                "puoi",
                "può",
                "possiamo",
                "potete",
                "possono", // Imperfect
                "potevo",
                "potevi",
                "poteva",
                "potevamo",
                "potevate",
                "potevano", // Passato remoto
                "potei",
                "potesti",
                "poté",
                "potemmo",
                "poteste",
                "poterono", // Future
                "potrò",
                "potrai",
                "potrà",
                "potremo",
                "potrete",
                "potranno", // Conditional
                "potrei",
                "potresti",
                "potrebbe",
                "potremmo",
                "potreste",
                "potrebbero",
            ];

            let dovere_forms = [
                // Present
                "devo",
                "devi",
                "deve",
                "dobbiamo",
                "dovete",
                "devono", // Imperfect
                "dovevo",
                "dovevi",
                "doveva",
                "dovevamo",
                "dovevate",
                "dovevano", // Future
                "dovrò",
                "dovrai",
                "dovrà",
                "dovremo",
                "dovrete",
                "dovranno", // Conditional
                "dovrei",
                "dovresti",
                "dovrebbe",
                "dovremmo",
                "dovreste",
                "dovrebbero",
            ];

            let volere_forms = [
                // Present
                "voglio",
                "vuoi",
                "vuole",
                "vogliamo",
                "volete",
                "vogliono", // Imperfect
                "volevo",
                "volevi",
                "voleva",
                "volevamo",
                "volevate",
                "volevano", // Future
                "vorrò",
                "vorrai",
                "vorrà",
                "vorremo",
                "vorrete",
                "vorranno", // Conditional
                "vorrei",
                "vorresti",
                "vorrebbe",
                "vorremmo",
                "vorreste",
                "vorrebbero",
            ];

            let stare_forms = [
                // Present
                "sto",
                "stai",
                "sta",
                "stiamo",
                "state",
                "stanno", // Imperfect
                "stavo",
                "stavi",
                "stava",
                "stavamo",
                "stavate",
                "stavano", // Passato remoto
                "stetti",
                "stesti",
                "stette",
                "stemmo",
                "steste",
                "stettero", // Future
                "starò",
                "starai",
                "starà",
                "staremo",
                "starete",
                "staranno", // Conditional
                "starei",
                "staresti",
                "starebbe",
                "staremmo",
                "stareste",
                "starebbero",
            ];

            if essere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "essere"
            {
                reasons.push(format!(
                    "'{}' (essere) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses with past participles (e.g., 'è andato'), VERB when used as a copula expressing identity/state (e.g., 'è bello', 'sono stanco', 'è tardi')",
                    token.text
                ));
            }

            if avere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "avere"
            {
                reasons.push(format!(
                    "'{}' (avere) can be either AUX or VERB depending on context. Rule: AUX when forming compound tenses with past participles (e.g., 'ho mangiato'), VERB when expressing possession or other meanings (e.g., 'ho un libro', 'ha fame')",
                    token.text
                ));
            }

            if potere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "potere"
            {
                reasons.push(format!(
                    "'{}' (potere) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'posso venire'), VERB when used standalone",
                    token.text
                ));
            }

            if dovere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "dovere"
            {
                reasons.push(format!(
                    "'{}' (dovere) can be either AUX or VERB depending on context. Rule: AUX when expressing obligation with infinitive (e.g., 'devo andare'), VERB when expressing owing (e.g., 'mi deve dei soldi')",
                    token.text
                ));
            }

            if volere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "volere"
            {
                reasons.push(format!(
                    "'{}' (volere) can be either AUX or VERB depending on context. Rule: AUX when expressing desire with infinitive (e.g., 'voglio andare'), VERB when used with direct object (e.g., 'voglio un caffè')",
                    token.text
                ));
            }

            if stare_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "stare"
            {
                reasons.push(format!(
                    "'{}' (stare) can be either AUX or VERB depending on context. Rule: AUX when forming progressive tenses with gerund (e.g., 'sto mangiando'), VERB when expressing state/location (e.g., 'sto bene', 'sta a casa')",
                    token.text
                ));
            }

            let andare_forms = [
                // Present
                "vado",
                "vai",
                "va",
                "andiamo",
                "andate",
                "vanno", // Imperfect
                "andavo",
                "andavi",
                "andava",
                "andavamo",
                "andavate",
                "andavano", // Passato remoto
                "andai",
                "andasti",
                "andò",
                "andammo",
                "andaste",
                "andarono", // Future
                "andrò",
                "andrai",
                "andrà",
                "andremo",
                "andrete",
                "andranno", // Conditional
                "andrei",
                "andresti",
                "andrebbe",
                "andremmo",
                "andreste",
                "andrebbero",
            ];

            let venire_forms = [
                // Present
                "vengo",
                "vieni",
                "viene",
                "veniamo",
                "venite",
                "vengono", // Imperfect
                "venivo",
                "venivi",
                "veniva",
                "venivamo",
                "venivate",
                "venivano", // Passato remoto
                "venni",
                "venisti",
                "venne",
                "venimmo",
                "veniste",
                "vennero", // Future
                "verrò",
                "verrai",
                "verrà",
                "verremo",
                "verrete",
                "verranno", // Conditional
                "verrei",
                "verresti",
                "verrebbe",
                "verremmo",
                "verreste",
                "verrebbero",
            ];

            let fare_forms = [
                // Present
                "faccio",
                "fai",
                "fa",
                "facciamo",
                "fate",
                "fanno", // Imperfect
                "facevo",
                "facevi",
                "faceva",
                "facevamo",
                "facevate",
                "facevano", // Passato remoto
                "feci",
                "facesti",
                "fece",
                "facemmo",
                "faceste",
                "fecero", // Future
                "farò",
                "farai",
                "farà",
                "faremo",
                "farete",
                "faranno", // Conditional
                "farei",
                "faresti",
                "farebbe",
                "faremmo",
                "fareste",
                "farebbero",
            ];

            let sapere_forms = [
                // Present
                "so",
                "sai",
                "sa",
                "sappiamo",
                "sapete",
                "sanno", // Imperfect
                "sapevo",
                "sapevi",
                "sapeva",
                "sapevamo",
                "sapevate",
                "sapevano", // Passato remoto
                "seppi",
                "sapesti",
                "seppe",
                "sapemmo",
                "sapeste",
                "seppero", // Future
                "saprò",
                "saprai",
                "saprà",
                "sapremo",
                "saprete",
                "sapranno", // Conditional
                "saprei",
                "sapresti",
                "saprebbe",
                "sapremmo",
                "sapreste",
                "saprebbero",
            ];

            if andare_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "andare"
            {
                reasons.push(format!(
                    "'{}' (andare) can be either AUX or VERB depending on context. Rule: AUX when forming passive-obligation (e.g., 'va fatto' = it must be done), VERB when expressing movement (e.g., 'vado a Roma')",
                    token.text
                ));
            }

            if venire_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "venire"
            {
                reasons.push(format!(
                    "'{}' (venire) can be either AUX or VERB depending on context. Rule: AUX when forming alternative passive (e.g., 'viene visto'), VERB when expressing coming/arriving (e.g., 'viene domani')",
                    token.text
                ));
            }

            if fare_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "fare"
            {
                reasons.push(format!(
                    "'{}' (fare) can be either AUX or VERB depending on context. Rule: AUX when used as causative with infinitive (e.g., 'faccio lavare la macchina'), VERB when meaning to do/make (e.g., 'faccio un dolce', 'fa caldo')",
                    token.text
                ));
            }

            if sapere_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "sapere"
            {
                reasons.push(format!(
                    "'{}' (sapere) can be either AUX or VERB depending on context. Rule: AUX when expressing ability with infinitive (e.g., 'so nuotare'), VERB when expressing knowledge (e.g., 'so la risposta')",
                    token.text
                ));
            }

            // Check for compound verb forms with clitics misclassified as nouns
            // Examples: dacci (dare + ci), dammi (dare + mi), dimmi (dire + mi)
            // These imperative + clitic combinations are often misclassified
            let common_clitic_endings = ["ci", "mi", "ti", "lo", "la", "vi", "li", "le", "ne"];
            if token.pos == PartOfSpeechTag::Noun {
                // Check if word ends with a common clitic and has verb lemma
                let ends_with_clitic = common_clitic_endings
                    .iter()
                    .any(|&ending| text_lower.ends_with(ending) && text_lower.len() > ending.len());

                // Also check if lemma is same as text (no morphological analysis)
                let lemma_matches_text = token.lemma.to_lowercase() == text_lower;

                if ends_with_clitic && lemma_matches_text {
                    reasons.push(format!(
                        "Check whether '{}' is really a NOUN (could be a verb with clitic pronoun, e.g., 'dacci' = dare + ci)",
                        token.text
                    ));
                }
            }

            // Check for participles/adjectives ending in -ato/-ato/-ito/-ito/-uto/-uto misclassified as nouns
            // These endings are common for past participles used as adjectives
            let participle_endings = [
                "ato", "ata", "ati", "ate", "ito", "ita", "iti", "ite", "uto", "uta", "uti", "ute",
            ];
            if token.pos == PartOfSpeechTag::Noun
                && participle_endings
                    .iter()
                    .any(|&ending| text_lower.ends_with(ending))
            {
                // Check if the lemma looks like an infinitive verb (ends in -are, -ere, -ire)
                let lemma_lower = token.lemma.to_lowercase();
                if lemma_lower.ends_with("are")
                    || lemma_lower.ends_with("ere")
                    || lemma_lower.ends_with("ire")
                {
                    reasons.push(format!(
                        "Check whether '{}' is really a NOUN (has verb lemma '{}' and participle ending)",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for adjectives like "arrabbiati" being misclassified
            // Words ending in -ato/-ito/-uto plural forms often misclassified
            if token.pos == PartOfSpeechTag::Noun
                && (text_lower.ends_with("ati")
                    || text_lower.ends_with("iti")
                    || text_lower.ends_with("uti"))
            {
                reasons.push(format!(
                    "Check whether '{}' is really a NOUN (ends in -ati/-iti/-uti, which are common adjective/participle endings)",
                    token.text
                ));
            }

            // Check for malformed verb infinitives (not ending in -are/-ere/-ire)
            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && !token.lemma.ends_with("are")
                && !token.lemma.ends_with("ere")
                && !token.lemma.ends_with("ire")
                && !token.lemma.ends_with("rre") // irregular verbs like "porre"
                && token.lemma.len() > 3
            {
                reasons.push(format!(
                    "Check whether Verb/Aux '{}' has correct lemma '{}' (doesn't end in -are/-ere/-ire/-rre)",
                    token.text, token.lemma
                ));
            }

            // Check for adjectives with plural forms as lemmas (should be singular)
            if token.pos == PartOfSpeechTag::Adj {
                let lemma_lower = token.lemma.to_lowercase();
                if lemma_lower.ends_with("i") && lemma_lower.len() > 2 {
                    // Common plural endings
                    reasons.push(format!(
                        "Check whether adjective '{}' should have lemma '{}' (appears to be plural)",
                        token.text, token.lemma
                    ));
                }
            }

            // Check for words that look like verbs but are misclassified as adverbs
            // Common misclassification: "vivo" (I live) tagged as Adv instead of Verb
            if token.pos == PartOfSpeechTag::Adv {
                // Common verb forms that might be misclassified as adverbs
                // These are first-person singular present tense forms
                let common_verb_forms = [
                    "vivo", "parlo", "mangio", "bevo", "scrivo", "leggo", "corro", "salto",
                ];
                if common_verb_forms.contains(&text_lower.as_str()) {
                    reasons.push(format!(
                        "Check whether '{}' is really an ADV (could be a verb, e.g., 'vivo' = I live)",
                        token.text
                    ));
                }
            }

            // Detect verbs with broken lemmas — Italian infinitives end in -are, -ere, -ire, -rre (porre/trarre/etc.)
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma_lower = token.lemma.to_lowercase();
                if !lemma_lower.ends_with("are")
                    && !lemma_lower.ends_with("ere")
                    && !lemma_lower.ends_with("ire")
                    && !lemma_lower.ends_with("rre")  // porre, trarre, durre
                    && !lemma_lower.ends_with("rsi")  // reflexive infinitives
                    && lemma_lower != "essere"
                    && lemma_lower != "avere"
                    && lemma_lower != "fare"
                    && lemma_lower != "dare"
                    && lemma_lower != "dire"
                    && lemma_lower != "stare"
                    && text_lower.len() > 2
                {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which doesn't look like an Italian infinitive — likely a failed lemmatization",
                        token.text, token.lemma
                    ));
                }

                // Detect -are → -ere corruption (e.g., "amare" → "amere")
                // This is a systematic issue where 1st conjugation (-are) gets turned into 2nd (-ere)
                if lemma_lower.ends_with("ere") && lemma_lower.len() > 4 {
                    let as_are = format!("{}are", &lemma_lower[..lemma_lower.len() - 3]);
                    // Common -are verbs that might get corrupted to -ere
                    let common_are_stems = [
                        "amare",
                        "parlare",
                        "mangiare",
                        "guardare",
                        "chiamare",
                        "trovare",
                        "pensare",
                        "lavorare",
                        "giocare",
                        "comprare",
                        "pagare",
                        "portare",
                        "provare",
                        "cambiare",
                        "sparare",
                        "mostrare",
                        "telefonare",
                        "darere",
                    ];
                    // If the -ere form matches a known -are verb pattern, flag it
                    if common_are_stems.contains(&lemma_lower.as_str())
                        || common_are_stems
                            .iter()
                            .any(|v| v.replace("are", "ere") == lemma_lower)
                    {
                        reasons.push(format!(
                            "'{}' has lemma '{}' — possible -are/-ere conjugation class confusion. Should it be '{}'?",
                            token.text, token.lemma, as_are
                        ));
                    }
                }
            }

            // Clitic-attached verb forms mistagged as Noun/Intj
            // e.g., "telefonami" (call me), "dimmi" (tell me), "dacci" (give us)
            if (token.pos == PartOfSpeechTag::Noun || token.pos == PartOfSpeechTag::Intj)
                && token.text.to_lowercase() == token.lemma.to_lowercase()
                && text_lower.len() > 4
            {
                // Common Italian clitic endings
                let clitic_endings = [
                    "mi", "ti", "ci", "vi", "lo", "la", "li", "le", "ne", "gli", "si",
                ];
                let has_clitic = clitic_endings.iter().any(|c| text_lower.ends_with(c));
                if has_clitic {
                    reasons.push(format!(
                        "'{}' tagged as {:?} with lemma '{}' — could be a verb+clitic form (imperative with attached pronoun)",
                        token.text, token.pos, token.lemma
                    ));
                }
            }

            // "sia" can be Cconj ("either...or") or Verb (subjunctive of essere)
            if text_lower == "sia" {
                reasons.push(format!(
                    "'sia' can be: (1) VERB — subjunctive/imperative of 'essere' (e.g., 'sia crudele' = be cruel), or (2) CCONJ — in 'sia...sia/che' constructions (e.g., 'sia l'uno sia l'altro'). Current POS: {:?}, lemma: '{}'.",
                    token.pos, token.lemma
                ));
            }

            // "sei" can be Num (6) or Verb (you are, from essere)
            if text_lower == "sei" {
                reasons.push(format!(
                    "'sei' can be: (1) AUX/VERB — 'you are' from 'essere' (e.g., 'sei impegnata?'), or (2) NUM — the number 6. Current POS: {:?}, lemma: '{}'.",
                    token.pos, token.lemma
                ));
            }

            // "Le" as Det vs Pron — when before a verb, it's usually a pronoun (her/to her)
            if text_lower == "le"
                && token.pos == PartOfSpeechTag::Det
                && let Some(next) = sentence.doc.get(idx + 1)
                && (next.pos == PartOfSpeechTag::Verb || next.pos == PartOfSpeechTag::Aux)
            {
                reasons.push(format!(
                            "'Le' tagged as DET but followed by verb '{}' — likely a pronoun (her/to her), not an article",
                            next.text
                        ));
            }

            // Adjectives mistagged as Verb — e.g., "vive" (alive) tagged as Verb/vivere
            // when it follows a copula like "sembrare", "essere", "restare"
            if token.pos == PartOfSpeechTag::Verb && idx > 0 {
                let prev = &sentence.doc[idx - 1];
                let prev_lemma = prev.lemma.to_lowercase();
                let copulas = [
                    "sembrare",
                    "essere",
                    "restare",
                    "diventare",
                    "rimanere",
                    "parere",
                    "apparire",
                ];
                if (prev.pos == PartOfSpeechTag::Verb || prev.pos == PartOfSpeechTag::Aux)
                    && copulas.iter().any(|c| prev_lemma == *c)
                {
                    reasons.push(format!(
                        "'{}' (lemma '{}') tagged as VERB after copula '{}' — could be an adjective used as predicate complement",
                        token.text, token.lemma, prev.text
                    ));
                }
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::Italian, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            if token.lemma == "essere" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (essere) is tagged AUX but is followed by adjective '{next_text}' — if this is a copula (e.g., 'è bello', 'sono stanco'), it should be VERB, not AUX. essere is only AUX when forming compound tenses with past participles (e.g., 'è andato') or passive (e.g., 'è stato costruito').",
                        token.text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (essere) is tagged AUX — please double-check: it should be VERB when used as a copula (e.g., 'è bello', 'è tardi') and only AUX when forming compound tenses (e.g., 'è andato') or passive (e.g., 'è stato costruito').",
                        token.text
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Italian-specific corrector
struct ItalianCorrector;

impl WordCorrector for ItalianCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        for token in &mut sentence.doc {
            let text_lower = token.text.to_lowercase();

            // Fix "non" POS - should always be Adv, not Part
            if text_lower == "non" && token.pos == PartOfSpeechTag::Part {
                corrections.push(format!("Fixed '{}' POS from Part to Adv", token.text));
                token.pos = PartOfSpeechTag::Adv;
                corrected = true;
            }

            // Fix clitic pronoun lemma consistency
            let italian_clitic_fixes: &[(&str, &str)] = &[
                ("mi", "mi"),
                ("ti", "ti"),
                ("si", "si"),
                ("ci", "ci"),
                ("vi", "vi"),
            ];
            for &(form, expected_lemma) in italian_clitic_fixes {
                if text_lower == form
                    && token.pos == PartOfSpeechTag::Pron
                    && token.lemma != expected_lemma
                {
                    corrections.push(format!(
                        "Fixed clitic pronoun '{}' lemma from '{}' to '{}'",
                        token.text, token.lemma, expected_lemma
                    ));
                    token.lemma = expected_lemma.to_string();
                    corrected = true;
                    break;
                }
            }

            // Fix "lei" (she) lemma - should be "lei", not "lui" (he)
            if text_lower == "lei" && token.lemma == "lui" && token.pos == PartOfSpeechTag::Pron {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to 'lei'",
                    token.text, token.lemma
                ));
                token.lemma = "lei".to_string();
                corrected = true;
            }

            // Fix "essa" (she/it) lemma - should be "essa", not "esso" (he/it)
            if text_lower == "essa" && token.lemma == "esso" {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to 'essa'",
                    token.text, token.lemma
                ));
                token.lemma = "essa".to_string();
                corrected = true;
            }

            // Fix feminine plural pronouns
            if text_lower == "esse" && token.lemma == "esso" {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to 'essa'",
                    token.text, token.lemma
                ));
                token.lemma = "essa".to_string();
                corrected = true;
            }

            // Fix "gli" when used as pronoun (not article)
            if text_lower == "gli" && token.lemma == "il" && token.pos == PartOfSpeechTag::Pron {
                corrections.push(format!(
                    "Fixed pronoun '{}' lemma from 'il' to 'gli'",
                    token.text
                ));
                token.lemma = "gli".to_string();
                corrected = true;
            }

            // Fix capitalized lemmas (only for non-proper nouns)
            if token.pos != PartOfSpeechTag::Propn
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                let lowercase_lemma = token.lemma.to_lowercase();
                corrections.push(format!(
                    "Fixed capitalized lemma '{}' to lowercase '{}'",
                    token.lemma, lowercase_lemma
                ));
                token.lemma = lowercase_lemma;
                corrected = true;
            }

            // Contractions keep their contracted form as lemma
            if let Some(expected) = contraction_lemma(Language::Italian, &text_lower, token.pos)
                && token.lemma != expected
            {
                corrections.push(format!(
                    "Fixed '{}' lemma from '{}' to '{expected}'",
                    token.text, token.lemma
                ));
                token.lemma = expected.to_string();
                corrected = true;
            }
        }

        CorrectionResult {
            corrected,
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Fix "non" POS
            if text_lower == "non" && token.pos == PartOfSpeechTag::Part {
                token.pos = PartOfSpeechTag::Adv;
            }

            // Clitic pronoun lemma consistency
            let italian_clitic_fixes: &[(&str, &str)] = &[
                ("mi", "mi"),
                ("ti", "ti"),
                ("si", "si"),
                ("ci", "ci"),
                ("vi", "vi"),
            ];
            for &(form, expected_lemma) in italian_clitic_fixes {
                if text_lower == form
                    && token.pos == PartOfSpeechTag::Pron
                    && token.lemma != expected_lemma
                {
                    token.lemma = expected_lemma.to_string();
                    break;
                }
            }

            if let Some(expected) = contraction_lemma(Language::Italian, &text_lower, token.pos)
                && token.lemma != expected
            {
                token.lemma = expected.to_string();
            }
        }
    }
}

/// Russian-specific classifier
struct RussianClassifier;

impl SentenceClassifier for RussianClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for token in &sentence.doc {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token, which is usually not necessary due to the `whitespace` field".to_string());
            }

            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun, but the legacy NLP pipeline often over-classifies things as proper nouns",
                    token.text
                ));
            }

            let text_lower = token.text.to_lowercase();

            // Check for lemmas containing spaces (parsing error)
            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    token.text, token.lemma
                ));
            }

            // --- быть (byt') AUX vs VERB disambiguation ---
            // VERB: copula ("он был учителем"), existential ("здесь будет парк")
            // AUX: passive ("он был принят"), future compound ("она будет петь")
            let byt_forms = [
                // Past
                "был",
                "была",
                "было",
                "были",
                // Future
                "буду",
                "будешь",
                "будет",
                "будем",
                "будете",
                "будут",
                // Imperative/infinitive
                "быть",
                "будь",
                "будьте",
                // Present (archaic/formal, rare but exists)
                "есть",
                "суть",
            ];

            if byt_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && (token.lemma == "быть" || token.lemma == "бы")
            {
                reasons.push(format!(
                    "'{}' (быть) can be either AUX or VERB depending on context. Rule: VERB when used as copula (e.g., 'он был учителем', 'здесь будет парк') or existential, AUX when forming passive (e.g., 'был принят') or future compound tense with imperfective infinitive (e.g., 'будет петь')",
                    token.text
                ));
            }

            // --- мочь (moch') AUX vs VERB ---
            let moch_forms = [
                "могу",
                "можешь",
                "может",
                "можем",
                "можете",
                "могут",
                "мог",
                "могла",
                "могло",
                "могли",
            ];

            if moch_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "мочь"
            {
                reasons.push(format!(
                    "'{}' (мочь) can be either AUX or VERB depending on context. Rule: AUX when expressing ability/possibility with infinitive (e.g., 'могу помочь'), VERB when used independently (rare)",
                    token.text
                ));
            }

            // --- хотеть (khotet') AUX vs VERB ---
            let khotet_forms = [
                "хочу",
                "хочешь",
                "хочет",
                "хотим",
                "хотите",
                "хотят",
                "хотел",
                "хотела",
                "хотело",
                "хотели",
            ];

            if khotet_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "хотеть"
            {
                reasons.push(format!(
                    "'{}' (хотеть) can be either AUX or VERB depending on context. Rule: AUX when expressing desire with infinitive (e.g., 'хочу спать'), VERB when expressing desire for a noun (e.g., 'хочу воды')",
                    token.text
                ));
            }

            // --- стать (stat') AUX vs VERB ---
            let stat_forms = [
                "стану",
                "станешь",
                "станет",
                "станем",
                "станете",
                "станут",
                "стал",
                "стала",
                "стало",
                "стали",
            ];

            if stat_forms.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && token.lemma == "стать"
            {
                reasons.push(format!(
                    "'{}' (стать) can be either AUX or VERB depending on context. Rule: VERB when meaning 'to become' (e.g., 'стал учителем'), AUX when forming negative future with imperfective infinitive (e.g., 'не стану слушать')",
                    token.text
                ));
            }

            // --- должен modal: often mistagged ---
            let dolzhen_forms = ["должен", "должна", "должно", "должны"];

            if dolzhen_forms.contains(&text_lower.as_str()) && token.pos != PartOfSpeechTag::Adj {
                reasons.push(format!(
                        "'{}' (должен) is a short-form adjective expressing obligation — should be tagged ADJ, not {:?}",
                        token.text, token.pos
                    ));
            }

            // --- Impersonal predicatives that are short-form neuter adjectives ---
            // нужно (нужный), должно (должный), важно (важный), видно (видный), etc.
            // These should be ADJ with the full adjective as lemma, so learners can
            // connect them to their adjective families. можно/надо/нельзя/пора are
            // genuine adverbs/particles with no adjective paradigm.
            let short_neuter_adj_predicatives: &[(&str, &str)] = &[
                ("нужно", "нужный"),
                ("должно", "должный"),
                ("важно", "важный"),
                ("видно", "видный"),
            ];

            for &(form, full_adj) in short_neuter_adj_predicatives {
                if text_lower == form && token.pos != PartOfSpeechTag::Adj {
                    reasons.push(format!(
                        "'{}' is the short neuter form of '{}' — should be tagged ADJ with lemma '{}', not {:?}. This connects learners to the full adjective paradigm.",
                        token.text, full_adj, full_adj, token.pos
                    ));
                }
            }

            // больно is genuinely ambiguous: predicative ADJ ("мне больно" = it hurts me)
            // vs true ADV ("больно ударить" = to hit painfully). Flag for context check.
            if text_lower == "больно" {
                reasons.push(
                    "'больно' is ambiguous: ADJ (short neuter of 'больной', predicative 'мне больно') vs ADV (adverb 'больно ударить'). Check context.".to_string()
                );
            }

            // можно/надо/нельзя/пора have no adjective paradigm — these are genuinely
            // adverbial/predicative and don't need POS correction.

            // --- DET vs PRON disambiguation ---
            let det_or_pron_words = [
                // Demonstratives
                "этот",
                "эта",
                "это",
                "эти",
                "тот",
                "та",
                "то",
                "те",
                // Possessives
                "мой",
                "моя",
                "моё",
                "мои",
                "твой",
                "твоя",
                "твоё",
                "твои",
                "наш",
                "наша",
                "наше",
                "наши",
                "ваш",
                "ваша",
                "ваше",
                "ваши",
                "свой",
                "своя",
                "своё",
                "свои",
                // Quantifiers
                "весь",
                "вся",
                "всё",
                "все",
                "каждый",
                "каждая",
                "каждое",
                "каждые",
                "какой",
                "какая",
                "какое",
                "какие",
                "некоторый",
                "некоторая",
                "некоторое",
                "некоторые",
                "другой",
                "другая",
                "другое",
                "другие",
                "такой",
                "такая",
                "такое",
                "такие",
                "сам",
                "сама",
                "само",
                "сами",
            ];

            if det_or_pron_words.contains(&text_lower.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (Rule: modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // --- все vs всё homograph (without ё) ---
            if text_lower == "все" || text_lower == "всё" {
                reasons.push(format!(
                    "'{}' — if written without ё, this could be 'все' (all/everyone, DET/PRON plural) or 'всё' (everything, PRON neuter singular). Check context.",
                    token.text
                ));
            }

            // --- уже homograph ---
            if text_lower == "уже" {
                reasons.push(
                    "'уже' could be ADV (already) or comparative ADJ (narrower, from узкий). Check context.".to_string()
                );
            }

            // --- Broken verb lemmas: Russian infinitives end in -ть/-ти/-чь ---
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma = &token.lemma;
                if !lemma.ends_with("ть")
                    && !lemma.ends_with("ти")
                    && !lemma.ends_with("чь")
                    && !lemma.ends_with("ться")
                    && !lemma.ends_with("тись")
                    && lemma != "быть"
                    && lemma != "есть"
                    && text_lower.len() > 4
                // skip very short tokens that might be particles
                {
                    reasons.push(format!(
                        "'{}' has lemma '{}' which doesn't look like a Russian infinitive (should end in -ть, -ти, -чь, or -ться) — likely a failed lemmatization",
                        token.text, token.lemma
                    ));
                }
            }

            // --- Participles: ADJ vs VERB ---
            // Russian participles (читающий, прочитанный, сделанный, etc.) can be ADJ or VERB
            if token.pos == PartOfSpeechTag::Adj || token.pos == PartOfSpeechTag::Verb {
                let t = &text_lower;
                // Active present participles (-ущий/-ющий/-ащий/-ящий)
                // Active past participles (-вший/-ший)
                // Passive past participles (-нный/-тый/-енный)
                // Passive present participles (-емый/-имый)
                let is_participle_form = t.ends_with("ущий")
                    || t.ends_with("ющий")
                    || t.ends_with("ащий")
                    || t.ends_with("ящий")
                    || t.ends_with("ущая")
                    || t.ends_with("ющая")
                    || t.ends_with("ащая")
                    || t.ends_with("ящая")
                    || t.ends_with("ущее")
                    || t.ends_with("ющее")
                    || t.ends_with("ащее")
                    || t.ends_with("ящее")
                    || t.ends_with("вший")
                    || t.ends_with("ший")
                    || t.ends_with("вшая")
                    || t.ends_with("шая")
                    || t.ends_with("вшее")
                    || t.ends_with("шее")
                    || t.ends_with("нный")
                    || t.ends_with("тый")
                    || t.ends_with("нная")
                    || t.ends_with("тая")
                    || t.ends_with("нное")
                    || t.ends_with("тое")
                    || t.ends_with("енный")
                    || t.ends_with("ённый")
                    || t.ends_with("емый")
                    || t.ends_with("имый")
                    || t.ends_with("емая")
                    || t.ends_with("имая")
                    || t.ends_with("емое")
                    || t.ends_with("имое");

                if is_participle_form {
                    reasons.push(format!(
                        "'{}' looks like a participle — verify POS. If used as modifier/adjective, tag ADJ with infinitive lemma. If part of verb phrase, tag VERB.",
                        token.text
                    ));
                }
            }

            // --- Short-form adjectives (рад, готов, должен, нужен, etc.) should be ADJ ---
            let short_adj_forms = [
                "рад",
                "рада",
                "радо",
                "рады",
                "готов",
                "готова",
                "готово",
                "готовы",
                "нужен",
                "нужна",
                "нужно",
                "нужны",
                "важен",
                "важна",
                "важно",
                "важны",
                "болен",
                "больна",
                // "больно" omitted — genuinely ambiguous (ADJ predicative vs ADV), handled separately
                "больны",
                "виден",
                "видна",
                "видно",
                "видны",
                "волен",
                "вольна",
                "вольно",
                "вольны",
                "прав",
                "права",
                "право",
                "правы",
                "жив",
                "жива",
                "живо",
                "живы",
                "похож",
                "похожа",
                "похоже",
                "похожи",
                "согласен",
                "согласна",
                "согласно",
                "согласны",
                "способен",
                "способна",
                "способно",
                "способны",
                "уверен",
                "уверена",
                "уверено",
                "уверены",
                "знаком",
                "знакома",
                "знакомо",
                "знакомы",
                "доволен",
                "довольна",
                "довольно",
                "довольны",
            ];

            if short_adj_forms.contains(&text_lower.as_str()) && token.pos != PartOfSpeechTag::Adj {
                reasons.push(format!(
                    "'{}' is a short-form adjective — should typically be tagged ADJ, not {:?}",
                    token.text, token.pos
                ));
            }

            // --- Pronoun lemma checks ---
            // Flag pronouns where lemma looks like an oblique form instead of nominative
            if token.pos == PartOfSpeechTag::Pron {
                let pron_fixes: &[(&[&str], &str)] = &[
                    (&["меня", "мне", "мной", "мною"], "я"),
                    (&["тебя", "тебе", "тобой", "тобою"], "ты"),
                    (&["его", "ему", "им", "нём", "него"], "он"),
                    (&["её", "ей", "ею", "неё", "ней"], "она"),
                    (&["нас", "нам", "нами"], "мы"),
                    (&["вас", "вам", "вами"], "вы"),
                    (&["их", "им", "ими", "них", "ним", "ними"], "они"),
                    (&["себя", "себе", "собой", "собою"], "себя"),
                ];

                for &(forms, expected_lemma) in pron_fixes {
                    if forms.contains(&text_lower.as_str()) && token.lemma != expected_lemma {
                        reasons.push(format!(
                            "Pronoun '{}' has lemma '{}', expected nominative form '{}'",
                            token.text, token.lemma, expected_lemma
                        ));
                        break;
                    }
                }
            }

            // --- Reflexive verb lemma convention: keep -ся ---
            // If a verb text has -ся/-сь but lemma doesn't, flag it
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let is_reflexive_form = text_lower.ends_with("ся") || text_lower.ends_with("сь");
                let lemma_is_reflexive = token.lemma.ends_with("ся") || token.lemma.ends_with("сь");

                if is_reflexive_form && !lemma_is_reflexive {
                    reasons.push(format!(
                        "'{}' is a reflexive verb form but lemma '{}' is non-reflexive — for pedagogy, reflexive verbs should keep -ся in the lemma (e.g., 'мыться', not 'мыть')",
                        token.text, token.lemma
                    ));
                }
            }

            // --- ё normalization: flag if lemma uses е where ё is expected ---
            // Common words where ё matters for learners
            let yo_words: &[(&str, &str)] = &[
                ("еще", "ещё"),
                ("все", "всё"), // when meaning "everything"
                ("ее", "её"),
                ("елка", "ёлка"),
                ("мед", "мёд"),
                ("лед", "лёд"),
                ("берет", "берёт"), // verb form
            ];

            for &(without_yo, with_yo) in yo_words {
                if token.lemma == without_yo {
                    reasons.push(format!(
                        "'{}' has lemma '{}' — check if it should be '{}' (with ё). Ё is important for learners.",
                        token.text, without_yo, with_yo
                    ));
                    break;
                }
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::Russian, &text_lower) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // быть tagged AUX followed by adjective/noun → likely should be VERB (copula)
            if token.lemma == "быть" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Adj) {
                    reasons.push(format!(
                        "'{}' (быть) is tagged AUX but is followed by adjective '{}' — if this is a copula (быть + adjective/noun predicate), it should be VERB, not AUX. быть is only AUX in passive (e.g., 'был принят') or future compound (e.g., 'будет петь').",
                        token.text, next_text
                    ));
                } else if next_pos == Some(PartOfSpeechTag::Noun) {
                    reasons.push(format!(
                        "'{}' (быть) is tagged AUX but is followed by noun '{}' — if this is a copula (e.g., 'он был учителем'), it should be VERB, not AUX.",
                        token.text, next_text
                    ));
                } else if next_pos != Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'{}' (быть) is tagged AUX — please double-check: it should be VERB when used as a copula or existential, and only AUX when forming passive or future compound with infinitive.",
                        token.text
                    ));
                }
            }

            // Flag non-PROPN tokens with spaces in lemma — these should be split
            // into separate tokens so learners see each word independently.
            if token.lemma.contains(' ') && token.pos != PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "'{}' has lemma '{}' containing a space — this should be split into separate tokens. Learners need to see each word as independent vocabulary (e.g., 'всё равно' → two tokens: 'всё' + 'равно', 'слава богу' → 'слава' + 'богу').",
                    token.text, token.lemma
                ));
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Russian-specific corrector
struct RussianCorrector;

impl WordCorrector for RussianCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let mut corrected = false;
        let mut corrections = Vec::new();

        for token in &mut sentence.doc {
            let text_lower = token.text.to_lowercase();

            // --- Pronoun lemma normalization to nominative form ---
            if token.pos == PartOfSpeechTag::Pron {
                let expected = match text_lower.as_str() {
                    "меня" | "мне" | "мной" | "мною" => Some("я"),
                    "тебя" | "тебе" | "тобой" | "тобою" => Some("ты"),
                    // его/ему/им are ambiguous (он or оно), but "он" is the standard lemma
                    "ему" | "им" | "нём" | "него" => Some("он"),
                    "её" | "ей" | "ею" | "неё" | "ней" => Some("она"),
                    "нас" | "нам" | "нами" => Some("мы"),
                    "вас" | "вам" | "вами" => Some("вы"),
                    "их" | "ими" | "них" | "ним" | "ними" => Some("они"),
                    "себя" | "себе" | "собой" | "собою" => Some("себя"),
                    _ => None,
                };

                if let Some(expected) = expected
                    && token.lemma != expected
                {
                    corrections.push(format!(
                        "Fixed pronoun '{}' lemma from '{}' to '{}'",
                        token.text, token.lemma, expected
                    ));
                    token.lemma = expected.to_string();
                    corrected = true;
                }
            }

            // --- Reflexive verb lemma: ensure -ся stays ---
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let is_reflexive_form = text_lower.ends_with("ся") || text_lower.ends_with("сь");
                let lemma_is_reflexive = token.lemma.ends_with("ся") || token.lemma.ends_with("сь");

                if is_reflexive_form && !lemma_is_reflexive {
                    // Append -ся to the lemma
                    let new_lemma = format!("{}ся", token.lemma);
                    corrections.push(format!(
                        "Fixed reflexive verb '{}' lemma from '{}' to '{}'",
                        token.text, token.lemma, new_lemma
                    ));
                    token.lemma = new_lemma;
                    corrected = true;
                }
            }

            // --- Fix capitalized lemmas for non-proper nouns ---
            if token.pos != PartOfSpeechTag::Propn
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                let lowercase_lemma = token.lemma.to_lowercase();
                corrections.push(format!(
                    "Fixed capitalized lemma '{}' to lowercase '{}'",
                    token.lemma, lowercase_lemma
                ));
                token.lemma = lowercase_lemma;
                corrected = true;
            }

            // --- Fix "не" POS: should be PART, not ADV ---
            // In Russian UD, "не" is PART (unlike French/Spanish "no"/"non" which are ADV)
            if text_lower == "не" && token.pos == PartOfSpeechTag::Adv {
                corrections.push(format!("Fixed '{}' POS from Adv to Part", token.text));
                token.pos = PartOfSpeechTag::Part;
                corrected = true;
            }
        }

        CorrectionResult {
            corrected,
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens {
            let text_lower = token.text.to_lowercase();

            // Pronoun lemma normalization
            if token.pos == PartOfSpeechTag::Pron {
                let expected = match text_lower.as_str() {
                    "меня" | "мне" | "мной" | "мною" => Some("я"),
                    "тебя" | "тебе" | "тобой" | "тобою" => Some("ты"),
                    "ему" | "им" | "нём" | "него" => Some("он"),
                    "её" | "ей" | "ею" | "неё" | "ней" => Some("она"),
                    "нас" | "нам" | "нами" => Some("мы"),
                    "вас" | "вам" | "вами" => Some("вы"),
                    "их" | "ими" | "них" | "ним" | "ними" => Some("они"),
                    "себя" | "себе" | "собой" | "собою" => Some("себя"),
                    _ => None,
                };

                if let Some(expected) = expected
                    && token.lemma != expected
                {
                    token.lemma = expected.to_string();
                }
            }

            // Reflexive verb lemma normalization
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let is_reflexive_form = text_lower.ends_with("ся") || text_lower.ends_with("сь");
                let lemma_is_reflexive = token.lemma.ends_with("ся") || token.lemma.ends_with("сь");

                if is_reflexive_form && !lemma_is_reflexive {
                    token.lemma = format!("{}ся", token.lemma);
                }
            }

            // Fix capitalized lemmas
            if token.pos != PartOfSpeechTag::Propn
                && token.lemma.chars().next().is_some_and(|c| c.is_uppercase())
            {
                token.lemma = token.lemma.to_lowercase();
            }

            // Fix "не" POS
            if text_lower == "не" && token.pos == PartOfSpeechTag::Adv {
                token.pos = PartOfSpeechTag::Part;
            }
        }
    }
}

/// Chinese-specific classifier
struct ChineseClassifier;

impl SentenceClassifier for ChineseClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for (idx, token) in sentence.doc.iter().enumerate() {
            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token, which is usually not necessary due to the `whitespace` field".to_string());
            }

            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{}' classified as a proper noun — subtitle data often over-classifies common words as proper nouns",
                    token.text
                ));
            }

            // Check for lemmas containing spaces (parsing error)
            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    token.text, token.lemma
                ));
            }

            // Chinese words generally should have themselves as lemma (no morphological inflection)
            // but check for obvious mismatches
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Adj {
                // In Chinese, the surface form IS the lemma — no conjugation
                if token.text != token.lemma && !token.lemma.is_empty() {
                    reasons.push(format!(
                        "'{}' ({:?}) has different lemma '{}' — Chinese words generally don't inflect, so lemma should match surface form",
                        token.text, token.pos, token.lemma
                    ));
                }
            }

            // --- Word segmentation sanity checks ---
            // Single-character tokens that are commonly part of multi-character words
            // Flag when a single CJK character is tagged as a content word — may be an
            // over-segmentation error (e.g., 因 + 为 instead of 因为, 可 + 以 instead of 可以)
            let char_count = token.text.chars().count();
            if char_count == 1
                && matches!(
                    token.pos,
                    PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Noun
                )
            {
                let c = token.text.chars().next().unwrap();
                let is_cjk = ('\u{4E00}'..='\u{9FFF}').contains(&c);
                if is_cjk {
                    // Check if the next token is also a single CJK character with the same POS
                    // — strong signal of over-segmentation
                    if let Some(next) = sentence.doc.get(idx + 1) {
                        let next_count = next.text.chars().count();
                        let next_is_single_cjk = next_count == 1
                            && next
                                .text
                                .chars()
                                .next()
                                .is_some_and(|nc| ('\u{4E00}'..='\u{9FFF}').contains(&nc));
                        if next_is_single_cjk
                            && next.pos == token.pos
                            && token.whitespace.is_empty()
                        {
                            reasons.push(format!(
                                "'{}' + '{}' are adjacent single-character {} tokens with no whitespace — possible over-segmentation (should these be one word '{}{}'?)",
                                token.text, next.text, format!("{:?}", token.pos).to_lowercase(),
                                token.text, next.text
                            ));
                        }
                    }
                }
            }

            // 是 (shì) AUX vs VERB disambiguation — copula
            if token.text == "是"
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
            {
                reasons.push(format!(
                    "'是' can be either AUX or VERB depending on context. Rule: VERB when used as copula (e.g., '他是老师' = he is a teacher), AUX in 是...的 focus constructions (e.g., '他是昨天来的'). Current POS: {:?}",
                    token.pos
                ));
            }

            // 在 (zài) — one of the most common and most ambiguous words
            // VERB (to be at: 他在家), ADP (at/in: 在学校学习), ADV (progressive: 他在吃饭)
            if token.text == "在" {
                if token.pos == PartOfSpeechTag::Verb
                    || token.pos == PartOfSpeechTag::Adp
                    || token.pos == PartOfSpeechTag::Adv
                {
                    reasons.push(format!(
                        "'在' is highly ambiguous: VERB when copula-like (e.g., '他在家' = he is at home), ADP when prepositional (e.g., '在学校学习' = study at school), ADV when progressive aspect marker (e.g., '他在吃饭' = he is eating). Current POS: {:?}",
                        token.pos
                    ));
                } else {
                    reasons.push(format!(
                        "'在' tagged as {:?} but should be VERB (locative copula), ADP (preposition), or ADV (progressive marker)",
                        token.pos
                    ));
                }
            }

            // 有 (yǒu) — VERB for possession/existence, not AUX
            if token.text == "有" && token.pos == PartOfSpeechTag::Aux {
                reasons.push(
                    "'有' tagged as AUX — verify: 有 is typically VERB (possession: '我有书', existence: '有人来了'). AUX usage is rare."
                        .to_string(),
                );
            }

            // 把 (bǎ) — disposal/object-fronting marker (ADP) vs measure word vs VERB "to hold"
            if token.text == "把" {
                if token.pos == PartOfSpeechTag::Verb {
                    reasons.push(
                        "'把' tagged as VERB — check context: it's most commonly ADP (disposal construction: '把书放下'), rarely VERB meaning 'to hold/guard'. As a measure word it should be NOUN."
                            .to_string(),
                    );
                } else if token.pos == PartOfSpeechTag::Adp {
                    // Correct for disposal, but flag for review
                    reasons.push(
                        "'把' as ADP (disposal/object-fronting marker, e.g., '把门关上') — verify this is the disposal construction and not the measure word or verb"
                            .to_string(),
                    );
                }
            }

            // 被 (bèi) — passive marker (ADP) vs VERB "to cover"
            if token.text == "被" {
                if token.pos == PartOfSpeechTag::Verb {
                    reasons.push(
                        "'被' tagged as VERB — check context: it's most commonly ADP (passive marker: '被打了' = was hit). It's rarely used as VERB meaning 'to cover' in modern Chinese."
                            .to_string(),
                    );
                } else if token.pos != PartOfSpeechTag::Adp {
                    reasons.push(format!(
                        "'被' tagged as {:?} but is typically ADP (passive marker, e.g., '被老师批评了' = was criticized by the teacher)",
                        token.pos
                    ));
                }
            }

            // 会/能/可以/要/想/应该 — modal verbs, AUX vs VERB
            // (exclude 得 — handled separately below to avoid double-flagging)
            let modal_verbs = [
                "会", "能", "可以", "要", "想", "应该", "必须", "可能", "愿意",
            ];
            if modal_verbs.contains(&token.text.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
            {
                reasons.push(format!(
                    "'{}' can be either AUX or VERB depending on context. AUX when modifying another verb (e.g., '我会说中文'), VERB when standalone (e.g., '我要这个'). Current POS: {:?}",
                    token.text, token.pos
                ));
            }

            // 得 (de/dé/děi) — three-way ambiguity, handled in one place to avoid double-flagging:
            // 1. Structural particle (complement marker): PART (e.g., 跑得快)
            // 2. Modal "must" (děi): AUX (e.g., 我得走了)
            // 3. Verb "to get/obtain" (dé): VERB (e.g., 得到)
            if token.text == "得" {
                reasons.push(format!(
                    "'得' is three-way ambiguous: PART when complement marker (e.g., '跑得快' = runs fast), AUX when modal 'must' (e.g., '我得走了' = I must go), VERB when meaning 'to get' (rare standalone, more common in compounds like '得到'). Current POS: {:?}",
                    token.pos
                ));
            }

            // 了/过/着 — aspect particles, often mistagged as VERB
            // 了: perfective aspect or sentence-final change-of-state
            // 过: experiential aspect ("have you ever...") vs VERB "to pass/cross"
            // 着: durative aspect ("holding") vs VERB "to touch/arrive"
            if token.text == "了" && token.pos == PartOfSpeechTag::Verb {
                reasons.push(
                    "'了' tagged as VERB — verify: 了 is almost always PART (aspect marker after verb, or sentence-final change-of-state). VERB usage is extremely rare in modern Chinese."
                        .to_string(),
                );
            }

            if token.text == "过" {
                if token.pos == PartOfSpeechTag::Verb {
                    // Could be VERB "to pass" or mistagged aspect particle
                    // If preceded by another verb, it's almost certainly the aspect particle
                    let prev_is_verb = idx > 0
                        && matches!(
                            sentence.doc[idx - 1].pos,
                            PartOfSpeechTag::Verb | PartOfSpeechTag::Adj
                        );
                    if prev_is_verb {
                        reasons.push(
                            "'过' tagged as VERB after another verb — likely the experiential aspect particle (PART), meaning 'have ever done X' (e.g., '我去过中国' = I have been to China). Should be PART."
                                .to_string(),
                        );
                    } else {
                        reasons.push(format!(
                            "'过' can be VERB (to pass/cross, e.g., '过马路') or PART (experiential aspect after a verb, e.g., '吃过'). Current POS: {:?}",
                            token.pos
                        ));
                    }
                } else if token.pos == PartOfSpeechTag::Part {
                    // Fine, but note it for context review
                } else {
                    reasons.push(format!(
                        "'过' tagged as {:?} but should be VERB (to pass) or PART (experiential aspect)",
                        token.pos
                    ));
                }
            }

            if token.text == "着" {
                if token.pos == PartOfSpeechTag::Verb {
                    let prev_is_verb = idx > 0
                        && matches!(
                            sentence.doc[idx - 1].pos,
                            PartOfSpeechTag::Verb | PartOfSpeechTag::Adj
                        );
                    if prev_is_verb {
                        reasons.push(
                            "'着' tagged as VERB after another verb — likely the durative aspect particle (PART), indicating ongoing state (e.g., '开着门' = the door is open, '穿着红衣服' = wearing red). Should be PART."
                                .to_string(),
                        );
                    } else {
                        reasons.push(format!(
                            "'着' can be VERB (to touch/arrive, rare) or PART (durative aspect after a verb, e.g., '拿着'). Current POS: {:?}",
                            token.pos
                        ));
                    }
                } else if token.pos != PartOfSpeechTag::Part {
                    reasons.push(format!(
                        "'着' tagged as {:?} but should typically be PART (durative aspect particle)",
                        token.pos
                    ));
                }
            }

            // 的/地 — structural particles, should be PART
            // (得 is handled separately above)
            if (token.text == "的" || token.text == "地") && token.pos != PartOfSpeechTag::Part {
                reasons.push(format!(
                    "'{}' tagged as {:?} but is typically PART (structural particle). 的 = attributive, 地 = adverbial",
                    token.text, token.pos
                ));
            }

            // 吗/呢/吧 — sentence-final particles, should be PART
            if (token.text == "吗" || token.text == "呢" || token.text == "吧")
                && token.pos != PartOfSpeechTag::Part
            {
                reasons.push(format!(
                    "'{}' tagged as {:?} — verify: sentence-final particles (吗/呢/吧) are typically PART. Check if this is genuinely sentence-final.",
                    token.text, token.pos
                ));
            }

            // Measure words / classifiers — after numbers OR demonstratives
            let common_measure_words = [
                "个", "只", "条", "张", "件", "本", "台", "辆", "位", "块", "杯", "瓶", "双", "次",
                "遍", "种", "些",
            ];
            // (把 excluded — handled separately above as disposal marker)
            if common_measure_words.contains(&token.text.as_str()) && idx > 0 {
                let prev = &sentence.doc[idx - 1];
                let prev_triggers_classifier = prev.pos == PartOfSpeechTag::Num
                    || ["这", "那", "哪", "每", "几"].contains(&prev.text.as_str());
                if prev_triggers_classifier && token.pos != PartOfSpeechTag::Noun {
                    reasons.push(format!(
                        "'{}' after '{}' is a measure word/classifier — check POS (currently {:?})",
                        token.text, prev.text, token.pos
                    ));
                }
            }

            // 不/没 negation — should be ADV
            if (token.text == "不" || token.text == "没") && token.pos != PartOfSpeechTag::Adv {
                reasons.push(format!(
                    "'{}' tagged as {:?} — verify: 不/没 are typically ADV (negation). Check context.",
                    token.text, token.pos
                ));
            }
            // 没有 — ADV (negation, "haven't") or VERB ("don't have")
            if token.text == "没有"
                && token.pos != PartOfSpeechTag::Adv
                && token.pos != PartOfSpeechTag::Verb
            {
                reasons.push(format!(
                    "'没有' tagged as {:?} but should be ADV (negation: '没有去过') or VERB (non-possession: '我没有钱')",
                    token.pos
                ));
            }

            // DET/PRON ambiguity for demonstratives
            let det_or_pron_words = ["这", "那", "这些", "那些", "每", "哪", "哪些", "某", "各"];
            if det_or_pron_words.contains(&token.text.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{}' can be either DET or PRON depending on context (modifies noun → DET, stands alone → PRON)",
                    token.text
                ));
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::ChineseSimplified, &token.text) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // 是 tagged AUX followed by noun → likely copula VERB
            if token.text == "是" && token.pos == PartOfSpeechTag::Aux {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                let next_text = next.map(|t| t.text.as_str()).unwrap_or("");

                if next_pos == Some(PartOfSpeechTag::Noun) || next_pos == Some(PartOfSpeechTag::Adj)
                {
                    reasons.push(format!(
                        "'是' is tagged AUX but is followed by '{}' ({:?}) — if this is a copula (是 + noun/adjective), it should be VERB, not AUX. 是 is only AUX in 是...的 focus constructions.",
                        next_text, next_pos.unwrap_or(PartOfSpeechTag::X)
                    ));
                }
            }

            // 在 tagged as VERB but followed by another verb → likely progressive ADV
            if token.text == "在" && token.pos == PartOfSpeechTag::Verb {
                let next = tokens.get(idx + 1);
                let next_pos = next.map(|t| t.pos);
                if next_pos == Some(PartOfSpeechTag::Verb) {
                    reasons.push(format!(
                        "'在' is tagged VERB but followed by verb '{}' — if this is the progressive marker (在 + verb = doing), it should be ADV, not VERB.",
                        next.map(|t| t.text.as_str()).unwrap_or("")
                    ));
                }
            }

            // Complement-marker 得 (PART) only exists after a verb/adjective. Anywhere
            // else it is the modal děi (AUX) or the verb dé.
            if token.text == "得" && token.pos == PartOfSpeechTag::Part {
                let prev_pos = idx.checked_sub(1).map(|i| tokens[i].pos);
                if !matches!(
                    prev_pos,
                    Some(PartOfSpeechTag::Verb | PartOfSpeechTag::Adj | PartOfSpeechTag::Aux)
                ) {
                    reasons.push(format!(
                        "'得' is tagged PART but does not follow a verb/adjective (previous: {}) — the complement marker only appears after a predicate. Is this the modal 得 (AUX, 'must') or the verb 得 ('to get')?",
                        idx.checked_sub(1)
                            .map(|i| tokens[i].text.as_str())
                            .unwrap_or("<start>")
                    ));
                }
            }

            // A modal tagged AUX must govern a verb somewhere to its right. The
            // deterministic rule already fixes the adjacent cases; this catches an AUX
            // stranded before a pure noun phrase.
            if ZH_MODAL_VERBS.contains(&token.text.as_str())
                && token.pos == PartOfSpeechTag::Aux
                && !tokens[idx + 1..]
                    .iter()
                    .any(|t| matches!(t.pos, PartOfSpeechTag::Verb | PartOfSpeechTag::Aux))
            {
                reasons.push(format!(
                    "modal '{}' is tagged AUX but no verb follows it — a standalone modal (e.g. 我要这个, 我不会) is VERB.",
                    token.text
                ));
            }

            // 是 is only ever the copula (VERB), the 是...的 focus marker (AUX), or a
            // bare "yes" reply (INTJ). Anything else — the ADV it sometimes gets in
            // 是不是 A-not-A strings — is a mistag.
            if token.text == "是"
                && !matches!(
                    token.pos,
                    PartOfSpeechTag::Verb | PartOfSpeechTag::Aux | PartOfSpeechTag::Intj
                )
            {
                reasons.push(format!(
                    "'是' is tagged {:?} — 是 is VERB (copula), AUX (是...的 focus), or INTJ (bare reply); in 是不是 each 是 is VERB.",
                    token.pos
                ));
            }

            // Aspect particles attach to a preceding predicate (or, for sentence-final
            // 了, at least a preceding phrase). At the start of a sentence or right
            // after punctuation they cannot be aspect markers.
            if matches!(token.text.as_str(), "了" | "过" | "着")
                && token.pos == PartOfSpeechTag::Part
            {
                let prev_pos = idx.checked_sub(1).map(|i| tokens[i].pos);
                if prev_pos.is_none() || prev_pos == Some(PartOfSpeechTag::Punct) {
                    reasons.push(format!(
                        "'{}' is tagged PART but starts the sentence/follows punctuation — an aspect particle needs something before it. Retokenization may be needed.",
                        token.text
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Chinese-specific corrector
struct ChineseCorrector;

impl WordCorrector for ChineseCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let corrections = fix_chinese(&mut sentence.doc);
        CorrectionResult {
            corrected: !corrections.is_empty(),
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        fix_chinese(tokens);
    }
}

/// Thai-specific corrector
struct ThaiCorrector;

impl WordCorrector for ThaiCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let corrections = fix_thai(&mut sentence.doc);
        CorrectionResult {
            corrected: !corrections.is_empty(),
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        fix_thai(tokens);
    }
}

/// True for a token that is a single Thai-script character. Thai has essentially no
/// one-character words (a bare consonant or vowel sign is not a word), so these are
/// almost always fragments of an unknown word the dictionary-based segmenter
/// couldn't handle — typically a transliterated name. The two real one-character
/// signs are exempt: ๆ (mai yamok, repetition) and ฯ (paiyannoi, abbreviation).
fn is_thai_fragment(text: &str) -> bool {
    let mut chars = text.trim().chars();
    match (chars.next(), chars.next()) {
        (Some('ๆ') | Some('ฯ'), None) => false,
        (Some(c), None) => ('\u{0E00}'..='\u{0E7F}').contains(&c),
        _ => false,
    }
}

/// Thai-specific classifier
struct ThaiClassifier;

impl SentenceClassifier for ThaiClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for token in sentence.doc.iter() {
            let text = token.text.trim();

            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token, which is usually not necessary due to the `whitespace` field".to_string());
            }

            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{text}' classified as a proper noun — subtitle data often over-classifies common words as proper nouns"
                ));
            }

            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    text, token.lemma
                ));
            }

            // Thai words don't inflect — surface form is the lemma
            if token.text != token.lemma && !token.lemma.is_empty() {
                reasons.push(format!(
                    "'{}' has different lemma '{}' — Thai words don't inflect, so lemma should match surface form",
                    token.text, token.lemma
                ));
            }

            // Single-character fragments: the dictionary segmenter shreds unknown
            // words (usually transliterated names) into meaningless pieces.
            if is_thai_fragment(&token.text) {
                reasons.push(format!(
                    "'{text}' is a single Thai character — almost never a real word. Likely a fragment of an unsegmented word (a transliterated name?); merge the whole run into one token."
                ));
            }

            // ได้ — three-way ambiguous, the LLM must decide from context
            if text == "ได้" {
                reasons.push(format!(
                    "'ได้' is ambiguous: AUX before a verb (past attainment: ได้ไป), AUX after a verb (potential: ไปได้ = can go), VERB as main verb 'get/receive' (ได้เงิน). Current POS: {:?}",
                    token.pos
                ));
            }

            // ที่ — the most overloaded word in Thai
            if text == "ที่" {
                reasons.push(format!(
                    "'ที่' is ambiguous: ADP 'at' before a place, SCONJ relativizer/complementizer (คนที่มา, ขอโทษที่มาสาย), NOUN 'place/land', or ordinal marker before a numeral (ที่สอง). Current POS: {:?}",
                    token.pos
                ));
            }

            // ให้ — verb vs grammatical marker
            if text == "ให้" {
                reasons.push(format!(
                    "'ให้' is ambiguous: VERB 'give' (ให้เงิน), ADP benefactive 'for' (ทำให้แม่ = do for mother), or causative marker (ทำให้เขาเสียใจ). Current POS: {:?}",
                    token.pos
                ));
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            let text = token.text.trim();

            // จะ has no reading other than the irrealis marker
            if text == "จะ" && token.pos != PartOfSpeechTag::Aux {
                reasons.push(format!(
                    "'จะ' is tagged {:?} — จะ is the irrealis/future marker and should be AUX.",
                    token.pos
                ));
            }

            // A preverbal marker tagged AUX must govern a verb somewhere to its right.
            if TH_PREVERBAL_AUX.contains(&text)
                && token.pos == PartOfSpeechTag::Aux
                && !tokens[idx + 1..]
                    .iter()
                    .any(|t| matches!(t.pos, PartOfSpeechTag::Verb | PartOfSpeechTag::Aux))
            {
                reasons.push(format!(
                    "marker '{text}' is tagged AUX but no verb follows it — a standalone modal answering a question (ได้, ต้อง) is VERB."
                ));
            }

            // Copulas are VERB in this dataset (matching the Chinese 是 convention)
            if matches!(text, "เป็น" | "คือ") && token.pos == PartOfSpeechTag::Aux {
                reasons.push(format!(
                    "'{text}' is tagged AUX — the copula is VERB in this dataset (เขาเป็นครู → เป็น VERB). AUX is only right for postverbal potential เป็น ('know how to')."
                ));
            }

            // A single-character fragment survived the LLM pass
            if is_thai_fragment(&token.text) {
                reasons.push(format!(
                    "'{text}' is a single Thai character, which is almost never a real word — is this an unmerged fragment of a longer word or name?"
                ));
            }

            // ได้ has exactly two tags: AUX (preverbal attainment / postverbal
            // potential) or VERB ("get/receive")
            if text == "ได้" && !matches!(token.pos, PartOfSpeechTag::Verb | PartOfSpeechTag::Aux)
            {
                reasons.push(format!(
                    "'ได้' is tagged {:?} — ได้ is AUX (before a verb: attainment; after a verb: potential) or VERB ('get/receive'), never anything else.",
                    token.pos
                ));
            }

            // ที่ has a closed set of readings
            if text == "ที่"
                && !matches!(
                    token.pos,
                    PartOfSpeechTag::Adp
                        | PartOfSpeechTag::Sconj
                        | PartOfSpeechTag::Noun
                        | PartOfSpeechTag::Part
                )
            {
                reasons.push(format!(
                    "'ที่' is tagged {:?} — ที่ is ADP ('at'), SCONJ (relativizer/complementizer), NOUN ('place'), or PART (ordinal marker). A locative like ที่นั่น/ที่นี่ should be one merged token instead.",
                    token.pos
                ));
            }

            // ไหม mid-sentence is the noun "silk", not the question particle
            if text == "ไหม"
                && token.pos == PartOfSpeechTag::Part
                && !tokens[idx + 1..]
                    .iter()
                    .all(|t| matches!(t.pos, PartOfSpeechTag::Punct | PartOfSpeechTag::Part))
            {
                reasons.push(
                    "'ไหม' is tagged PART but is not sentence-final — the question particle only appears at the end; mid-sentence ไหม is the noun 'silk'.".to_string(),
                );
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Japanese words where a たち/達 ending is part of the word, not the plural suffix.
const TATSU_COMPOUNDS: &[&str] = &[
    "友達",
    "発達",
    "上達",
    "調達",
    "伝達",
    "配達",
    "到達",
    "速達",
    "通達",
    "熟達",
    "闊達",
    "かたち",
];

/// Japanese-specific classifier
///
/// Tokenization policy: a token is a word, not a morpheme. A predicate and its bound
/// auxiliary chain are one token (食べませんでした), while boundaries whose two sides
/// are both words survive (食べて|いる, 学生|です, 静か|です). The policy is spelled
/// out in `language_specific_tips` and enforced mechanically by `JapaneseCorrector`
/// (`merge_japanese_inflection`) on both the NLP proposal and the LLM output; the
/// rules here only flag what the mechanical pass cannot fix on its own.
struct JapaneseClassifier;

impl SentenceClassifier for JapaneseClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for (idx, token) in sentence.doc.iter().enumerate() {
            let text = &token.text;

            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token".to_string());
            }

            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{text}' classified as a proper noun — subtitle data often over-classifies common words as proper nouns"
                ));
            }

            if token.lemma.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    text, token.lemma
                ));
            }

            // --- Verb lemmatization: dictionary form ends in う-row kana ---
            if token.pos == PartOfSpeechTag::Verb {
                let lemma = &token.lemma;
                let u_row = ['う', 'く', 'す', 'つ', 'ぬ', 'ぶ', 'む', 'る', 'ぐ', 'ず'];
                let has_japanese = lemma.chars().any(|c| {
                    ('\u{3040}'..='\u{309F}').contains(&c)
                        || ('\u{4E00}'..='\u{9FFF}').contains(&c)
                        || ('\u{30A0}'..='\u{30FF}').contains(&c)
                });
                if has_japanese
                    && !lemma.chars().last().is_some_and(|c| u_row.contains(&c))
                    && !lemma.ends_with("だ")
                    && lemma != "て"
                    && lemma != "た"
                {
                    reasons.push(format!(
                        "'{text}' (VERB) has lemma '{lemma}' which doesn't end in dictionary form — should end in う-row kana"
                    ));
                }
            }

            // --- AUX checks ---
            if token.pos == PartOfSpeechTag::Aux {
                let lemma = &token.lemma;
                // Copula lemma must be だ
                if (text == "です" || text == "でした") && lemma != "だ" {
                    reasons.push(format!(
                        "'{text}' (AUX) has lemma '{lemma}' — copula lemma should always be 'だ'"
                    ));
                }
                if (text == "ます" || text == "ました" || text == "ません") && lemma != "ます"
                {
                    reasons.push(format!(
                        "'{text}' (AUX) has lemma '{lemma}' — should be 'ます'"
                    ));
                }
            }

            // --- ます: always AUX ---
            if (text == "ます" || text == "ました" || text == "ません" || text == "ませんでした")
                && token.pos != PartOfSpeechTag::Aux
            {
                reasons.push(format!(
                    "'{}' tagged as {:?} — verify: ます and its forms are typically AUX (politeness suffix), not standalone verbs.",
                    text, token.pos
                ));
            }

            // --- ない: ADJ vs AUX ---
            if (text == "ない" || token.lemma == "ない")
                && token.pos != PartOfSpeechTag::Aux
                && token.pos != PartOfSpeechTag::Adj
            {
                reasons.push(format!(
                    "'ない' tagged as {:?} — should be ADJ (nonexistent: 時間がない) or AUX (negation: 食べない)", token.pos
                ));
            }

            let common_names = [
                "トム",
                "ボブ",
                "ビル",
                "メアリー",
                "ジョン",
                "マイク",
                "スー",
                "パトリシア",
                "ジム",
                "ケン",
                "ベン",
                "サム",
                "アリス",
                "ボブ",
                "ジェーン",
                "マーク",
                "リサ",
                "ポール",
                "ジョージ",
                "トニー",
                "ケイト",
                "ナンシー",
                "ジャック",
                "ヘレン",
                "ピーター",
                "ロバート",
                "デイヴィッド",
                "エミリー",
            ];
            if common_names.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "'{text}' tagged as {pos:?} — is it actually a {pos:?} or is it a name that should be tagged PROPN? Check context.",
                    pos=token.pos
                ));
            }

            // --- Adjective checks ---
            if token.pos == PartOfSpeechTag::Adj {
                let lemma = &token.lemma;
                // na-adjective lemma must include だ
                let common_na_adj = [
                    "きれい",
                    "静か",
                    "大切",
                    "大変",
                    "元気",
                    "有名",
                    "便利",
                    "不便",
                    "親切",
                    "丁寧",
                    "簡単",
                    "複雑",
                    "重要",
                    "特別",
                    "自由",
                    "安全",
                    "危険",
                    "可能",
                    "不可能",
                    "素敵",
                    "立派",
                    "無理",
                    "大丈夫",
                    "心配",
                    "好き",
                    "嫌い",
                    "上手",
                    "下手",
                ];
                if (common_na_adj.contains(&text.as_str())
                    || common_na_adj.contains(&lemma.as_str()))
                    && !lemma.ends_with("だ")
                {
                    reasons.push(format!(
                            "'{text}' is a na-adjective — lemma should include だ ('{text}だ'). Current lemma: '{lemma}'."
                        ));
                }
            }

            // --- na-adjectives mistagged as NOUN ---
            let common_na_adjectives = [
                "きれい",
                "静か",
                "大切",
                "大変",
                "元気",
                "有名",
                "便利",
                "不便",
                "親切",
                "丁寧",
                "簡単",
                "複雑",
                "重要",
                "特別",
                "自由",
                "安全",
                "危険",
                "可能",
                "不可能",
                "素敵",
                "立派",
                "無理",
                "大丈夫",
                "心配",
                "好き",
                "嫌い",
                "上手",
                "下手",
            ];
            if token.pos == PartOfSpeechTag::Noun && common_na_adjectives.contains(&text.as_str()) {
                // Many na-adjectives genuinely function as nouns in certain constructions:
                // 人気がある (popularity exists), 危険に気づく (notice the danger), 心配をかける (cause worry)
                // Only flag when the context suggests adjectival use, not nominal use
                let next_is_noun_particle = sentence.doc.get(idx + 1).is_some_and(|n| {
                    // が/を/の/に/から/まで after the word = treating it as a noun
                    matches!(n.text.as_str(), "が" | "を" | "の" | "から" | "まで")
                });
                if next_is_noun_particle {
                    // Likely genuinely used as NOUN — don't assert it's wrong
                    reasons.push(format!(
                        "'{text}' tagged as NOUN — this word can be either NOUN or ADJ (na-adjective). Before '{}' it's likely NOUN (e.g., 人気がある = popularity exists). Please verify based on context.",
                        sentence.doc.get(idx + 1).map(|n| n.text.as_str()).unwrap_or("")
                    ));
                } else {
                    reasons.push(format!(
                        "'{text}' tagged as NOUN — this word is often a na-adjective (ADJ with lemma '{text}だ'). Check context: if it modifies a noun ('{text}な...') or is a predicate ('{text}だ'), it should be ADJ. If it's the subject/object of a verb ('{text}がある'), NOUN is correct.",
                    ));
                }
            }

            // --- na-adjectives tagged ADJ but used as NOUN ---
            // The reverse of the above: if a na-adjective word is tagged ADJ but followed
            // by が/を (case particles that mark nouns), it may be functioning as a noun.
            // e.g., 人気がある (popularity exists), 危険に気づく (notice the danger)
            if token.pos == PartOfSpeechTag::Adj && common_na_adjectives.contains(&text.as_str()) {
                let next_is_noun_particle = sentence
                    .doc
                    .get(idx + 1)
                    .is_some_and(|n| matches!(n.text.as_str(), "が" | "を" | "の" | "から"));
                if next_is_noun_particle {
                    reasons.push(format!(
                        "'{text}' tagged as ADJ but followed by '{}' — verify: when followed by が/を (case particles), this word may be functioning as a NOUN (e.g., '人気がある' = popularity exists, '危険に気づく' = notice the danger). ADJ is correct when it's a predicate ('人気だ') or modifier ('人気な').",
                        sentence.doc.get(idx + 1).map(|n| n.text.as_str()).unwrap_or("")
                    ));
                }
            }

            // --- Particle checks: case particles → ADP ---
            // Note: の is handled separately below (genitive vs nominalizer)
            let case_particles = ["は", "が", "を", "に", "へ", "も", "から", "まで", "より"];
            if case_particles.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Adp {
                reasons.push(format!(
                    "'{text}' tagged as {:?} — this is typically ADP (case/topic particle). Please verify based on context.",
                    token.pos
                ));
            }

            // --- の: genitive (ADP) vs nominalizer (SCONJ/PART) ---
            // After a verb/adj, の nominalizes the clause → should be SCONJ or PART, NOT ADP
            // After a noun, の marks genitive → ADP
            if text == "の" && idx > 0 {
                let prev = &sentence.doc[idx - 1];
                if matches!(
                    prev.pos,
                    PartOfSpeechTag::Verb | PartOfSpeechTag::Aux | PartOfSpeechTag::Adj
                ) {
                    // Nominalizer の (e.g., 食べるのが好き, 鍵を捜すのを手伝って)
                    if token.pos == PartOfSpeechTag::Adp {
                        reasons.push(format!(
                                "'の' after verb/adj '{}' — verify: when の follows a verb/adjective, it's typically a nominalizer (SCONJ or PART), not genitive (ADP). Check if this の turns the clause into a noun phrase.",
                                prev.text
                            ));
                    }
                } else {
                    // Genitive の (e.g., 猫の名前) → ADP is correct
                    if token.pos != PartOfSpeechTag::Adp {
                        reasons.push(format!(
                            "'の' after noun '{}' — verify: genitive の is typically ADP. Currently tagged {:?}.",
                            prev.text, token.pos
                        ));
                    }
                }
            }

            // で is ambiguous: case particle vs copula て-form
            if text == "で" {
                reasons.push(format!(
                    "'で' is ambiguous: ADP when case particle (学校で), or copula て-form (静かで). Current POS: {:?}", token.pos
                ));
            }

            // と is especially ambiguous
            if text == "と" {
                reasons.push(format!(
                    "'と' is ambiguous: ADP comitative/quotative/conditional, or CCONJ listing nouns. Current POS: {:?}", token.pos
                ));
            }

            // Sentence-final particles → PART
            let final_particles = ["か", "よ", "ね", "な", "わ", "ぞ", "ぜ", "さ"];
            if final_particles.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Part {
                reasons.push(format!(
                    "'{text}' tagged as {:?} — verify: sentence-final particles are typically PART. Check if this is genuinely sentence-final.",
                    token.pos
                ));
            }

            // --- Indefinite 何か/誰か vs embedded question 何|か ---
            if matches!(text.as_str(), "何" | "誰" | "どこ" | "いつ")
                && sentence.doc.get(idx + 1).is_some_and(|n| n.text == "か")
            {
                reasons.push(format!(
                    "'{text}' + 'か' — verify the boundary: the indefinite '{text}か' (something/someone/somewhere/sometime, e.g. 何か食べたい, 誰か助けて) is ONE token with lemma '{text}か'. Keep the split only when か is the question particle of an embedded question (彼が誰|か|知らない = I don't know WHO he is)."
                ));
            }

            // --- Copula だ/です: always AUX, lemma だ ---
            if (text == "だ" || text == "です" || text == "でした" || text == "だった")
                && token.pos != PartOfSpeechTag::Aux
            {
                reasons.push(format!(
                    "'{text}' tagged as {:?} — verify: copula (だ/です) should typically be AUX with lemma 'だ'. Check context.",
                    token.pos
                ));
            }

            // --- のだ/んだ explanatory mood ---
            if (text == "の" || text == "ん")
                && token.pos != PartOfSpeechTag::Adp
                && let Some(next) = sentence.doc.get(idx + 1)
                && (next.text == "だ"
                    || next.text == "です"
                    || next.text == "でした"
                    || next.text == "だった")
                && token.pos != PartOfSpeechTag::Part
                && token.pos != PartOfSpeechTag::Noun
            {
                reasons.push(format!(
                    "'{}' before '{}' is the explanatory の — should be PART or NOUN",
                    text, next.text
                ));
            }

            // --- たい baked into verb lemma ---
            // If a verb lemma ends in たい, the tokenizer merged V+たい when they should be split
            if token.pos == PartOfSpeechTag::Verb
                && token.lemma.ends_with("たい")
                && token.lemma != "たい"
            {
                reasons.push(format!(
                    "'{}' has lemma '{}' which ends in たい — verify: たい is typically a separate AUX token (e.g., '食べたい' → '食べ'(VERB) + 'たい'(AUX)). Check if this lemma incorrectly includes たい.",
                    text, token.lemma
                ));
            }

            // --- ある/いる: AUX after て-form, VERB for existence ---
            if (token.lemma == "ある" || token.lemma == "いる")
                && (token.pos == PartOfSpeechTag::Aux || token.pos == PartOfSpeechTag::Verb)
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') — AUX after て-form, VERB for existence. Current POS: {:?}",
                    text, token.lemma, token.pos
                ));
            }

            // --- て-form auxiliaries ---
            let te_form_auxiliaries = [
                ("しまう", "completion/regret"),
                ("おく", "preparation"),
                ("みる", "trying"),
                ("くる", "toward speaker"),
                ("いく", "away from speaker"),
                ("もらう", "receiving favor"),
                ("あげる", "giving favor"),
                ("くれる", "favor toward me"),
            ];
            for (lemma_form, description) in &te_form_auxiliaries {
                if token.lemma == *lemma_form
                    && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                    && idx > 0
                {
                    let prev = &sentence.doc[idx - 1];
                    if prev.text.ends_with('て') || prev.text.ends_with('で') {
                        reasons.push(format!(
                            "'{}' (lemma '{}') after て-form — auxiliary ({description}) should be AUX. Current POS: {:?}",
                            text, token.lemma, token.pos
                        ));
                    }
                }
            }

            // --- Contracted て-form direction ---
            if (text.contains("てった")
                || text.contains("ていった")
                || text.ends_with("てく")
                || text.ends_with("ていく"))
                && token.lemma.contains("くる")
            {
                reasons.push(format!(
                        "'{}' has lemma '{}' — this is ていく (going away). Lemma should contain 'いく', not 'くる'.", text, token.lemma
                    ));
            }
            if (text.contains("てきた") || text.ends_with("てくる")) && token.lemma.contains("いく")
            {
                reasons.push(format!(
                        "'{}' has lemma '{}' — this is てくる (coming toward). Lemma should contain 'くる', not 'いく'.", text, token.lemma
                    ));
            }

            // --- Causative させる / passive られる ---
            if (token.lemma == "させる" || token.lemma == "せる")
                && token.pos == PartOfSpeechTag::Verb
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') tagged VERB — causative suffix should be AUX",
                    text, token.lemma
                ));
            }
            if (token.lemma == "られる" || token.lemma == "れる")
                && token.pos == PartOfSpeechTag::Verb
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') tagged VERB — passive/potential suffix should be AUX",
                    text, token.lemma
                ));
            }

            // --- らしい: productive suffix ---
            if (token.lemma == "らしい" || text.ends_with("らしい") || text.ends_with("らしく"))
                && token.pos != PartOfSpeechTag::Aux
                && token.pos != PartOfSpeechTag::Adj
            {
                reasons.push(format!(
                    "'{}' — らしい is productive, should be AUX or ADJ, not {:?}",
                    text, token.pos
                ));
            }

            // --- Words commonly mistagged ---
            if text == "好み" && token.pos == PartOfSpeechTag::Adj {
                reasons.push(
                    "'好み' is tagged ADJ but 好み is a noun meaning 'preference/taste' (e.g., '好みの問題'). It is not a na-adjective. Should be NOUN."
                        .to_string(),
                );
            }
            if text == "みんな" && token.pos == PartOfSpeechTag::Adv {
                reasons.push(
                    "'みんな' is tagged ADV but みんな means 'everyone' — should be PRON."
                        .to_string(),
                );
            }

            // --- て (AUX) with lemma いる: impossible ---
            if text == "て" && token.pos == PartOfSpeechTag::Aux && token.lemma == "いる" {
                reasons.push(
                    "'て' (AUX) has lemma 'いる' — て alone can't stand for いる. Either this is a contraction that should have been merged into the preceding verb, or it's mistagged."
                        .to_string(),
                );
            }

            // --- ADV ending in く with lemma == text: could be i-adj adverbial ---
            // Many adverbs legitimately end in く and are not derived from i-adjectives
            // (しばらく, ごく, せっかく, ことごとく, つくづく, ようやく, とにかく, まったく, etc.).
            // But i-adjective adverbial forms (早く, 大きく) are also tagged ADV by some models
            // and need their lemma fixed to the い-form. Flag for verification.
            if token.pos == PartOfSpeechTag::Adv
                && text.ends_with("く")
                && token.lemma == text.as_str()
                && text.chars().count() >= 2
            {
                let stem = &text[..text.len() - "く".len()];
                reasons.push(format!(
                    "'{text}' (ADV) has lemma '{text}' — verify: if this is an i-adjective adverbial form (e.g., 早く from 早い, 大きく from 大きい), the lemma should be '{stem}い' and POS should be ADJ. If it's a genuine adverb (しばらく, ごく, せっかく, ようやく, ことごとく, つくづく, etc.), the current lemma is correct."
                ));
            }

            // --- 一番 is never ADJ (na-adjective) ---
            if text == "一番" && token.pos == PartOfSpeechTag::Adj {
                reasons.push(
                    "'一番' tagged ADJ — 一番 is never a na-adjective (you don't say 一番な). Before の it's NOUN (the best/number one); before an adjective it's ADV (most)."
                        .to_string(),
                );
            }

            // --- VERB containing てみ/でみ — likely a merged て-form + みる ---
            // The auxiliary verb みる after a て-form is a word of its own: やってみた
            // splits as やって|みた, never merges whole.
            if token.pos == PartOfSpeechTag::Verb
                && (text.contains("てみ") || text.contains("でみ"))
                && text != "てみ"
                && text != "でみ"
                && text != "てみた"
                && text != "でみた"
                && text != "てみたら"
                && text != "でみたら"
            {
                reasons.push(format!(
                    "'{text}' (VERB) contains てみ/でみ — potentially a merged てみる. The auxiliary verb after a て-form is its own token: 'やってみたら' → 'やって' (VERB) + 'みたら' (AUX, lemma 'みる')."
                ));
            }

            // --- 別+の or 一番+の where の is tagged AUX ---
            if (text == "別" || text == "一番") && idx + 1 < sentence.doc.len() {
                let next = &sentence.doc[idx + 1];
                if next.text == "の" && next.pos == PartOfSpeechTag::Aux {
                    reasons.push(format!(
                        "'{text}' + 'の' where の is tagged AUX — の after {text} may be a genitive ADP, rather than a copula-related auxiliary."
                    ));
                }
            }

            // --- たち/達 merged into noun or pron ---
            if matches!(token.pos, PartOfSpeechTag::Noun | PartOfSpeechTag::Pron)
                && !TATSU_COMPOUNDS.contains(&text.as_str())
            {
                for suffix in ["たち", "達"] {
                    if text.ends_with(suffix) && text.chars().count() > suffix.chars().count() {
                        let stem = &text[..text.len() - suffix.len()];
                        if !stem.is_empty() {
                            reasons.push(format!(
                                "'{text}' has {suffix} (plural) merged. Split: '{stem}' ({:?}) + '{suffix}' (PART, lemma 'たち').",
                                token.pos
                            ));
                        }
                    }
                }
            }

            // --- DET check ---
            let always_det = [
                "この",
                "その",
                "あの",
                "どの",
                "こんな",
                "そんな",
                "あんな",
                "どんな",
            ];
            if always_det.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Det {
                reasons.push(format!(
                    "'{text}' tagged as {:?} — verify: this is typically DET (always modifies a noun).",
                    token.pos
                ));
            }

            // --- こそあど demonstrative pronouns: should be PRON ---
            let demonstrative_pronouns = [
                "ここ",
                "そこ",
                "あそこ",
                "どこ", // place
                "これ",
                "それ",
                "あれ",
                "どれ", // thing
                "こちら",
                "そちら",
                "あちら",
                "どちら", // direction/polite
            ];
            if demonstrative_pronouns.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Pron
            {
                reasons.push(format!(
                    "'{text}' tagged as {:?} — verify: こそあど demonstrative pronouns (ここ/そこ/これ/それ etc.) are typically PRON.",
                    token.pos
                ));
            }

            // --- Contracted form in lemma ---
            let contracted_forms = [
                "ちゃう",
                "ちゃった",
                "じゃう",
                "じゃった",
                "とく",
                "とった",
                "とけ",
                "とける",
            ];
            for form in &contracted_forms {
                if token.lemma.contains(form) && token.lemma != *form {
                    reasons.push(format!(
                        "'{}' (lemma '{}') — lemma contains contracted form '{}'. Verify this is the actual dictionary form. Contractions: てしまう→ちゃう, でしまう→じゃう, ておく→とく.",
                        text, token.lemma, form
                    ));
                    break;
                }
            }

            // --- Short れる/られる as VERB might be AUX suffix ---
            if token.pos == PartOfSpeechTag::Verb
                && (text == "れる"
                    || text == "られる"
                    || text == "れた"
                    || text == "られた"
                    || text == "れて"
                    || text == "られて"
                    || text == "れない"
                    || text == "られない")
            {
                reasons.push(format!(
                    "'{text}' tagged as VERB — verify: when れる/られる is a passive/potential suffix, it should be AUX. VERB is correct only for standalone use (rare)."
                ));
            }

            if let Some(reason) = check_polysemous(Language::Japanese, &token.text) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // --- Bound auxiliary stranded as its own token ---
            // Post-merge, a bound auxiliary surviving alone means the merge pass found
            // no predicate to its left to absorb it into — either a neighbor's POS is
            // wrong or the split is. (食べ+ました must come out as 食べました.)
            let bound_aux_lemmas = [
                "ます",
                "た",
                "たい",
                "ない",
                "れる",
                "られる",
                "せる",
                "させる",
                "ぬ",
                "う",
                "まい",
                "すぎる",
            ];
            if token.pos == PartOfSpeechTag::Aux && bound_aux_lemmas.contains(&token.lemma.as_str())
            {
                reasons.push(format!(
                    "'{}' (AUX, lemma '{}') is a bound auxiliary stranded as its own token — it belongs inside the predicate it inflects (食べ+ました → 食べました, one VERB token). If it genuinely stands alone as a predicate (e.g. 時間がない), retag it instead (ない → ADJ).",
                    token.text, token.lemma
                ));
            }

            // --- Noun+する compound merged into one token: should split ---
            // Only when the stem is a standalone word (≥2 chars): 勉強する flags, but
            // grammaticalized する-verbs whose stem can't stand alone (対する, 関する,
            // 愛する) are single analyzer lexemes and stay whole.
            if token.pos == PartOfSpeechTag::Verb
                && token.lemma.ends_with("する")
                && token.lemma != "する"
                && token.lemma.chars().count() >= 4
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') — noun+する compounds split into noun + する-form: '確認した' → '確認' (NOUN, lemma '確認') + 'した' (VERB, lemma 'する'). The する-form itself stays whole (した, して, される, しました).",
                    token.text, token.lemma
                ));
            }

            // --- Compound verb ていく/てくる merged whole ---
            if token.pos == PartOfSpeechTag::Verb
                && (token.lemma.contains("ていく")
                    || token.lemma.contains("てくる")
                    || token.lemma.contains("ていく")
                    || token.lemma.contains("てくる"))
            {
                reasons.push(format!(
                "'{}' (lemma '{}') — ていく/てくる splits at the て-form boundary: '連れていった' → '連れて' (VERB, lemma '連れる') + 'いった' (AUX, lemma 'いく').",
                token.text, token.lemma
            ));
            }

            // Copula lemma consistency
            if (token.text == "です" || token.text == "でした") && token.lemma != "だ" {
                reasons.push(format!(
                    "'{}' has lemma '{}' — should be 'だ'.",
                    token.text, token.lemma
                ));
            }

            // て-form auxiliaries tagged VERB → should be AUX
            let te_aux_lemmas = [
                "しまう",
                "おく",
                "みる",
                "くる",
                "いく",
                "もらう",
                "あげる",
                "くれる",
            ];
            if te_aux_lemmas.contains(&token.lemma.as_str())
                && token.pos == PartOfSpeechTag::Verb
                && idx > 0
            {
                let prev_text = &tokens[idx - 1].text;
                if prev_text.ends_with('て') || prev_text.ends_with('で') {
                    reasons.push(format!(
                        "'{}' (lemma '{}') is VERB after て-form '{}' — should be AUX.",
                        token.text, token.lemma, prev_text
                    ));
                }
            }

            // --- Na-adjective tagged ADJ before が/を — likely NOUN ---
            let common_na_adjectives = [
                "きれい",
                "静か",
                "大切",
                "大変",
                "元気",
                "有名",
                "便利",
                "不便",
                "親切",
                "丁寧",
                "簡単",
                "複雑",
                "重要",
                "特別",
                "自由",
                "安全",
                "危険",
                "可能",
                "不可能",
                "素敵",
                "立派",
                "無理",
                "大丈夫",
                "心配",
                "好き",
                "嫌い",
                "上手",
                "下手",
            ];
            if token.pos == PartOfSpeechTag::Adj
                && common_na_adjectives.contains(&token.text.as_str())
            {
                let next = tokens.get(idx + 1);
                if next.is_some_and(|n| matches!(n.text.as_str(), "が" | "を" | "の")) {
                    reasons.push(format!(
                        "'{}' is tagged ADJ but followed by '{}' — when a na-adjective word is followed by が/を/の, it's functioning as a NOUN (e.g., '人気がある' = popularity exists, '必要がある' = there is a need). Please change to NOUN with lemma '{}'.",
                        token.text,
                        next.unwrap().text,
                        token.text
                    ));
                }
            }

            // --- な merged into na-adjective: should split ---
            // 綿密な → 綿密 + な, 変な → 変 + な
            if token.pos == PartOfSpeechTag::Adj && token.text.ends_with("な") {
                let stem = &token.text[..token.text.len() - "な".len()];
                // If the lemma is stem + だ (or just the stem), な was merged
                if !stem.is_empty() && (token.lemma == format!("{stem}だ") || token.lemma == stem)
                {
                    reasons.push(format!(
                        "'{}' (ADJ) — な should be a separate token: '{}' (ADJ) + 'な' (AUX/PART). The な cleanly appends to the na-adjective stem.",
                        token.text, stem
                    ));
                }
            }

            // --- て/で as conjunctive tagged ADP after verb ---
            if (token.text == "て" || token.text == "で")
                && token.pos == PartOfSpeechTag::Adp
                && idx > 0
            {
                let prev_pos = tokens[idx - 1].pos;
                if matches!(
                    prev_pos,
                    PartOfSpeechTag::Verb | PartOfSpeechTag::Aux | PartOfSpeechTag::Adj
                ) {
                    reasons.push(format!(
                        "'{}' after '{}' is tagged ADP — this is the て-form conjunctive and should be SCONJ, not ADP.",
                        token.text, tokens[idx - 1].text
                    ));
                }
            }
            if token.pos == PartOfSpeechTag::Aux {
                let valid_aux_lemmas = [
                    "だ",
                    "ます",
                    "た",
                    "ない",
                    "する",
                    "いる",
                    "ある",
                    "れる",
                    "られる",
                    "せる",
                    "させる",
                    "たい",
                    "なさる",
                    "為さる",
                    "くださる",
                    "下さる",
                    "くれる",
                    "呉れる",
                    "もらう",
                    "貰う",
                    "いく",
                    "行く",
                    "くる",
                    "来る",
                    "しまう",
                    "仕舞う",
                    "おく",
                    "置く",
                    "みる",
                    "見る",
                    "あげる",
                    "上げる",
                    "居る",
                    "有る",
                    "ほしい",
                    "欲しい",
                    "やる",
                    "遣る",
                    "いただく",
                    "頂く",
                    "おる",
                    "いい",
                    "良い",
                    "無い",
                    "出来る",
                    "できる",
                    "為る",
                    "終わる",
                    "じゃ",
                    "ず",
                    "なり",
                    "ごとし",
                    "つう",
                    "き",
                    "む",
                    "そう",
                    "らしい",
                    "みたい",
                    "みたいだ",
                    "ようだ",
                    "な",
                    "ぬ",
                    "う",
                    "よう",
                    "まい",
                    "すぎる",
                ];
                if !valid_aux_lemmas.contains(&token.lemma.as_str()) {
                    reasons.push(format!(
                        "'{}' (AUX) has lemma '{}' — not a recognized auxiliary dictionary form. Expected one of the standard auxiliary lemmas.",
                        token.text, token.lemma
                    ));
                }
            }

            // --- VERB text containing 達/たち → should split ---
            if token.pos == PartOfSpeechTag::Verb
                && (token.text.ends_with("達") || token.text.ends_with("たち"))
                && token.text.chars().count() > 1
            {
                reasons.push(format!(
                    "'{}' (VERB) ends with 達/たち — verify: this may be a noun+plural merged into the verb token. Should split if 達/たち is a plural suffix.",
                    token.text
                ));
            }

            // --- Title suffix stranded after a common noun: over-split ---
            // Titles stand alone only after proper names (田中|さん). After an ordinary
            // noun the combination is one lexicalized word: 皆さん, 神様, お客さん.
            if matches!(token.text.as_str(), "さん" | "様" | "くん" | "ちゃん") && idx > 0 {
                let prev = &tokens[idx - 1];
                if prev.pos == PartOfSpeechTag::Noun && prev.whitespace.is_empty() {
                    reasons.push(format!(
                        "'{}' + '{}' — a title suffix after a common noun is one fused word (皆さん, 神様, お客さん). Merge them into a single NOUN token — unless '{}' is actually a proper name, in which case retag it PROPN and keep the split.",
                        prev.text, token.text, prev.text
                    ));
                }
            }

            // --- Name + title merged into one token: should split ---
            if token.pos == PartOfSpeechTag::Propn {
                for suffix in ["さん", "様", "くん", "ちゃん"] {
                    if token.text.ends_with(suffix)
                        && token.text.chars().count() > suffix.chars().count()
                    {
                        reasons.push(format!(
                            "'{}' (PROPN) ends in {suffix} — a title on a proper name is its own token: 田中さん → 田中 (PROPN) + さん (NOUN).",
                            token.text
                        ));
                        break;
                    }
                }
            }

            // --- ら split from its stem: the ら plurals are single words ---
            if token.text == "ら"
                && idx > 0
                && matches!(
                    tokens[idx - 1].pos,
                    PartOfSpeechTag::Pron | PartOfSpeechTag::Noun
                )
                && tokens[idx - 1].whitespace.is_empty()
            {
                reasons.push(format!(
                    "'{}' + 'ら' — the ら plurals are single words (彼ら, 僕ら, 奴ら): merge into one token, keeping the analyzer-style lemma (彼等).",
                    tokens[idx - 1].text
                ));
            }

            // --- ADJ with だ-lemma not on known na-adjective list ---
            {
                let common_na_adjectives_check = [
                    "きれい",
                    "静か",
                    "大切",
                    "大変",
                    "元気",
                    "有名",
                    "便利",
                    "不便",
                    "親切",
                    "丁寧",
                    "簡単",
                    "複雑",
                    "重要",
                    "特別",
                    "自由",
                    "安全",
                    "危険",
                    "可能",
                    "不可能",
                    "素敵",
                    "立派",
                    "無理",
                    "大丈夫",
                    "心配",
                    "好き",
                    "嫌い",
                    "上手",
                    "下手",
                ];
                if token.pos == PartOfSpeechTag::Adj && token.lemma.ends_with("だ") {
                    let stem = &token.lemma[..token.lemma.len() - "だ".len()];
                    if !common_na_adjectives_check.contains(&stem) {
                        let next_is_na = tokens.get(idx + 1).is_some_and(|n| n.text == "な");
                        let next_is_copula = tokens.get(idx + 1).is_some_and(|n| {
                            matches!(n.text.as_str(), "だ" | "です" | "でした" | "だった")
                        });
                        if !next_is_na && !next_is_copula {
                            reasons.push(format!(
                                "'{}' tagged ADJ with lemma '{}' — this word isn't on the known na-adjective list, and it doesn't appear before な or copula. Verify this is actually a na-adjective and not a noun used predicatively (e.g., '絶品' is a noun, not a na-adjective).",
                                token.text, token.lemma
                            ));
                        }
                    }
                }
            }

            // --- て (AUX) with lemma いる: impossible ---
            if token.text == "て" && token.pos == PartOfSpeechTag::Aux && token.lemma == "いる" {
                reasons.push(
                    "'て' (AUX) has lemma 'いる' — て alone can't stand for いる. Either this is a contraction that should have been merged into the preceding verb (e.g., 寝てて → 寝て + いて), or it's mistagged."
                        .to_string(),
                );
            }

            // --- ADV ending in く with lemma == text: could be i-adj adverbial ---
            if token.pos == PartOfSpeechTag::Adv
                && token.text.ends_with("く")
                && token.lemma == token.text
                && token.text.chars().count() >= 2
            {
                let stem = &token.text[..token.text.len() - "く".len()];
                reasons.push(format!(
                    "'{}' (ADV) has lemma '{}' — verify: if this is an i-adjective adverbial form (e.g., 早く from 早い, 大きく from 大きい), the lemma should be '{stem}い' and POS should be ADJ. If it's a genuine adverb (しばらく, ごく, せっかく, ようやく, ことごとく, つくづく, etc.), the current lemma is correct.",
                    token.text, token.lemma
                ));
            }

            // --- ている/てある bracketed onto the auxiliary instead of the verb ---
            if matches!(token.pos, PartOfSpeechTag::Verb | PartOfSpeechTag::Aux)
                && (token.lemma == "いる" || token.lemma == "ある")
                && token.text.chars().count() > 2
                && (token.text.starts_with("てい")
                    || token.text.starts_with("でい")
                    || token.text.starts_with("てあ")
                    || token.text.starts_with("であ"))
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') — the て/で belongs to the preceding verb's て-form, not to いる/ある: 話しかけ+ている should be 話しかけて (VERB, lemma 話しかける) + いる (AUX, lemma いる).",
                    token.text, token.lemma
                ));
            }

            // --- あり/い (AUX, lemma ある/いる) without preceding て-form: should be VERB ---
            if token.pos == PartOfSpeechTag::Aux
                && (token.lemma == "ある" || token.lemma == "いる")
                && idx > 0
            {
                let prev = &tokens[idx - 1];
                let prev_is_te = prev.text.ends_with('て') || prev.text.ends_with('で');
                if !prev_is_te {
                    reasons.push(format!(
                        "'{}' (AUX, lemma '{}') after '{}' (not て-form) — ある/いる is AUX only after a て-form. For existence/possession (e.g., 窓がありません = there are no windows), it should be VERB. Retag as VERB.",
                        token.text, token.lemma, prev.text
                    ));
                }
            }

            // --- は + ね(られる) likely misanalyzed はねる ---
            if token.text == "ね"
                && token.pos == PartOfSpeechTag::Verb
                && token.lemma == "ねる"
                && idx > 0
            {
                let prev = &tokens[idx - 1];
                if prev.text == "は" && prev.pos == PartOfSpeechTag::Adp {
                    reasons.push(
                        "'は' + 'ね' (VERB, lemma 'ねる') — verify: は may not be a topic particle here. はねる ('to hit/run over') is a single godan verb, and は+ね could be a misanalysis (e.g., 車にはねられる = to be hit by a car, where はね is the stem of はねる, not topic は + ね)."
                            .to_string(),
                    );
                }
            }

            // --- 一番 is never a na-adjective ---
            if token.text == "一番" && token.pos == PartOfSpeechTag::Adj {
                reasons.push(
                    "一番 is not a na-adjective. Before の it's NOUN (the best/number one). Before an adjective it's ADV (most). Never ADJ."
                        .to_string(),
                );
            }

            // --- 別+の where 別 is ADJ with lemma 別だ and の is AUX ---
            if token.text == "別"
                && token.pos == PartOfSpeechTag::Adj
                && token.lemma == "別だ"
                && let Some(next) = tokens.get(idx + 1)
                && next.text == "の"
                && next.pos == PartOfSpeechTag::Aux
            {
                reasons.push(
                            "'別' (ADJ, lemma '別だ') before の (AUX) — fix: retag 別 as NOUN with lemma '別' (it's a noun meaning 'another/different', not a na-adjective here), and retag の as ADP (genitive)."
                                .to_string(),
                        );
            }

            // --- Common names still not tagged PROPN ---
            let common_names = [
                "トム",
                "ボブ",
                "ビル",
                "メアリー",
                "ジョン",
                "マイク",
                "スー",
                "ジム",
                "ケン",
                "ベン",
                "サム",
                "アリス",
                "ジェーン",
                "マーク",
                "リサ",
                "ポール",
                "ジョージ",
                "トニー",
                "ケイト",
                "ナンシー",
                "ジャック",
                "ヘレン",
                "ピーター",
                "ロバート",
                "デイヴィッド",
                "エミリー",
            ];
            if common_names.contains(&token.text.as_str()) && token.pos != PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "'{}' is tagged {:?} but is a common name — should be PROPN.",
                    token.text, token.pos
                ));
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Japanese-specific corrector.
///
/// Tokenization policy: a token is a word, not a morpheme. The corrector enforces the
/// policy mechanically in two places — `correct` fixes the NLP proposal before the LLM
/// ever sees it, and `post_corrections` re-applies the same fixes to whatever the LLM
/// returns, as a safety net.
struct JapaneseCorrector;

impl WordCorrector for JapaneseCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        let corrections = fix_japanese(&mut sentence.doc);
        CorrectionResult {
            corrected: !corrections.is_empty(),
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        fix_japanese(tokens);
    }
}

/// Hindi-specific classifier
struct HindiClassifier;

impl SentenceClassifier for HindiClassifier {
    fn classify(&self, sentence: &NlpAnalyzedSentence) -> SentenceClassification {
        let mut reasons = Vec::new();

        for (idx, token) in sentence.doc.iter().enumerate() {
            let text = &token.text;

            if token.pos == PartOfSpeechTag::Space {
                reasons.push("Contains Space token".to_string());
            }

            if token.pos == PartOfSpeechTag::Propn {
                reasons.push(format!(
                    "Contains '{text}' classified as a proper noun — subtitle data often over-classifies common words as proper nouns"
                ));
            }

            // Multiword lemma check — but skip deliberate multiword tokens (text itself has a space)
            if token.lemma.contains(' ') && !token.text.contains(' ') {
                reasons.push(format!(
                    "'{}' has lemma with space: '{}'",
                    text, token.lemma
                ));
            }

            if let Some(c) = token.text.chars().next() {
                use unicode_general_category::GeneralCategory;
                if matches!(
                    unicode_general_category::get_general_category(c),
                    GeneralCategory::SpacingMark
                        | GeneralCategory::NonspacingMark
                        | GeneralCategory::EnclosingMark
                ) {
                    reasons.push(format!(
                        "Token '{}' starts with a combining character — likely a tokenization bug (Devanagari combining marks should not be split from their base consonant)",
                        token.text
                    ));
                }
            }

            // --- Verb lemmatization: Hindi infinitives end in -ना ---
            if token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux {
                let lemma = &token.lemma;
                if !lemma.ends_with("ना") && !lemma.is_empty() && lemma.chars().count() > 1 {
                    reasons.push(format!(
                        "'{}' ({:?}) has lemma '{}' which doesn't end in -ना — Hindi verb lemma should be the infinitive form (e.g., खाना, जाना, करना)",
                        text, token.pos, lemma
                    ));
                }
            }

            // --- होना (honā) copula/auxiliary: AUX vs VERB ---
            let hona_forms = [
                "है",
                "हैं",
                "हूँ",
                "हो",
                "था",
                "थी",
                "थे",
                "थीं",
                "होगा",
                "होगी",
                "होंगे",
                "होंगी",
                "हुआ",
                "हुई",
                "हुए",
            ];
            if hona_forms.contains(&text.as_str())
                && (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
            {
                reasons.push(format!(
                    "'{}' (होना) can be AUX or VERB. AUX for copula ('वह शिक्षक है') and tense auxiliary ('वह खा रहा है'). VERB only for existential ('यहाँ शांति है', 'तुम्हारे पास कंबल हैं'). Current POS: {:?}",
                    text, token.pos
                ));
            }

            if token.pos == PartOfSpeechTag::Intj
                && (text.ends_with("ो") || text.ends_with("इए") || text.ends_with("इये"))
            {
                reasons.push(format!(
                    "'{text}' tagged INTJ but looks like an imperative verb form. Even when used as a discourse marker ('look...', 'come on...'), it should be VERB with infinitive lemma (e.g., देखो → देखना, चलो → चलना) so learners can connect it to the base verb."
                ));
            }

            if text == "बहुत"
                && token.pos == PartOfSpeechTag::Det
                && let Some(next) = sentence.doc.get(idx + 1)
                && next.pos == PartOfSpeechTag::Adj
            {
                reasons.push(
                    "'बहुत' tagged DET before ADJ — when modifying an adjective ('बहुत कठोर' = very harsh), बहुत is an intensifier and should be ADV, not DET. DET is correct when quantifying a noun ('बहुत लोग' = many people).".to_string()
                );
            }

            // --- की: ambiguous postposition vs feminine past of करना ---
            if text == "की"
                && (token.pos == PartOfSpeechTag::Adp || token.pos == PartOfSpeechTag::Aux)
            {
                reasons.push(
                    "'की' is ambiguous: ADP (postposition 'of', lemma 'की') when linking possessor to noun (e.g., 'राम की किताब'). AUX (feminine past of करना, lemma 'करना') when completing a N+करना light verb construction (e.g., 'निंदा की', 'मदद की', 'शादी की'). Test: does की follow a noun and precede punctuation/clause end with no following noun? → likely करना. Does की link a noun to a following noun? → postposition.".to_string()
                );
            }

            // --- रहा/रही/रहे progressive aspect marker ---
            // In "खा रहा है", रहा is the progressive aspect marker
            // Often mistagged as ADJ or VERB. Should be AUX (or PART in some frameworks)
            let raha_forms = ["रहा", "रही", "रहे", "रहीं"];
            if raha_forms.contains(&text.as_str()) {
                if token.pos == PartOfSpeechTag::Adj {
                    // Check if preceded by a verb stem — strong signal of progressive
                    let prev_is_verb = idx > 0
                        && matches!(
                            sentence.doc[idx - 1].pos,
                            PartOfSpeechTag::Verb | PartOfSpeechTag::Aux
                        );
                    if prev_is_verb {
                        reasons.push(format!(
                            "'{}' tagged as ADJ after verb '{}' — this is likely the progressive aspect marker (V + रहा + होना = continuous tense). Should be AUX, not ADJ.",
                            text, sentence.doc[idx - 1].text
                        ));
                    } else {
                        reasons.push(format!(
                            "'{}' tagged as {:?} — if progressive aspect marker (खा रहा है), should be AUX. If genuine adjective, ADJ is fine. Check context.",
                            text, token.pos
                        ));
                    }
                } else if token.pos == PartOfSpeechTag::Verb {
                    reasons.push(format!(
                        "'{text}' tagged as VERB — if progressive aspect marker (V + रहा + होना), should be AUX. Only VERB if standalone meaning 'to remain/stay'."
                    ));
                }
            }

            // --- Compound verb light verbs ---
            let light_verbs = [
                "जाना",
                "लेना",
                "देना",
                "डालना",
                "बैठना",
                "उठना",
                "पड़ना",
                "रखना",
                "आना",
                "चुकना",
            ];
            if (token.pos == PartOfSpeechTag::Verb || token.pos == PartOfSpeechTag::Aux)
                && light_verbs.contains(&token.lemma.as_str())
            {
                reasons.push(format!(
                    "'{}' (lemma '{}') — if this is part of a compound verb (e.g., 'खा लेना'), the light verb should be AUX. If used independently, it should be VERB. Current POS: {:?}",
                    text, token.lemma, token.pos
                ));
            }

            // --- जनता (NOUN "public") vs जानता (VERB "knows") ---
            // Common confusion: जनता is a noun, जानता is a verb form of जानना
            if text == "जनता" && token.pos == PartOfSpeechTag::Verb {
                reasons.push(
                    "'जनता' tagged as VERB — this is likely NOUN (the public/people). The verb form 'knows' is 'जानता' (from जानना). Check spelling and context.".to_string()
                );
            }
            if text == "जानता" && token.pos == PartOfSpeechTag::Noun {
                reasons.push(
                    "'जानता' tagged as NOUN — this is likely VERB (knows, from जानना). The noun 'public/people' is 'जनता'. Check spelling and context.".to_string()
                );
            }

            // --- Noun/Adj + करना: करना is light verb (AUX) ---
            // In NOUN+करना compounds (प्रशंसा करना, शिकार करना, प्रयत्न करना),
            // करना is a verbalizer and gets AUX. The noun is the lexical head.
            if token.lemma == "करना" && token.pos == PartOfSpeechTag::Verb && idx > 0 {
                let prev = &sentence.doc[idx - 1];
                if prev.pos == PartOfSpeechTag::Adj || prev.pos == PartOfSpeechTag::Noun {
                    reasons.push(format!(
                        "'{}' (करना) after '{}' ({:?}) is tagged VERB — in NOUN+करना compounds, करना is a verbalizer (AUX). The noun carries the lexical meaning.",
                        text, prev.text, prev.pos
                    ));
                }
            }

            // --- Multiword proper nouns should be single tokens ---
            // मेक्सिको नगर, भीतरी मंगोलिया, न्यू यॉर्क, etc. should be one PROPN token
            // Catches PROPN+PROPN, PROPN+NOUN, ADJ+PROPN patterns
            if token.whitespace == " "
                && let Some(next) = sentence.doc.get(idx + 1)
            {
                let both_propn = token.pos == PartOfSpeechTag::Propn
                    && (next.pos == PartOfSpeechTag::Propn || next.pos == PartOfSpeechTag::Noun);
                let adj_before_propn =
                    token.pos == PartOfSpeechTag::Adj && next.pos == PartOfSpeechTag::Propn;
                if both_propn || adj_before_propn {
                    reasons.push(format!(
                            "'{}' + '{}' — if these form a single proper noun (e.g., a place name), they should be merged into one PROPN token. Apply this consistently across all occurrences.",
                            text, next.text
                        ));
                }
            }

            // --- Simple postpositions ---
            // पर and तक are excluded — they have non-ADP uses (CCONJ/PART) handled via polysemous words.
            let simple_postpositions = ["में", "को", "से", "के", "का", "ने", "द्वारा"];
            if simple_postpositions.contains(&text.as_str()) && token.pos != PartOfSpeechTag::Adp {
                reasons.push(format!(
                    "'{}' tagged as {:?} — verify: this is typically ADP (simple postposition). Check context.",
                    text, token.pos
                ));
            }

            // --- की: ADP (possessive postposition) vs AUX (past tense of करना) ---
            if text == "की" {
                reasons.push(format!(
                    "'की' is ambiguous: ADP when possessive postposition ('मेरी बहन की किताब' = my sister's book) or AUX when feminine past tense of करना ('उसने कोशिश की' = she tried). Current POS: {:?}",
                    token.pos
                ));
            }

            // --- Compound postpositions: nouns that function as postpositions after के/की ---
            // These legitimately can be NOUN standalone, but after के/की they're part of compound postpositions
            let compound_postposition_nouns = [
                "लिए",
                "साथ",
                "बारे",
                "बाद",
                "पहले",
                "ऊपर",
                "नीचे",
                "बीच",
                "अंदर",
                "बाहर",
                "पास",
            ];
            if compound_postposition_nouns.contains(&text.as_str())
                && token.pos != PartOfSpeechTag::Adp
                && token.pos != PartOfSpeechTag::Noun
            {
                reasons.push(format!(
                    "'{}' tagged as {:?} — should be ADP (compound postposition, e.g., 'के {}') or NOUN",
                    text, token.pos, text
                ));
            }

            // --- Oblique noun form lemmatization ---
            // Hindi nouns change form before postpositions (लड़का → लड़के को)
            // If the lemma matches the surface form and ends in oblique markers, it may be unlemmatized
            if token.pos == PartOfSpeechTag::Noun {
                let lemma = &token.lemma;
                // Masculine nouns ending in -ा take oblique -े (लड़का → लड़के)
                // If lemma ends in -े and equals text, the pipeline may have failed to lemmatize
                if lemma == text && text.ends_with("े") && text.chars().count() > 2 {
                    reasons.push(format!(
                        "Noun '{text}' has itself as lemma but ends in -े — this may be an oblique form. Check if the lemma should be the direct form (e.g., 'लड़के' → lemma 'लड़का', 'घरों' → lemma 'घर')"
                    ));
                }
                // Plural oblique -ों
                if lemma == text && text.ends_with("ों") {
                    reasons.push(format!(
                        "Noun '{text}' has itself as lemma but ends in -ों (plural oblique) — lemma should be the singular direct form (e.g., 'लड़कों' → lemma 'लड़का')"
                    ));
                }
            }

            // --- वाला/वाली/वाले: adjective-former, near-future, relative marker ---
            let vala_forms = ["वाला", "वाली", "वाले"];
            if vala_forms.contains(&text.as_str()) {
                reasons.push(format!(
                    "'{}' is multifunctional: ADJ when forming adjectives (दूध वाला = the milk one), PART/AUX when marking near-future (जाने वाला है = is about to go), DET when specifying (वह वाला = that one). Current POS: {:?}. Please tag based on context.",
                    text, token.pos
                ));
            }

            // --- ही/भी/तो: focus/emphasis particles, should be PART ---
            if text == "ही" && token.pos != PartOfSpeechTag::Part {
                reasons.push(format!(
                    "'ही' tagged as {:?} but is a focus particle meaning 'only/very/emphasis' — should be PART",
                    token.pos
                ));
            }
            if text == "भी" && token.pos != PartOfSpeechTag::Part {
                reasons.push(format!(
                    "'भी' tagged as {:?} but is a focus particle meaning 'also/even' — should be PART",
                    token.pos
                ));
            }
            if text == "तो"
                && token.pos != PartOfSpeechTag::Part
                && token.pos != PartOfSpeechTag::Cconj
            {
                reasons.push(format!(
                    "'तो' tagged as {:?} but is typically PART (emphasis/then) or CCONJ (then/so)",
                    token.pos
                ));
            }

            // --- नहीं/न/मत negation should be ADV or PART ---
            // नहीं/न/मत: standardize on ADV
            if (text == "नहीं" || text == "न" || text == "मत") && token.pos != PartOfSpeechTag::Adv
            {
                reasons.push(format!(
                    "'{}' tagged as {:?} — negation words should be ADV consistently",
                    text, token.pos
                ));
            }

            // --- DET/PRON ambiguity ---
            let det_or_pron = [
                "यह",
                "वह",
                "ये",
                "वे",
                "कोई",
                "कुछ",
                "सब",
                "हर",
                "इस",
                "उस",
                "इन",
                "उन",
                "मेरा",
                "मेरी",
                "मेरे",
                "तेरा",
                "तेरी",
                "तेरे",
                "उसका",
                "उसकी",
                "उसके",
            ];
            if det_or_pron.contains(&text.as_str())
                && (token.pos == PartOfSpeechTag::Det || token.pos == PartOfSpeechTag::Pron)
            {
                reasons.push(format!(
                    "'{text}' can be either DET or PRON depending on context (modifies noun → DET, stands alone → PRON)"
                ));
            }

            // --- मेरी/मेरे + को: possibly PROPN (Mary/Marie), not possessive ---
            // When मेरी or similar possessive-looking forms are followed by को,
            // it's likely a proper noun (मैरी को = to Mary), not a possessive pronoun.
            // Possessives don't take को — you'd say मुझको, not मेरी को.
            if (text == "मेरी" || text == "मेरे")
                && token.pos == PartOfSpeechTag::Pron
                && let Some(next) = sentence.doc.get(idx + 1)
                && (next.text == "को" || next.text == "ने" || next.text == "से")
            {
                reasons.push(format!(
                            "'{}' tagged as PRON but is followed by '{}' — possessive pronouns don't take direct postpositions. This is likely PROPN (a name like मैरी/Mary). If it's a pronoun meaning 'me', the form should be मुझे/मुझको, not मेरी को.",
                            text, next.text
                        ));
            }

            // --- Compound postposition consistency ---
            // के लिए / के लिये should be consistently two tokens: के (ADP) + लिए (ADP)
            // Similarly: के साथ, के बारे में, के बाद, etc.
            if text == "के"
                && token.pos == PartOfSpeechTag::Adp
                && let Some(next) = sentence.doc.get(idx + 1)
            {
                let compound_parts = [
                    "लिए",
                    "लिये",
                    "साथ",
                    "बारे",
                    "बाद",
                    "पहले",
                    "अंदर",
                    "बाहर",
                    "ऊपर",
                    "नीचे",
                    "बीच",
                    "पास",
                    "द्वारा",
                    "अलावा",
                    "बदले",
                    "बजाय",
                    "अनुसार",
                ];
                if compound_parts.contains(&next.text.as_str()) && next.pos != PartOfSpeechTag::Adp
                {
                    reasons.push(format!(
                            "'{}' after 'के' is part of a compound postposition (के {}) — should be ADP, not {:?}",
                            next.text, next.text, next.pos
                        ));
                }
            }

            // --- चाहिए: lemma should be "चाहिए", not "चाहना" ---
            if text == "चाहिए" && token.lemma == "चाहना" {
                reasons.push(
                    "'चाहिए' has lemma 'चाहना' but should have lemma 'चाहिए'. चाहिए (needed/should) is a separate dictionary entry from चाहना (to want).".to_string()
                );
            }

            // --- पहले as ADV: lemma should be पहला ---
            if text == "पहले" && token.pos == PartOfSpeechTag::Adv && token.lemma != "पहला"
            {
                reasons.push(format!(
                    "'पहले' (ADV) has lemma '{}' — if this derives from the adjective पहला (first), the lemma should be 'पहला'",
                    token.lemma
                ));
            }

            // --- एक: DET (indefinite article) vs NUM (the number one) ---
            if text == "एक" {
                reasons.push(format!(
                    "'एक' can be DET (indefinite article, 'एक लड़का आया' = a boy came) or NUM (number one, 'सिर्फ़ एक बचा' = only one remains). Current POS: {:?}",
                    token.pos
                ));
            }

            // --- वह/यह tagged CCONJ: almost certainly wrong ---
            if (text == "वह" || text == "यह" || text == "जो") && token.pos == PartOfSpeechTag::Cconj
            {
                reasons.push(format!(
                    "'{text}' tagged as CCONJ but is a pronoun (PRON). In correlative constructions ('जो चढ़ेगा वह गिरेगा'), वह is the subject pronoun, not a conjunction."
                ));
            }

            // Check polysemous words
            if let Some(reason) = check_polysemous(Language::Hindi, &token.text) {
                reasons.push(reason);
            }
        }

        if reasons.is_empty() {
            SentenceClassification::Unknown
        } else {
            SentenceClassification::Suspicious { reasons }
        }
    }

    fn needs_double_check(
        &self,
        _sentence: &str,
        tokens: &[SimplifiedTokenPrime],
    ) -> Option<Vec<String>> {
        let mut reasons = Vec::new();

        for (idx, token) in tokens.iter().enumerate() {
            // होना tagged VERB when preceded by noun/adj → likely copula, should be AUX
            // Convention: copular and tense-auxiliary होना = AUX, existential होना = VERB
            let hona_forms = [
                "है",
                "हैं",
                "हूँ",
                "हो",
                "था",
                "थी",
                "थे",
                "थीं",
                "होगा",
                "होगी",
                "होंगे",
                "होंगी",
            ];
            if hona_forms.contains(&token.text.as_str()) && token.pos == PartOfSpeechTag::Verb {
                // Walk backwards past ADV tokens to find the real predicate
                let mut check_idx = idx;
                while check_idx > 0 {
                    check_idx -= 1;
                    let prev = &tokens[check_idx];
                    if prev.pos == PartOfSpeechTag::Adv {
                        continue; // skip adverbs
                    }
                    // If preceded by noun/adj, this is copular होना → should be AUX
                    if prev.pos == PartOfSpeechTag::Noun || prev.pos == PartOfSpeechTag::Adj {
                        reasons.push(format!(
                            "'{}' (होना) is tagged VERB after '{}' ({:?}) — if this is copular (linking subject to predicate), it should be AUX, not VERB. होना is only VERB for existential use (शांति है, तुम्हारे पास कंबल हैं).",
                            token.text, prev.text, prev.pos
                        ));
                    }
                    break;
                }
            }

            // रहा tagged ADJ after a verb → should be AUX
            let raha_forms = ["रहा", "रही", "रहे", "रहीं"];
            if raha_forms.contains(&token.text.as_str())
                && token.pos == PartOfSpeechTag::Adj
                && idx > 0
                && matches!(
                    tokens[idx - 1].pos,
                    PartOfSpeechTag::Verb | PartOfSpeechTag::Aux
                )
            {
                reasons.push(format!(
                    "'{}' is tagged ADJ after verb '{}' — this is the progressive aspect marker, should be AUX.",
                    token.text, tokens[idx - 1].text
                ));
            }

            // --- जनता (NOUN) in verb position → likely जानता (VERB) ---
            // "कौन जनता है" should be "कौन जानता है" (who knows).
            // जनता as NOUN between an interrogative/pronoun and है is almost certainly
            // a mistagged जानता.
            if token.text == "जनता" && token.pos == PartOfSpeechTag::Noun {
                let next_is_hona = tokens
                    .get(idx + 1)
                    .is_some_and(|t| ["है", "हैं", "था", "थी", "थे"].contains(&t.text.as_str()));
                let prev_is_pronoun = idx > 0
                    && matches!(
                        tokens[idx - 1].pos,
                        PartOfSpeechTag::Pron | PartOfSpeechTag::Noun
                    );
                if next_is_hona && prev_is_pronoun {
                    reasons.push(format!(
                        "'जनता' tagged NOUN between '{}' and '{}' — this is likely the verb जानता (knows, lemma जानना), not the noun जनता (the public). Check context.",
                        tokens[idx - 1].text,
                        tokens[idx + 1].text
                    ));
                }
            }

            // --- Stranded aspectual auxiliary ---
            // These lemmas are AUX only as vector verbs after a verb stem
            // (खा लिया, जा सकते, कर रहा). AUX with no VERB/AUX host to the left
            // usually means a main-verb use was mistagged (standalone जाओ, or
            // motion गया after a noun). देना/लेना/करना/होना are excluded: they
            // are legitimately AUX after nouns (conjunct verbs, copula).
            let vector_only_lemmas = [
                "जाना",
                "सकना",
                "चुकना",
                "पड़ना",
                "रहना",
                "डालना",
                "बैठना",
                "उठना",
            ];
            if token.pos == PartOfSpeechTag::Aux
                && vector_only_lemmas.contains(&token.lemma.as_str())
            {
                let host = tokens[..idx]
                    .iter()
                    .rev()
                    .find(|t| !is_hindi_aux_intervener(&t.text));
                let has_verbal_host = host
                    .is_some_and(|t| matches!(t.pos, PartOfSpeechTag::Verb | PartOfSpeechTag::Aux));
                if !has_verbal_host {
                    reasons.push(format!(
                        "'{}' (lemma '{}') is tagged AUX but has no verb to its left — aspectual auxiliaries attach to a verb stem (खा लिया, जा सकते). If it is the main verb here (e.g. imperative or motion), it should be VERB.",
                        token.text, token.lemma
                    ));
                }
            }

            // --- करना tagged VERB right after a NOUN/ADJ ---
            // Usually a conjunct verb where policy wants AUX (मदद करो, खत्म कर).
            // Interrogatives/demonstratives (कैसा कर रहा है, ऐसा करो) and
            // temporal nouns (कल करने वाला) are real main-verb uses, so this is
            // a flag for review, not a hard fix.
            if token.lemma == "करना" && token.pos == PartOfSpeechTag::Verb && idx > 0 {
                let prev = &tokens[idx - 1];
                let prev_is_non_conjunct = matches!(
                    prev.text.as_str(),
                    "कैसा"
                        | "कैसी"
                        | "कैसे"
                        | "ऐसा"
                        | "ऐसी"
                        | "ऐसे"
                        | "वैसा"
                        | "वैसी"
                        | "वैसे"
                        | "क्या"
                        | "कुछ"
                        | "सब"
                        | "कल"
                        | "आज"
                        | "परसों"
                );
                if matches!(prev.pos, PartOfSpeechTag::Noun | PartOfSpeechTag::Adj)
                    && !prev_is_non_conjunct
                {
                    reasons.push(format!(
                        "'{}' (करना) tagged VERB after '{}' ({:?}) — if '{} {}' is a conjunct verb (the noun/adjective carries the meaning), करना is the verbalizer and should be AUX. Keep VERB only if '{}' is a real object or modifier of an independent करना.",
                        token.text, prev.text, prev.pos, prev.text, token.text, prev.text
                    ));
                }
            }

            // --- Adjacent NOUN/ADJ tokens that should be a single compound token ---
            // काला बाज़ार, भीतरी मंगोलिया, etc.
            // If a content word is tagged AUX/VERB but is clearly a noun (बाज़ार as AUX),
            // flag it. Also flag adjacent same-POS tokens that might need merging.
            if token.pos == PartOfSpeechTag::Aux || token.pos == PartOfSpeechTag::Verb {
                // Check if this looks like a noun that got mistagged
                let common_nouns = ["बाज़ार", "नगर", "मिर्च", "भूगोल", "मंगोलिया", "देश", "शहर"];
                if common_nouns.contains(&token.text.as_str()) {
                    reasons.push(format!(
                        "'{}' tagged as {:?} but this is a common noun — check if it should be NOUN (possibly part of a compound like काला बाज़ार, शिमला मिर्च).",
                        token.text, token.pos
                    ));
                }
            }
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons)
        }
    }
}

/// Hindi-specific corrector
struct HindiCorrector;

/// `TokenView` (from generate-data's token_corrections) over the LLM-output token
/// type, so the shared deterministic fixes apply to it like every other pipeline.
impl TokenView for SimplifiedTokenPrime {
    fn text(&self) -> &str {
        &self.text
    }
    fn whitespace(&self) -> &str {
        &self.whitespace
    }
    fn pos(&self) -> PartOfSpeechTag {
        self.pos
    }
    fn lemma(&self) -> &str {
        &self.lemma
    }
    fn push_text(&mut self, more: &str) {
        self.text.push_str(more);
    }
    fn set_text(&mut self, text: String) {
        self.text = text;
    }
    fn set_whitespace(&mut self, ws: String) {
        self.whitespace = ws;
    }
    fn set_pos(&mut self, pos: PartOfSpeechTag) {
        self.pos = pos;
    }
    fn set_lemma(&mut self, lemma: String) {
        self.lemma = lemma;
    }
}

impl WordCorrector for HindiCorrector {
    fn correct(&self, sentence: &mut NlpAnalyzedSentence) -> CorrectionResult {
        // Spelling normalization rewrites token text, which only this pipeline may
        // do (its downstream reconstructs the sentence from the tokens) — the
        // shared fix_hindi deliberately excludes it. Run it first so the चाहिए
        // rules see the normalized spelling.
        let mut corrections = Vec::new();
        for token in &mut sentence.doc {
            corrections.extend(normalize_hindi_spelling(token));
        }
        corrections.extend(fix_hindi(&mut sentence.doc));
        CorrectionResult {
            corrected: !corrections.is_empty(),
            corrections,
        }
    }

    fn post_corrections(&self, tokens: &mut Vec<SimplifiedTokenPrime>) {
        for token in tokens.iter_mut() {
            normalize_hindi_spelling(token);
        }
        fix_hindi(tokens);
    }
}

/// Tamil-specific classifier
#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    #[test]
    fn test_french_elle_correction() {
        use language_utils::{DocToken, MultiwordTerms};

        let mut sentence = NlpAnalyzedSentence {
            sentence: "Elle parle".to_string(),
            multiword_terms: MultiwordTerms {
                high_confidence: vec![],
                low_confidence: vec![],
            },
            doc: vec![
                DocToken {
                    text: "Elle".to_string(),
                    whitespace: " ".to_string(),
                    pos: PartOfSpeechTag::Pron,
                    lemma: "lui".to_string(), // Wrong lemma
                    morph: BTreeMap::new(),
                },
                DocToken {
                    text: "parle".to_string(),
                    whitespace: "".to_string(),
                    pos: PartOfSpeechTag::Verb,
                    lemma: "parler".to_string(),
                    morph: BTreeMap::new(),
                },
            ],
        };

        let corrector = FrenchCorrector;
        let result = corrector.correct(&mut sentence);

        assert!(result.corrected);
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(sentence.doc[0].lemma, "elle");
    }

    fn jpn_token(text: &str, pos: PartOfSpeechTag, lemma: &str) -> SimplifiedTokenPrime {
        SimplifiedTokenPrime {
            text: text.to_string(),
            whitespace: String::new(),
            pos,
            lemma: lemma.to_string(),
        }
    }

    /// Run the corrector's post_corrections over a token list and return the surfaces.
    fn jpn_merge(tokens: Vec<SimplifiedTokenPrime>) -> Vec<String> {
        let mut tokens = tokens;
        JapaneseCorrector.post_corrections(&mut tokens);
        tokens.into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn test_japanese_inflection_stays_one_word() {
        use PartOfSpeechTag::{Adj, Aux, Noun, Sconj, Verb};

        // 食べ|まし|た strands 食べ and まし, neither of which is a word
        assert_eq!(
            jpn_merge(vec![
                jpn_token("食べ", Verb, "食べる"),
                jpn_token("まし", Aux, "ます"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["食べました"]
        );

        // however many morphemes deep it runs
        assert_eq!(
            jpn_merge(vec![
                jpn_token("食べ", Verb, "食べる"),
                jpn_token("させ", Aux, "させる"),
                jpn_token("られ", Aux, "られる"),
                jpn_token("たく", Aux, "たい"),
                jpn_token("なかっ", Aux, "ない"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["食べさせられたくなかった"]
        );

        // い-adjective inflection likewise: 高かっ is not a word
        assert_eq!(
            jpn_merge(vec![
                jpn_token("高かっ", Adj, "高い"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["高かった"]
        );

        // the noun keeps the copula separate, as Korean keeps 학생|입니다
        assert_eq!(
            jpn_merge(vec![
                jpn_token("学生", Noun, "学生"),
                jpn_token("でし", Aux, "だ"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["学生", "でした"]
        );

        // て-form + auxiliary verb: both halves are real forms, so the split stays —
        // the same category Korean keeps split for serial verbs (가져|가)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("食べ", Verb, "食べる"),
                jpn_token("て", Sconj, "て"),
                jpn_token("いる", Aux, "いる"),
            ]),
            vec!["食べて", "いる"]
        );

        // noun + する compound: 勉強 is a word and した is a word
        assert_eq!(
            jpn_merge(vec![
                jpn_token("勉強", Noun, "勉強"),
                jpn_token("し", Verb, "する"),
                jpn_token("て", Sconj, "て"),
                jpn_token("いる", Aux, "いる"),
            ]),
            vec!["勉強", "して", "いる"]
        );

        // な on a na-adjective is treated like a particle
        assert_eq!(
            jpn_merge(vec![
                jpn_token("綿密", Adj, "綿密だ"),
                jpn_token("な", Aux, "な"),
            ]),
            vec!["綿密", "な"]
        );

        // the polite negative past is bound end to end
        assert_eq!(
            jpn_merge(vec![
                jpn_token("食べ", Verb, "食べる"),
                jpn_token("ませ", Aux, "ます"),
                jpn_token("ん", Aux, "ぬ"),
                jpn_token("でし", Aux, "だ"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["食べませんでした"]
        );

        // なさい attaches to a bare stem, so it merges: 見せ alone is not a word
        assert_eq!(
            jpn_merge(vec![
                jpn_token("見せ", Verb, "見せる"),
                jpn_token("なさい", Aux, "なさる"),
            ]),
            vec!["見せなさい"]
        );

        // …including when Sudachi tags なさい as a VERB (動詞/非自立可能)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("し", Verb, "為る"),
                jpn_token("なさい", Verb, "為さる"),
            ]),
            vec!["しなさい"]
        );

        // likelihood そう binds to the stem even when tagged ADJ (Sudachi's 形状詞)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("し", Verb, "為る"),
                jpn_token("そう", Adj, "そう"),
            ]),
            vec!["しそう"]
        );

        // conditional ば is an inflectional tail, not a word
        assert_eq!(
            jpn_merge(vec![
                jpn_token("食べれ", Verb, "食べる"),
                jpn_token("ば", Sconj, "ば"),
            ]),
            vec!["食べれば"]
        );

        // the voiced past of a 撥音便 verb arrives with lemma だ — inflection, not
        // the copula: 浮かん|だ would strand 浮かん
        assert_eq!(
            jpn_merge(vec![
                jpn_token("浮かん", Verb, "浮かぶ"),
                jpn_token("だ", Aux, "だ"),
            ]),
            vec!["浮かんだ"]
        );

        // てた/てない/てます are contractions of ていた/ていない/ています, so the piece
        // joins the て-form it rides on — unlike a real auxiliary word after て
        // (乗って|いた stays split)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("乗って", Verb, "乗る"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["乗ってた"]
        );
        assert_eq!(
            jpn_merge(vec![
                jpn_token("言って", Verb, "言う"),
                jpn_token("ない", Aux, "無い"),
            ]),
            vec!["言ってない"]
        );
        assert_eq!(
            jpn_merge(vec![
                jpn_token("書いて", Verb, "書く"),
                jpn_token("ます", Aux, "ます"),
            ]),
            vec!["書いてます"]
        );
        assert_eq!(
            jpn_merge(vec![
                jpn_token("乗って", Verb, "乗る"),
                jpn_token("いた", Aux, "いる"),
            ]),
            vec!["乗って", "いた"]
        );

        // じゃ carries lemma じゃ (not だ), but it is still the copula family: it must
        // not absorb into the predicate before it, and ない after it stays a word of
        // its own — 言った|じゃ|ない, never じゃない fused (whose lemma could only be
        // だ, erasing the negation)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("言った", Verb, "言う"),
                jpn_token("じゃ", Aux, "じゃ"),
                jpn_token("ない", Aux, "ない"),
            ]),
            vec!["言った", "じゃ", "ない"]
        );
    }

    #[test]
    fn test_japanese_copula_negation_stays_split() {
        use PartOfSpeechTag::{Adj, Aux, Noun};

        // ない after the copula is the standalone negation — a word of its own
        assert_eq!(
            jpn_merge(vec![
                jpn_token("学生", Noun, "学生"),
                jpn_token("じゃ", Aux, "だ"),
                jpn_token("ない", Aux, "無い"),
            ]),
            vec!["学生", "じゃ", "ない"]
        );

        // the copula て-form likewise: 静か|で|ない, not 静か|でない
        assert_eq!(
            jpn_merge(vec![
                jpn_token("静か", Adj, "静かだ"),
                jpn_token("で", Aux, "だ"),
                jpn_token("ない", Aux, "無い"),
            ]),
            vec!["静か", "で", "ない"]
        );

        // …but the negation still takes its own inflection: じゃ|なかっ|た → じゃ|なかった
        assert_eq!(
            jpn_merge(vec![
                jpn_token("じゃ", Aux, "だ"),
                jpn_token("なかっ", Aux, "ない"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["じゃ", "なかった"]
        );

        // a fused じゃない from the LLM is split back apart, keeping the surface —
        // じゃ gets the copula's identity and ない gets the negation's
        let mut tokens = vec![
            jpn_token("学生", Noun, "学生"),
            jpn_token("じゃない", Aux, "だ"),
        ];
        tokens[1].whitespace = " ".to_string();
        JapaneseCorrector.post_corrections(&mut tokens);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].text, "じゃ");
        assert_eq!(tokens[1].lemma, "だ");
        assert_eq!(tokens[1].whitespace, "");
        assert_eq!(tokens[2].text, "ない");
        assert_eq!(tokens[2].pos, Adj);
        assert_eq!(tokens[2].lemma, "無い");
        assert_eq!(tokens[2].whitespace, " ");

        // ない that already stands after the copula is retagged to the ADJ 無い
        let mut tokens = vec![
            jpn_token("学生", Noun, "学生"),
            jpn_token("じゃ", Aux, "だ"),
            jpn_token("ない", Aux, "ない"),
        ];
        JapaneseCorrector.post_corrections(&mut tokens);
        assert_eq!(tokens[2].pos, Adj);
        assert_eq!(tokens[2].lemma, "無い");

        // …while ない after anything that isn't the copula is left alone
        // (the contraction 言ってない still merges)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("言って", PartOfSpeechTag::Verb, "言う"),
                jpn_token("ない", Aux, "無い"),
            ]),
            vec!["言ってない"]
        );
    }

    #[test]
    fn test_japanese_copula_stays_a_word() {
        use PartOfSpeechTag::{Adj, Aux, Verb};

        // です after an adjective only adds politeness — the adjective is already
        // complete, so the copula stays its own token, exactly like 学生|です
        assert_eq!(
            jpn_merge(vec![
                jpn_token("静か", Adj, "静かだ"),
                jpn_token("です", Aux, "だ"),
            ]),
            vec!["静か", "です"]
        );
        assert_eq!(
            jpn_merge(vec![
                jpn_token("美しい", Adj, "美しい"),
                jpn_token("です", Aux, "だ"),
            ]),
            vec!["美しい", "です"]
        );
        assert_eq!(
            jpn_merge(vec![
                jpn_token("静か", Adj, "静かだ"),
                jpn_token("だっ", Aux, "だ"),
                jpn_token("た", Aux, "た"),
            ]),
            vec!["静か", "だった"]
        );

        // でしょう/だろう after a complete form is a word of its own (だろ+う still
        // merge with each other into one AUX token)
        assert_eq!(
            jpn_merge(vec![
                jpn_token("行く", Verb, "行く"),
                jpn_token("だろ", Aux, "だ"),
                jpn_token("う", Aux, "う"),
            ]),
            vec!["行く", "だろう"]
        );
    }

    #[test]
    fn test_japanese_sou_binds_to_stems_only() {
        use PartOfSpeechTag::{Aux, Verb};

        // likelihood そう on a bare stem: 降り is not a word, so it binds
        assert_eq!(
            jpn_merge(vec![
                jpn_token("降り", Verb, "降る"),
                jpn_token("そう", Aux, "そう"),
            ]),
            vec!["降りそう"]
        );

        // hearsay そう after a complete form: the predicate is finished, so
        // そう (and だ) stand alone
        assert_eq!(
            jpn_merge(vec![
                jpn_token("降る", Verb, "降る"),
                jpn_token("そう", Aux, "そう"),
                jpn_token("だ", Aux, "だ"),
            ]),
            vec!["降る", "そう", "だ"]
        );
    }

    #[test]
    fn test_japanese_correct_merges_the_proposal() {
        use PartOfSpeechTag::{Aux, Verb};
        use language_utils::{DocToken, MultiwordTerms};

        fn doc_token(text: &str, pos: PartOfSpeechTag, lemma: &str) -> DocToken {
            DocToken {
                text: text.to_string(),
                whitespace: String::new(),
                pos,
                lemma: lemma.to_string(),
                morph: BTreeMap::new(),
            }
        }

        // the NLP proposal itself gets merged, so the cleaning LLM is shown
        // word-level tokens instead of having its answer patched afterwards
        let mut sentence = NlpAnalyzedSentence {
            sentence: "食べました".to_string(),
            multiword_terms: MultiwordTerms {
                high_confidence: vec![],
                low_confidence: vec![],
            },
            doc: vec![
                doc_token("食べ", Verb, "食べる"),
                doc_token("まし", Aux, "ます"),
                doc_token("た", Aux, "た"),
            ],
        };

        let result = JapaneseCorrector.correct(&mut sentence);
        assert!(result.corrected);
        assert_eq!(sentence.doc.len(), 1);
        assert_eq!(sentence.doc[0].text, "食べました");
        assert_eq!(sentence.doc[0].lemma, "食べる");
        assert_eq!(sentence.doc[0].pos, Verb);
    }

    #[test]
    fn test_japanese_merge_preserves_surface_and_head() {
        use PartOfSpeechTag::{Aux, Verb};

        let mut tokens = vec![
            jpn_token("思い", Verb, "思う"),
            jpn_token("ます", Aux, "ます"),
        ];
        tokens[1].whitespace = " ".to_string();
        JapaneseCorrector.post_corrections(&mut tokens);

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "思います");
        // the head keeps its dictionary form and POS, as Korean does with 먹다
        assert_eq!(tokens[0].lemma, "思う");
        assert_eq!(tokens[0].pos, Verb);
        // trailing whitespace follows the last piece, so the surface reconstructs exactly
        assert_eq!(tokens[0].whitespace, " ");
    }

    #[test]
    fn test_japanese_merge_never_swallows_whitespace() {
        use PartOfSpeechTag::{Aux, Verb};

        // written apart in the source — merging would delete the space
        let mut tokens = vec![
            jpn_token("食べ", Verb, "食べる"),
            jpn_token("ます", Aux, "ます"),
        ];
        tokens[0].whitespace = " ".to_string();
        let before: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();
        JapaneseCorrector.post_corrections(&mut tokens);
        let after: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.text, t.whitespace))
            .collect();

        assert_eq!(tokens.len(), 2);
        assert_eq!(before, after);
    }

    fn hin_token(text: &str, pos: PartOfSpeechTag, lemma: &str) -> SimplifiedTokenPrime {
        SimplifiedTokenPrime {
            text: text.to_string(),
            whitespace: " ".to_string(),
            pos,
            lemma: lemma.to_string(),
        }
    }

    fn hin_fix(tokens: Vec<SimplifiedTokenPrime>) -> Vec<SimplifiedTokenPrime> {
        let mut tokens = tokens;
        HindiCorrector.post_corrections(&mut tokens);
        tokens
    }

    #[test]
    fn test_hindi_chahiye_pos_follows_context() {
        use PartOfSpeechTag::{Adv, Aux, Noun, Pron, Verb};

        // Need-sense: "मुझे चाय चाहिए" → चाहिए is VERB
        let fixed = hin_fix(vec![
            hin_token("मुझे", Pron, "मैं"),
            hin_token("चाय", Noun, "चाय"),
            hin_token("चाहिए", Aux, "चाहिए"),
        ]);
        assert_eq!(fixed[2].pos, Verb);

        // Deontic after infinitive: "तुम्हें जाना चाहिए" → चाहिए is AUX
        let fixed = hin_fix(vec![
            hin_token("तुम्हें", Pron, "तुम"),
            hin_token("जाना", Verb, "जाना"),
            hin_token("चाहिए", Verb, "चाहिए"),
        ]);
        assert_eq!(fixed[2].pos, Aux);

        // Negation between infinitive and चाहिए does not break the rule,
        // and the चहिए misspelling is normalized first
        let fixed = hin_fix(vec![
            hin_token("ऐसा", Pron, "ऐसा"),
            hin_token("करना", Verb, "करना"),
            hin_token("नहीं", Adv, "नहीं"),
            hin_token("चहिए", Verb, "चाहना"),
        ]);
        assert_eq!(fixed[3].text, "चाहिए");
        assert_eq!(fixed[3].lemma, "चाहिए");
        assert_eq!(fixed[3].pos, Aux);
    }

    #[test]
    fn test_hindi_theek_accha_before_lagna_is_adj() {
        use PartOfSpeechTag::{Adj, Adv, Aux, Pron, Verb};

        // "आप ठीक लग रहे हो" → ठीक is ADJ (complement of लगना)
        let fixed = hin_fix(vec![
            hin_token("आप", Pron, "आप"),
            hin_token("ठीक", Adv, "ठीक"),
            hin_token("लग", Verb, "लगना"),
            hin_token("रहे", Aux, "रहना"),
        ]);
        assert_eq!(fixed[1].pos, Adj);

        // "मुझे अच्छा नहीं लगा" → अच्छा is ADJ (negation skipped)
        let fixed = hin_fix(vec![
            hin_token("मुझे", Pron, "मैं"),
            hin_token("अच्छा", Adv, "अच्छा"),
            hin_token("नहीं", Adv, "नहीं"),
            hin_token("लगा", Verb, "लगना"),
        ]);
        assert_eq!(fixed[1].pos, Adj);

        // Manner use before another verb stays ADV: "अच्छा खेलता है"
        let fixed = hin_fix(vec![
            hin_token("अच्छा", Adv, "अच्छा"),
            hin_token("खेलता", Verb, "खेलना"),
        ]);
        assert_eq!(fixed[0].pos, Adv);
    }

    #[test]
    fn test_hindi_conjunct_adj_karna_is_aux() {
        use PartOfSpeechTag::{Adj, Aux, Pron, Verb};

        // "उसे खत्म कर दें" → कर is the verbalizer, AUX
        let fixed = hin_fix(vec![
            hin_token("उसे", Pron, "वह"),
            hin_token("खत्म", Adj, "खत्म"),
            hin_token("कर", Verb, "करना"),
            hin_token("दें", Aux, "देना"),
        ]);
        assert_eq!(fixed[2].pos, Aux);

        // "कैसा कर रहा है" — interrogative host, कर stays VERB
        let fixed = hin_fix(vec![
            hin_token("कैसा", Adj, "कैसा"),
            hin_token("कर", Verb, "करना"),
            hin_token("रहा", Aux, "रहना"),
        ]);
        assert_eq!(fixed[1].pos, Verb);
    }

    #[test]
    fn test_hindi_mechanical_lemma_fixes() {
        use PartOfSpeechTag::{Adp, Aux, Punct};

        let fixed = hin_fix(vec![
            hin_token(",", Punct, ", "),
            hin_token("के", Adp, "का"),
            hin_token("पास", Adp, "के पास"),
            hin_token("रही", Aux, "होना"),
        ]);
        assert_eq!(fixed[0].lemma, ",");
        assert_eq!(fixed[1].lemma, "के");
        assert_eq!(fixed[2].lemma, "पास");
        assert_eq!(fixed[3].lemma, "रहना");
    }

    #[test]
    fn test_hindi_stranded_aux_flagged() {
        use PartOfSpeechTag::{Aux, Noun, Pron, Verb};

        // "वह घर गया" with गया tagged AUX → stranded, flagged
        let tokens = vec![
            hin_token("वह", Pron, "वह"),
            hin_token("घर", Noun, "घर"),
            hin_token("गया", Aux, "जाना"),
        ];
        let reasons = HindiClassifier.needs_double_check("वह घर गया", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("no verb to its left"))),
            "expected stranded-aux flag, got {reasons:?}"
        );

        // "खा रहा है" — रहा has a verbal host, no stranded flag
        let tokens = vec![
            hin_token("खा", Verb, "खाना"),
            hin_token("रहा", Aux, "रहना"),
            hin_token("है", Aux, "होना"),
        ];
        let reasons = HindiClassifier.needs_double_check("खा रहा है", &tokens);
        assert!(
            !reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("no verb to its left"))),
            "unexpected stranded-aux flag: {reasons:?}"
        );
    }

    #[test]
    fn test_hindi_karna_verb_after_noun_flagged() {
        use PartOfSpeechTag::{Noun, Verb};

        let tokens = vec![
            hin_token("मदद", Noun, "मदद"),
            hin_token("करो", Verb, "करना"),
        ];
        let reasons = HindiClassifier.needs_double_check("मदद करो", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("conjunct"))),
            "expected conjunct-verb flag, got {reasons:?}"
        );
    }

    fn zh_token(text: &str, pos: PartOfSpeechTag) -> SimplifiedTokenPrime {
        SimplifiedTokenPrime {
            text: text.to_string(),
            whitespace: String::new(),
            pos,
            lemma: text.to_string(),
        }
    }

    fn zh_fix(tokens: Vec<SimplifiedTokenPrime>) -> Vec<SimplifiedTokenPrime> {
        let mut tokens = tokens;
        ChineseCorrector.post_corrections(&mut tokens);
        tokens
    }

    #[test]
    fn test_chinese_lemma_is_surface_form() {
        use PartOfSpeechTag::{Punct, Verb};

        let mut tokens = vec![zh_token("吃", Verb), zh_token("。", Punct)];
        tokens[0].lemma = "吃饭".to_string();
        let fixed = zh_fix(tokens);
        assert_eq!(fixed[0].lemma, "吃");
    }

    #[test]
    fn test_chinese_modal_pos_follows_context() {
        use PartOfSpeechTag::{Adv, Aux, Noun, Pron, Punct, Verb};

        // Modal before a verb → AUX: 我会说中文
        let fixed = zh_fix(vec![
            zh_token("我", Pron),
            zh_token("会", Verb),
            zh_token("说", Verb),
            zh_token("中文", Noun),
        ]);
        assert_eq!(fixed[1].pos, Aux);

        // Negation between modal and verb is skipped: 你应该不去
        let fixed = zh_fix(vec![
            zh_token("你", Pron),
            zh_token("应该", Verb),
            zh_token("不", Adv),
            zh_token("去", Verb),
        ]);
        assert_eq!(fixed[1].pos, Aux);

        // Standalone modal (sentence-final after negation) → VERB: 我不会。
        let fixed = zh_fix(vec![
            zh_token("我", Pron),
            zh_token("不", Adv),
            zh_token("会", Aux),
            zh_token("。", Punct),
        ]);
        assert_eq!(fixed[2].pos, Verb);

        // Modal before a noun phrase is left alone (the governed verb may sit
        // further right, e.g. behind a 把-phrase): 我要这个
        let fixed = zh_fix(vec![
            zh_token("我", Pron),
            zh_token("要", Aux),
            zh_token("这个", Pron),
        ]);
        assert_eq!(fixed[1].pos, Aux);
    }

    #[test]
    fn test_chinese_particle_enforcement() {
        use PartOfSpeechTag::{Noun, Part, Pron, Verb};

        // 的 and standalone 了 are always PART
        let fixed = zh_fix(vec![
            zh_token("我", Pron),
            zh_token("的", Verb),
            zh_token("书", Noun),
        ]);
        assert_eq!(fixed[1].pos, Part);

        let fixed = zh_fix(vec![zh_token("吃", Verb), zh_token("了", Verb)]);
        assert_eq!(fixed[1].pos, Part);
    }

    #[test]
    fn test_chinese_stranded_particles_flagged() {
        use PartOfSpeechTag::{Aux, Noun, Part, Pron, Verb};

        // 得 tagged PART with no predicate before it
        let tokens = vec![
            zh_token("我", Pron),
            zh_token("得", Part),
            zh_token("走", Verb),
        ];
        let reasons = ChineseClassifier.needs_double_check("我得走", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("complement marker"))),
            "expected 得-PART flag, got {reasons:?}"
        );

        // Modal AUX with no verb anywhere after it
        let tokens = vec![
            zh_token("我", Pron),
            zh_token("要", Aux),
            zh_token("水", Noun),
        ];
        let reasons = ChineseClassifier.needs_double_check("我要水", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("no verb follows"))),
            "expected stranded-modal flag, got {reasons:?}"
        );

        // A well-formed 跑得快 is not flagged for 得
        let tokens = vec![
            zh_token("跑", Verb),
            zh_token("得", Part),
            zh_token("快", PartOfSpeechTag::Adj),
        ];
        let reasons = ChineseClassifier.needs_double_check("跑得快", &tokens);
        assert!(
            !reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("complement marker"))),
            "unexpected 得 flag: {reasons:?}"
        );
    }

    fn th_token(text: &str, pos: PartOfSpeechTag) -> SimplifiedTokenPrime {
        SimplifiedTokenPrime {
            text: text.to_string(),
            whitespace: String::new(),
            pos,
            lemma: text.to_string(),
        }
    }

    fn th_fix(tokens: Vec<SimplifiedTokenPrime>) -> Vec<SimplifiedTokenPrime> {
        let mut tokens = tokens;
        ThaiCorrector.post_corrections(&mut tokens);
        tokens
    }

    #[test]
    fn test_thai_lemma_is_surface_form() {
        use PartOfSpeechTag::Verb;

        // Colloquial spellings keep their own spelling as lemma
        let mut tokens = vec![th_token("กิน", Verb)];
        tokens[0].lemma = "กินข้าว".to_string();
        let fixed = th_fix(tokens);
        assert_eq!(fixed[0].lemma, "กิน");
    }

    #[test]
    fn test_thai_particle_enforcement() {
        use PartOfSpeechTag::{Adv, Noun, Part, Pron, Verb};

        // ไม่ and ครับ are always PART
        let fixed = th_fix(vec![
            th_token("ผม", Pron),
            th_token("ไม่", Adv),
            th_token("ไป", Verb),
            th_token("ครับ", Noun),
        ]);
        assert_eq!(fixed[1].pos, Part);
        assert_eq!(fixed[3].pos, Part);

        // Sentence-final question particle → PART, even with a politeness
        // particle after it
        let fixed = th_fix(vec![
            th_token("ไป", Verb),
            th_token("ไหม", Noun),
            th_token("ครับ", Part),
        ]);
        assert_eq!(fixed[1].pos, Part);

        // Mid-sentence ไหม (silk) is left alone
        let fixed = th_fix(vec![
            th_token("ผ้า", Noun),
            th_token("ไหม", Noun),
            th_token("สวย", PartOfSpeechTag::Adj),
        ]);
        assert_eq!(fixed[1].pos, Noun);
    }

    #[test]
    fn test_thai_preverbal_marker_pos_follows_context() {
        use PartOfSpeechTag::{Aux, Pron, Verb};

        // จะ before a verb → AUX: ผมจะไป
        let fixed = th_fix(vec![
            th_token("ผม", Pron),
            th_token("จะ", Verb),
            th_token("ไป", Verb),
        ]);
        assert_eq!(fixed[1].pos, Aux);

        // Negation (ไม่ → PART) between marker and verb is skipped: ฉันจะไม่ไป
        let fixed = th_fix(vec![
            th_token("ฉัน", Pron),
            th_token("จะ", Verb),
            th_token("ไม่", PartOfSpeechTag::Part),
            th_token("ไป", Verb),
        ]);
        assert_eq!(fixed[1].pos, Aux);

        // A marker before a noun is left for the LLM: ต้องการ segmented as
        // ต้อง|การ would be wrong to force
        let fixed = th_fix(vec![
            th_token("ผม", Pron),
            th_token("ต้อง", Verb),
            th_token("เงิน", PartOfSpeechTag::Noun),
        ]);
        assert_eq!(fixed[1].pos, Verb);
    }

    #[test]
    fn test_thai_stranded_aux_flagged() {
        use PartOfSpeechTag::{Aux, Noun, Pron, Verb};

        // AUX marker with no verb after it
        let tokens = vec![
            th_token("ผม", Pron),
            th_token("ต้อง", Aux),
            th_token("เงิน", Noun),
        ];
        let reasons = ThaiClassifier.needs_double_check("ผมต้องเงิน", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("no verb follows"))),
            "expected stranded-marker flag, got {reasons:?}"
        );

        // Well-formed จะ AUX + verb is not flagged
        let tokens = vec![
            th_token("ผม", Pron),
            th_token("จะ", Aux),
            th_token("ไป", Verb),
        ];
        let reasons = ThaiClassifier.needs_double_check("ผมจะไป", &tokens);
        assert!(reasons.is_none(), "unexpected flags: {reasons:?}");

        // A single-character fragment is flagged
        let tokens = vec![th_token("มิ", Noun), th_token("น", Noun)];
        let reasons = ThaiClassifier.needs_double_check("มิน", &tokens);
        assert!(
            reasons
                .as_ref()
                .is_some_and(|r| r.iter().any(|s| s.contains("single Thai character"))),
            "expected fragment flag, got {reasons:?}"
        );
    }
}

/// Simplified token representation for LLM correction (without morphology)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SimplifiedToken {
    #[serde(rename = "1. text")]
    pub text: String,
    #[serde(rename = "2. whitespace")]
    pub whitespace: String,
    #[serde(rename = "3. pos")]
    pub pos: PartOfSpeechTag,
    #[serde(rename = "4. lemma")]
    pub lemma: String,
}

/// Simplified token representation for LLM correction (without morphology)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SimplifiedTokenPrime {
    pub text: String,
    pub whitespace: String,
    pub pos: PartOfSpeechTag,
    pub lemma: String,
}

impl From<SimplifiedToken> for SimplifiedTokenPrime {
    fn from(token: SimplifiedToken) -> Self {
        Self {
            text: token.text,
            whitespace: token.whitespace,
            pos: token.pos,
            lemma: token.lemma,
        }
    }
}

/// Response from the LLM for NLP sentence correction
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NlpCorrectionResponse {
    #[serde(rename = "tokens")]
    pub corrected_tokens: Vec<SimplifiedToken>,
}

/// Dependency relation types (Universal Dependencies)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum DependencyRelation {
    #[serde(rename = "acl")]
    Acl,
    #[serde(rename = "acl:relcl")]
    AclRelcl,
    #[serde(rename = "advcl")]
    Advcl,
    #[serde(rename = "advcl:relcl")]
    AdvclRelcl,
    #[serde(rename = "advmod")]
    Advmod,
    #[serde(rename = "advmod:emph")]
    AdvmodEmph,
    #[serde(rename = "advmod:lmod")]
    AdvmodLmod,
    #[serde(rename = "amod")]
    Amod,
    #[serde(rename = "appos")]
    Appos,
    #[serde(rename = "aux")]
    Aux,
    #[serde(rename = "aux:pass")]
    AuxPass,
    #[serde(rename = "case")]
    Case,
    #[serde(rename = "cc")]
    Cc,
    #[serde(rename = "cc:preconj")]
    CcPreconj,
    #[serde(rename = "ccomp")]
    Ccomp,
    #[serde(rename = "clf")]
    Clf,
    #[serde(rename = "compound")]
    Compound,
    #[serde(rename = "compound:lvc")]
    CompoundLvc,
    #[serde(rename = "compound:prt")]
    CompoundPrt,
    #[serde(rename = "compound:redup")]
    CompoundRedup,
    #[serde(rename = "compound:svc")]
    CompoundSvc,
    #[serde(rename = "conj")]
    Conj,
    #[serde(rename = "cop")]
    Cop,
    #[serde(rename = "csubj")]
    Csubj,
    #[serde(rename = "csubj:outer")]
    CsubjOuter,
    #[serde(rename = "csubj:pass")]
    CsubjPass,
    #[serde(rename = "dep")]
    Dep,
    #[serde(rename = "det")]
    Det,
    #[serde(rename = "det:numgov")]
    DetNumgov,
    #[serde(rename = "det:nummod")]
    DetNummod,
    #[serde(rename = "det:poss")]
    DetPoss,
    #[serde(rename = "discourse")]
    Discourse,
    #[serde(rename = "dislocated")]
    Dislocated,
    #[serde(rename = "expl")]
    Expl,
    #[serde(rename = "expl:impers")]
    ExplImpers,
    #[serde(rename = "expl:pass")]
    ExplPass,
    #[serde(rename = "expl:pv")]
    ExplPv,
    #[serde(rename = "fixed")]
    Fixed,
    #[serde(rename = "flat")]
    Flat,
    #[serde(rename = "flat:foreign")]
    FlatForeign,
    #[serde(rename = "flat:name")]
    FlatName,
    #[serde(rename = "goeswith")]
    Goeswith,
    #[serde(rename = "iobj")]
    Iobj,
    #[serde(rename = "list")]
    List,
    #[serde(rename = "mark")]
    Mark,
    #[serde(rename = "nmod")]
    Nmod,
    #[serde(rename = "nmod:poss")]
    NmodPoss,
    #[serde(rename = "nmod:tmod")]
    NmodTmod,
    #[serde(rename = "nsubj")]
    Nsubj,
    #[serde(rename = "nsubj:outer")]
    NsubjOuter,
    #[serde(rename = "nsubj:pass")]
    NsubjPass,
    #[serde(rename = "nummod")]
    Nummod,
    #[serde(rename = "nummod:gov")]
    NummodGov,
    #[serde(rename = "obj")]
    Obj,
    #[serde(rename = "obl")]
    Obl,
    #[serde(rename = "obl:agent")]
    OblAgent,
    #[serde(rename = "obl:arg")]
    OblArg,
    #[serde(rename = "obl:lmod")]
    OblLmod,
    #[serde(rename = "obl:tmod")]
    OblTmod,
    #[serde(rename = "orphan")]
    Orphan,
    #[serde(rename = "parataxis")]
    Parataxis,
    #[serde(rename = "punct")]
    Punct,
    #[serde(rename = "reparandum")]
    Reparandum,
    #[serde(rename = "root")]
    Root,
    #[serde(rename = "vocative")]
    Vocative,
    #[serde(rename = "xcomp")]
    Xcomp,
}

/// A single token with its dependency information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct TokenDependency {
    pub index: usize,
    pub word: String,
    pub dependency: DependencyRelation,
    pub head: usize,
}

/// Response from the LLM for dependency parsing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DependencyParseResponse {
    #[serde(rename = "1. thoughts")]
    pub thoughts: String,
    #[serde(rename = "2. dependencies")]
    pub dependencies: Vec<TokenDependency>,
}

/// Returns language-specific tips for the LLM correction prompt.
pub fn language_specific_tips(language: Language) -> &'static str {
    match language {
        Language::Korean => {
            r#"Korean-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses.

## Lemmatization

The most important rule: every verb and adjective lemma should be the dictionary form ending in -다. Please don't leave a conjugated form as the lemma. Common mistakes to watch for:
- 사랑해요 → lemma should be "사랑하다" (not "사랑해요")
- 공부해요 → lemma should be "공부하다" (not "공부해요")
- 마셨어요 → lemma should be "마시다" (not "마셨어요")
- 일어났어요 → lemma should be "일어나다" (not "일어났어요")
- 자야겠어요 → lemma should be "자다" (not "자야겠어요")
- 골랐다 → lemma should be "고르다" (not "골랐다")
- 누웠다 → lemma should be "눕다" (not "누웠다")
- 도와야 → lemma should be "돕다" (not "도와야")
- 추웠다 → lemma should be "춥다" (not "추웠다")
- 오신다 → lemma should be "오다" (not "오신다")
- 돼요 → lemma should be "되다" (not "돼요")
- 조심하세요 → lemma should be "조심하다" (not "조심하세요")
- 얘기했어 → lemma should be "얘기하다" (not "얘기했어")
- 복잡해요 → lemma should be "복잡하다" (not "복잡해요")
- 중요해요 → lemma should be "중요하다" (not "중요해요")
- 무시하면요 → lemma should be "무시하다" (not "무시하면요")
- 괴롭혔어 → lemma should be "괴롭히다" (not "괴롭혔+어" or "괴롭혔어")
- 꺼버려 → lemma should be "꺼버리다" (not "꺼버려")
- 놀려봐 → lemma should be "놀리다" (not "놀려봐")
This applies to all conjugated forms: -요 polite, -았/었 past, -겠 future, -ㅂ니까 formal question, -시 honorific, -야 informal, -는/은/ㄴ adnominal, etc.

Please don't use "+" morpheme notation in lemmas — the lemma should be a single clean dictionary form. The one exception is contracted forms (described below in the tokenization section), where "+" notation is used to show which morphemes fused together.
- 보호하다 (correct) not 보호+하+여야
- 생각하다 (correct) not 생각+하+었+는+데
- 나 (correct) not 나+는
- 톰 (correct) not 톰+에게

## Tokenization

### Particles

Particles (조사) like 은/는, 이/가, 을/를, 에, 에서, 의, 도, 만, 까지, 에게, 한테, 랑, 처럼, 라도, etc. should always be split into separate tokens from their noun or pronoun, as long as both pieces are visible in the surface form. This is critical for language learners who need to see and learn particles as independent grammatical units. All particles should be tagged ADP.
- "나는" → two tokens: "나" (PRON, lemma "나") + "는" (ADP, lemma "는")
- "사람들을" → two tokens: "사람들" (NOUN, lemma "사람") + "을" (ADP, lemma "을")
- "학교에서" → two tokens: "학교" (NOUN, lemma "학교") + "에서" (ADP, lemma "에서")
- "친구의" → two tokens: "친구" (NOUN, lemma "친구") + "의" (ADP, lemma "의")
- "톰에게" → two tokens: "톰" (PROPN, lemma "톰") + "에게" (ADP, lemma "에게")
- "학교도" → two tokens: "학교" (NOUN, lemma "학교") + "도" (ADP, lemma "도")
- "너한테" → two tokens: "너" (PRON, lemma "너") + "한테" (ADP, lemma "한테")
- "나랑" → two tokens: "나" (PRON, lemma "나") + "랑" (ADP, lemma "랑")
- "그때처럼" → two tokens: "그때" (NOUN, lemma "그때") + "처럼" (ADP, lemma "처럼")

Be mindful of ambiguous splits — use the sentence context to determine the correct tokenization. For example, "나라도" could be "나라" (country) + "도" (also), or "나" (me) + "라도" (even/at least). The surrounding sentence should make the intended meaning clear.

Note on particle variants: 이/가 subject particles and 을/를 object particles have two forms depending on whether the preceding syllable ends in a consonant or vowel. Both forms are valid particles. The lemma of the particle should be the particle itself (e.g., lemma of "는" is "는", lemma of "은" is "은").

### Uncontracted noun+particle eojeol

When a noun and particle are written together but haven't contracted, split them into two tokens as above. The pieces are already visible and no special handling is needed:
- "것은" → two tokens: "것" (NOUN, lemma "것") + "은" (ADP, lemma "은")
- "나를" → two tokens: "나" (PRON, lemma "나") + "를" (ADP, lemma "를")
- "것이" → two tokens: "것" (NOUN, lemma "것") + "이" (ADP, lemma "이")

### Contracted forms

Sometimes a noun/pronoun and particle fuse together into a new syllable that no longer visually contains the original pieces. When this happens, please keep it as one token — don't try to split it. The surface text must be preserved exactly. Use the lemma to show the components with "+" notation (this is the one place where "+" in lemmas is appropriate):
- "건" → one token: "건" (NOUN, lemma "것+은")
- "걸" → one token: "걸" (NOUN, lemma "것+을")
- "게" → one token: "게" (NOUN, lemma "것+이")
- "거" → one token: "거" (NOUN, lemma "것") — casual form of 것
- "건데" → one token: "건데" (NOUN, lemma "것+인데")
- "뭘" → one token: "뭘" (PRON, lemma "뭐+를")
- "뭔데요" → one token: "뭔데요" (PRON, lemma "뭐+인데+요")
- "난" → one token: "난" (PRON, lemma "나+는")
- "날" → one token: "날" (PRON, lemma "나+를") — when 날 means "me" (object), not the noun "day"
- "널" → one token: "널" (PRON, lemma "너+를")
- "전" → one token: "전" (PRON, lemma "저+는") — when 전 is a contraction of 저는, not the noun "before"
- "절" → one token: "절" (PRON, lemma "저+를")
- "우린" → one token: "우린" (PRON, lemma "우리+는")

Similarly for multi-syllable contracted forms:
- "이건" → one token: "이건" (PRON, lemma "이것+은")
- "이걸" → one token: "이걸" (PRON, lemma "이것+을")
- "그건" → one token: "그건" (PRON, lemma "그것+은")
- "그게" → one token: "그게" (PRON, lemma "그것+이")
- "저건" → one token: "저건" (PRON, lemma "저것+은")

The key principle: the text field must always match the original surface form. Never decompose a contraction in the text field (e.g., never output "이거" + "ㄴ" for "이건"). The morphological decomposition belongs only in the lemma.

### Verb conjugations

Conjugated verb forms should stay as one token with the dictionary form (-다) as the lemma. Don't split the stem from its endings, and don't use "+" in the lemma:
- "해요" → one token: "해요" (VERB, lemma "하다")
- "봐요" → one token: "봐요" (VERB, lemma "보다")
- "와요" → one token: "와요" (VERB, lemma "오다")
- "마셔요" → one token: "마셔요" (VERB, lemma "마시다")
- "먹었어요" → one token: "먹었어요" (VERB, lemma "먹다")
- "돼요" → one token: "돼요" (VERB, lemma "되다")
- "조심하세요" → one token: "조심하세요" (VERB, lemma "조심하다")

This applies even when vowel contraction has occurred (하+여→해, 보+아→봐, 오+아→와). The lemma is always the clean -다 dictionary form.

### Copula 이다

When 이다 is attached to a noun (입니다, 이에요, 이야, 이다, etc.), please split it into the noun + the copula as separate tokens. Tag the copula part as AUX with lemma "이다":
- "학생입니다" → two tokens: "학생" (NOUN, lemma "학생") + "입니다" (AUX, lemma "이다")
- "문제입니까" → two tokens: "문제" (NOUN, lemma "문제") + "입니까" (AUX, lemma "이다")
- "친구야" → two tokens: "친구" (NOUN, lemma "친구") + "야" (AUX, lemma "이다")
- "사실이에요" → two tokens: "사실" (NOUN, lemma "사실") + "이에요" (AUX, lemma "이다")
- "무엇입니까" → two tokens: "무엇" (PRON, lemma "무엇") + "입니까" (AUX, lemma "이다")
- "끝이다" → two tokens: "끝" (NOUN, lemma "끝") + "이다" (AUX, lemma "이다")
- "남자지" → two tokens: "남자" (NOUN, lemma "남자") + "지" (AUX, lemma "이다")
- "거야" → two tokens: "거" (NOUN, lemma "것") + "야" (AUX, lemma "이다")
- "샘이야" → two tokens: "샘" (PROPN, lemma "샘") + "이야" (AUX, lemma "이다")
- "살인범이라고" → two tokens: "살인범" (NOUN, lemma "살인범") + "이라고" (AUX, lemma "이다")

When 이다 appears as a separate word, also tag it as AUX with lemma "이다".

### Proper nouns

Proper nouns (names of people, places, etc.) should never be morphologically decomposed. Keep the name intact as one token. If a particle is attached, split only the particle:
- "톰에게" → "톰" (PROPN) + "에게" (ADP)
- "카즈토요를" → "카즈토요" (PROPN) + "를" (ADP)
- "레스트랭" → one token: "레스트랭" (PROPN, lemma "레스트랭") — not "레스트+이+랭"
- "나호코" → one token: "나호코" (PROPN, lemma "나호코") — not VERB with lemma "나호+코"
- "탄크레디" → one token: "탄크레디" (PROPN, lemma "탄크레디")

If you're unsure whether something is a proper noun, consider the context — names often appear in vocative position (calling someone), after titles (마담, 씨), or before 아/야 (informal address).

### Standalone dictionary forms and sentence fragments

Sometimes the input may be a standalone dictionary form (like "앉다", "좋아하다", "하다") rather than a sentence. In these cases, tag them with their correct POS (VERB, ADJ, etc.), not as SCONJ, CCONJ, or ADV. The lemma should be the word itself:
- "앉다" → VERB, lemma "앉다" (not ADV)
- "좋아하다" → VERB, lemma "좋아하다" (not CCONJ)
- "하다" → VERB, lemma "하다" (not SCONJ)
- "되다" → VERB, lemma "되다" (not SCONJ)
- "벗다" → VERB, lemma "벗다" (not SCONJ)

## Part of Speech

### 하다-adjectives vs 하다-verbs

Korean has descriptive verbs (형용사) that should be tagged ADJ, not VERB. These describe states or qualities rather than actions:
- ADJ: 행복하다, 필요하다, 중요하다, 유명하다, 위험하다, 안전하다, 편리하다, 깨끗하다, 복잡하다, 간단하다, 충분하다, 부족하다, 심각하다, 조용하다, 특별하다, 죄송하다, 감사하다, 안녕하다, 건강하다, 불편하다, 가능하다, 불가능하다, 친절하다, 솔직하다, 성실하다, 궁금하다
- Also ADJ: native Korean adjectives like 좋다, 나쁘다, 크다, 작다, 많다, 적다, 높다, 낮다, 길다, 짧다, 아프다, 싫다, 쉽다, 어렵다, 춥다, 덥다, 바쁘다, 예쁘다, 아름답다, 무섭다, 슬프다, 기쁘다

These are commonly mistagged as VERB. When you see a conjugated form like 복잡해요, 중요해요, 위험한, 궁금하겠소 — these are all ADJ, not VERB.

### 있다/없다

있다 is VERB when indicating existence or possession; 없다 is ADJ when indicating non-existence or lack. However, compound adjectives like 재미있다 (interesting) and 맛있다 (delicious) should be tagged ADJ as a whole. In auxiliary constructions like V-고 있다, tag 있다 as AUX.

### Auxiliary verbs

In compound verb constructions, the first verb is the main VERB, the second is AUX:
- V-고 있다: 먹고(VERB) 있다(AUX)
- V-고 싶다: 보고(VERB) 싶다(AUX) — note: 싶다 is AUX here
- V-아/어 주다: 도와(VERB) 주다(AUX)
- V-아/어 보다: 먹어(VERB) 보다(AUX)
In "V-ㄹ 수 있다": 수 is a bound noun (NOUN), not AUX.

### 어떻게 and 이렇게

어떻게 is always an adverb meaning "how" — tag as ADV, not VERB or SCONJ.
이렇게 is always an adverb meaning "like this" — tag as ADV, not SCONJ.

### 제 as determiner

제 before a noun is a possessive determiner meaning "my" (humble) — tag as DET, not ADJ or PRON:
- "제 편지" → "제" (DET, lemma "제") + "편지" (NOUN)

### 얘들아 as vocative

얘들아 is the vocative form of 얘들 (kids/guys) — tag as NOUN or INTJ, not SCONJ.

### 있잖아 as discourse marker

있잖아 ("you know" / "listen") functions as a discourse marker — tag as VERB (lemma "있다") or INTJ, not SCONJ.

### Final note

Just remember that the Spacy tokenization output that you will see is just terrible. It gets maybe 60% of tokens correct. So you will likely need to make a lot of changes."#
        }
        Language::French => {
            r#"

French-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

Past participles used as adjectives: when a past participle functions as an adjective (e.g., "fatigué", "désolé", "occupé", "terminé", "ouvert"), the lemma should be the verb infinitive, not the participle form. This is important for language learners who need to discover the connection to the source verb.
- "désolé" → lemma should be "désoler" (not "désolé")
- "fatigué" → lemma should be "fatiguer" (not "fatigué")
- "occupé" → lemma should be "occuper" (not "occupé")
- "terminé" → lemma should be "terminer" (not "terminé")
- "ouvert" → lemma should be "ouvrir" (not "ouvert")
- "reçu" → lemma should be "recevoir" (not "reçu")
- "perdu" → lemma should be "perdre" (not "perdu")

Contraction lemmas: contractions should use the contraction itself as the lemma, since these are treated as single dictionary entries for learners:
- "au" → lemma "au" (not "à le" or "à")
- "aux" → lemma "aux"
- "du" (partitive or contraction) → lemma "du" (not "de le" or "de")
- "des" → lemma "des"

Pronoun lemmas: please be consistent with pronoun lemmatization:
- Disjunctive/stressed pronouns keep their own form: "moi" → lemma "moi", "toi" → lemma "toi"
- Clitic object pronouns also keep their own form: "me"/"m'" → lemma "me", "te"/"t'" → lemma "te"
- Please don't map "me" to "se" or "moi" to "je" — keep each form as its own lemma.

## Part of Speech

"non" should consistently be tagged ADV (not PART).

être as copula or existential verb (e.g., "il est grand", "c'est bon", "il est tard") should be tagged VERB, not AUX. Only tag être as AUX when it forms compound tenses with past participles (e.g., "elle est partie")."#
        }
        Language::German => {
            r#"

German-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

The most important rule: lemmas should always be lowercase (except for nouns and proper nouns, which are capitalized in German). When a word appears at the start of a sentence, do not copy the capitalized surface form as the lemma.
- "Haben" (sentence-initial) → lemma should be "haben" (not "Haben")
- "Sei" (sentence-initial) → lemma should be "sein" (not "Sei")
- "Muss" (sentence-initial) → lemma should be "müssen" (not "Muss")
- "Meine" (sentence-initial DET) → lemma should be "mein" (not "Meine")
- "Morgen" (sentence-initial ADV) → lemma should be "morgen" (not "Morgen")
This applies to all word classes: verbs, auxiliaries, determiners, adverbs, adjectives, pronouns, etc. Only nouns and proper nouns get capitalized lemmas.

Nominalized verbs/adjectives: when a verb or adjective is used as a noun (substantivized), it should be tagged NOUN and the lemma should be the capitalized noun form, not the lowercase verb/adjective:
- "das Schweigen" → NOUN, lemma "Schweigen" (not "schweigen")
- "das Essen" → NOUN, lemma "Essen" (not "essen")
- "das Rauchen" → NOUN, lemma "Rauchen" (not "rauchen")
- "der Vorsitzende" → NOUN, lemma "Vorsitzender" (not "vorsitzend")

Contraction lemmas: German contractions should use the contraction itself as the lemma:
- "im" → lemma "im" (not "in" or "in dem")
- "zum" → lemma "zum" (not "zu" or "zu dem")
- "vom" → lemma "vom", "beim" → lemma "beim", "ins" → lemma "ins", etc.

The formal pronoun "Sie" (you, formal) should have lemma "Sie" (capitalized) to distinguish it from "sie" (she/they).

"haben" as auxiliary: please make sure the lemma is "haben" — we've seen a corrupted lemma "Haen" appear for "haben" forms. Double-check that "hast", "hat", "hatte", etc. all get lemma "haben"."#
        }
        Language::Spanish => {
            r#"

Spanish-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

Past participles used as adjectives: when a past participle functions as an adjective (e.g., "cansado", "ocupado", "cerrado"), the lemma should be the verb infinitive, not the participle form:
- "acostumbrado" → lemma should be "acostumbrar" (not "acostumbrado")
- "ocupado" → lemma should be "ocupar" (not "ocupado")
- "jubilado" → lemma should be "jubilar" (not "jubilado")
- "equivocado" → lemma should be "equivocar" (not "equivocado")
- "cerrado" → lemma should be "cerrar" (not "cerrado")

Contraction lemmas: contractions should use the contraction itself as the lemma:
- "al" → lemma "al" (not "a el" or "a")
- "del" → lemma "del" (not "de el" or "de")

Enclitic reflexive verbs: when a verb has an enclitic pronoun attached (e.g., "irse", "casarse", "quedarse"), the lemma should be the base verb without the pronoun:
- "irse" / "irnos" / "irme" → lemma "ir" (not "irse")
- "quedarse" → lemma "quedar" (not "quedarse")
- "casarse" / "casate" → lemma "casar" (not "casarse")
- "hacerlo" → lemma "hacer"
- "hablarte" → lemma "hablar"

Pronoun lemmas: clitic pronouns should keep their own form as the lemma:
- "me" → lemma "me", "te" → lemma "te", "se" → lemma "se", "le" → lemma "le"
- "conmigo" → lemma "conmigo", "contigo" → lemma "contigo"

Demonstrative pronouns: neuter demonstratives should lemmatize to the masculine form:
- "esto" → lemma "este", "eso" → lemma "ese", "aquello" → lemma "aquel"

Indefinite article: "un"/"una"/"unos"/"unas" should all lemmatize to "uno".

## Part of Speech

"no" (negation) should consistently be tagged ADV."#
        }
        Language::Italian => {
            r#"

Italian-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

Articulated preposition lemmas: this is the most important issue. Articulated prepositions (nel, del, al, sul, dal, etc.) should use the contraction itself as the lemma. Please be consistent:
- "del" → lemma "del" (not "di il", "di", or "di+il")
- "della" → lemma "della" (not "di il", "di la", "di", or "di+la")
- "al" → lemma "al" (not "a il", "a", or "a+il")
- "alla" → lemma "alla" (not "a la", "a il", or "a")
- "nel" → lemma "nel" (not "in il" or "in")
- "nella" → lemma "nella" (not "in la", "in il", or "in")
- "sul" → lemma "sul", "sulla" → lemma "sulla"
- "dal" → lemma "dal", "dalla" → lemma "dalla"
- Same for apostrophed forms: "dell'" → lemma "del", "all'" → lemma "al", "nell'" → lemma "nel"
- Partitive articles (del/dello/della/etc. as DET) should also use the contraction as lemma: "dello" → lemma "dello", "delle" → lemma "delle"

Verb+clitic lemmas: when a verb has an enclitic pronoun attached (e.g., "farlo", "dirsi", "aiutarla"), the lemma should be just the bare infinitive, not a space-separated fusion:
- "farlo" → lemma "fare" (not "fare lo")
- "dirsi" → lemma "dire" (not "dire si")
- "parlarle" → lemma "parlare" (not "parlare le")
- "ucciderlo" → lemma "uccidere" (not "uccidere lo")

Past participles used as adjectives: the lemma should be the verb infinitive:
- "impressionato" → lemma "impressionare" (not "impressionato")
- "arrabbiato" → lemma "arrabbiare" (not "arrabbiato")
- "affamato" → lemma "affamare" (not "affamato")

Clitic pronoun lemmas: please be consistent. "mi" → lemma "mi", "ti" → lemma "ti", "si" → lemma "si", "ci" → lemma "ci", "vi" → lemma "vi".

## Part of Speech

"non" should consistently be tagged ADV (not PART)."#
        }
        Language::Portuguese => {
            r#"

Portuguese-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

Contraction lemmas: this is the most important issue. Portuguese contractions should use the contraction itself as the lemma. Please be consistent and respect the gender of the contraction:
- "do" → lemma "do" (not "de o", "de", or "de+o")
- "da" → lemma "da" (not "de a", "de o", "de", or "de+a")
- "dos" → lemma "dos", "das" → lemma "das"
- "no" → lemma "no" (not "em o" or "em")
- "na" → lemma "na" (not "em a", "em o", or "em") — especially don't use a masculine lemma for a feminine contraction
- "nos" → lemma "nos", "nas" → lemma "nas"
- "ao" → lemma "ao", "aos" → lemma "aos", "à" → lemma "à", "às" → lemma "às"
- "pelo" → lemma "pelo", "pela" → lemma "pela", "pelos" → lemma "pelos", "pelas" → lemma "pelas"
- "num" → lemma "num", "numa" → lemma "numa"
- "dele"/"dela" → lemma "dele"/"dela" (not "de ele"/"de ela")
- "nele"/"nela" → lemma "nele"/"nela"

Past participles used as adjectives: the lemma should be the verb infinitive:
- "cansado" → lemma "cansar" (not "cansado")
- "ocupado" → lemma "ocupar" (not "ocupado")
- "coberto" → lemma "cobrir" (not "coberto")
- "aberto" → lemma "abrir" (not "aberto")
- "morto" → lemma "morrer" (not "morto")

Pronoun lemmas: clitic pronouns should keep their own form as the lemma:
- "me" → lemma "me" (not "eu"), "te" → lemma "te" (not "tu"), "se" → lemma "se"

Feminine noun lemmas: please be consistent. Feminine nouns that have a distinct masculine counterpart should keep the feminine form as lemma:
- "irmã" → lemma "irmã" (not "irmão"), "filha" → lemma "filha" (not "filho")
- "mãe" → lemma "mãe", "avó" → lemma "avó"

## Part of Speech

estar as copula (with adjective complement, e.g., "está bem", "estou em casa") should be tagged VERB, not AUX. Only tag estar as AUX when forming progressive tenses with gerund (e.g., "estou comendo").
saber can be either AUX or VERB: AUX when expressing ability with infinitive (e.g., "sei nadar"), VERB when expressing knowledge of facts (e.g., "sei a resposta")."#
        }
        Language::Russian => {
            r#"

Russian-specific rules — please follow these carefully, as they address systematic issues we've seen in past analyses:

## Lemmatization

Reflexive verb convention: reflexive verbs (ending in -ся/-сь) are distinct vocabulary items for learners. The lemma should keep -ся:
- "моюсь" / "моешься" / "мылся" → lemma "мыться" (not "мыть")
- "учится" / "учились" → lemma "учиться" (not "учить")
- "боюсь" / "боялся" → lemma "бояться" (not "бояться" is correct, don't strip -ся)
- "нравится" → lemma "нравиться" (not "нравить")
- "смеётся" → lemma "смеяться"

Pronoun lemmas: all pronoun forms should lemmatize to the nominative form:
- "меня" / "мне" / "мной" → lemma "я"
- "тебя" / "тебе" / "тобой" → lemma "ты"
- "его" / "ему" / "им" / "нём" → lemma "он"
- "её" / "ей" / "ею" → lemma "она"
- "нас" / "нам" / "нами" → lemma "мы"
- "вас" / "вам" / "вами" → lemma "вы"
- "их" / "им" / "ими" → lemma "они"
- "себя" / "себе" / "собой" → lemma "себя" (reflexive has no nominative)

Aspect pairs are separate lemmas: perfective and imperfective verbs are distinct dictionary entries:
- делать (impf) ≠ сделать (pf) — these are separate lemmas
- писать (impf) ≠ написать (pf)
- читать (impf) ≠ прочитать (pf)
- говорить (impf) ≠ сказать (pf)
Do NOT merge aspect pairs into one lemma. Each aspect is its own vocabulary item.

Lowercase lemmas: lemmas should always be lowercase (Russian doesn't capitalize common nouns). Only proper nouns (PROPN) get capitalized lemmas.

Ё in lemmas: preserve ё where it belongs — it's important for learners:
- "ещё" (not "еще"), "всё" (when meaning "everything", not "все")
- "берёт" → lemma "брать" (not "берет" → lemma "брать")

## Part of Speech

быть as copula or existential verb (e.g., "он был учителем", "здесь будет парк", "есть проблема") should be tagged VERB, not AUX. Only tag быть as AUX when forming passive (e.g., "был принят") or future compound tenses with imperfective infinitive (e.g., "будет петь").

Short-form adjectives (рад, готов, должен, нужен, болен, прав, жив, похож, согласен, способен, уверен, знаком, доволен) should consistently be tagged ADJ with the full long-form adjective as lemma (e.g., рад → радый, готов → готовый, должен → должный, нужен → нужный, болен → больной).

"не" (negation particle) should be tagged PART.

Short-form neuter adjectives used as predicatives: нужно (нужный), должно (должный), важно (важный), видно (видный) — tag as ADJ with the full adjective as lemma. This connects learners to the full adjective paradigm. "больно" is ambiguous: ADJ when predicative ("мне больно", lemma "больной") vs ADV when modifying a verb ("больно ударить"). можно, надо, нельзя, пора have no adjective paradigm — these are genuinely adverbial.

Participles used as adjectives: when a participle modifies a noun (e.g., "уставший человек", "написанное письмо"), tag as ADJ and use the verb infinitive as lemma. When part of a verb phrase, tag as VERB."#
        }
        Language::ChineseSimplified | Language::ChineseTraditional => {
            r#"

Chinese-specific rules — please follow these carefully. Chinese is written without spaces between words; a neural segmenter (fine-grained CTB standard) has already proposed a segmentation, and your job is to review it.

## Tokenization: a token is a word a learner can look up

Every token is shown to a learner on its own, with its own dictionary entry (think CC-CEDICT granularity). The guiding principle:

**Keep a boundary only when both sides are words a learner could be shown independently. Merge only when the pieces are not independent words — or when the combination is itself the dictionary word.**

Unlike Japanese, Chinese has no inflection to strand, so fine-grained segmentation is usually right: 都是 → 都|是, 很快 → 很|快, 不要 → 不|要 — each piece is a word, and none of those combinations is a dictionary entry. Where the segmenter keeps a compound whole (没有, 为什么, 已经, 放下), it is because the compound IS the dictionary word.

Common errors to fix:
- Over-segmenting: two adjacent single-character tokens that form one dictionary word (因|为 → 因为, 可|以 → 可以, 已|经 → 已经, 你|好 → 你好 as a greeting). If two adjacent single-character tokens have no whitespace between them and their combination is a common word used as that word here, merge them.
- Under-segmenting: two words fused into one token (他们说 → 他们|说).

### One token each (do not split)

- Greetings and formulas: 你好, 谢谢, 对不起, 没关系, 再见 — single INTJ (or VERB for 谢谢 with an object) tokens.
- Lexicalized verb compounds the dictionary lists: 放下, 起来, 回来, 出去, 得到, 看见, 听说, 觉得, 认识, 知道.
- Reduplicated verbs: 看看, 想想, 试试, 看一看, 聊聊天 — one token, lemma is the token itself.
- Erhua: 花儿, 一点儿, 一会儿 — the 儿 stays attached.
- Personal names: 张伟, 汤姆, 克里斯汀 — one PROPN token; never decompose a person's name, Chinese or transliterated. Surname and given name written together stay together (张伟, not 张|伟).
- Separable verbs (离合词) written contiguously: 见面, 睡觉, 吃饭, 帮忙 — one VERB token. When actually separated (睡了一觉, 帮个忙), the pieces are already separate tokens: label them individually (睡 VERB, 觉 NOUN).

### Never reorder the text — tokens must reproduce the sentence

Concatenating the tokens' text + whitespace in order must reproduce the sentence exactly. In particular, a separable verb with an INFIXED aspect particle stays in surface order as separate tokens:
- 结过婚 → 结|过|婚 (结 VERB, 过 PART, 婚 NOUN) — NEVER "结婚|过", which reorders the characters
- 堕过胎 → 堕|过|胎, 干过架 → 干|过|架, 做过爱 → 做|过|爱, 洗了澡 → 洗|了|澡
The urge to reunite the dictionary word 结婚 is wrong here — the sentence's own character order always wins.

Likewise, never INSERT characters the sentence doesn't contain. Colloquial elisions and contractions stay as written: 听过 stays 听|过 (do not expand to 听说过), the A-not-A contraction 知不知道 stays 知|不|知道 (do not expand to 知道不知道; tag the clipped 知 as VERB). The token texts must use exactly the sentence's characters, no more, no fewer.

### Separate tokens (do not merge)

- Number | measure word: 三|本, 一|杯, 这|个 — the measure word is its own learnable word (NOUN). Exception: 一下 is one token (a dictionary word).
- Verb | aspect particle: 吃|了, 去|过, 拿|着.
- Verb | complement 得|地|的 constructions: 跑|得|快, 慢慢|地|走.
- Adjacent verbs in serial constructions: 去|买, 想|吃.
- Institutional/geographic compound names may stay as the segmenter proposed (中华|人民|共和国 is fine — each part is a word); only person names are atomic.

## Lemmatization

Chinese words do not inflect — the lemma is always identical to the surface form of the token, for every token, including particles, punctuation, and proper nouns. Do not "normalize" 得/地/的, do not substitute synonyms, do not convert between scripts. (The pipeline enforces this deterministically — a differing lemma is discarded.)

## Part of Speech

### Structural particles (助词)
的 (attributive/genitive/nominalizing), 地 (adverbial) should be tagged PART.

得 is three-way ambiguous — handle carefully:
- PART: complement marker after a verb/adjective (e.g., 跑得快 = runs fast)
- AUX: modal "must" (pronunciation děi, e.g., 我得走了 = I must go)
- VERB: "to get/obtain" (pronunciation dé, rare standalone, common in compounds like 得到)

### Aspect particles (also PART)
了: perfective aspect (after verb: 吃了) or sentence-final change-of-state. Always PART, never VERB.
过: experiential aspect after a verb (e.g., 我去过中国 = I have been to China) → PART. But 过 as a standalone main verb meaning "to pass/cross" (e.g., 过马路) → VERB.
着: durative aspect after a verb (e.g., 开着门 = the door is open) → PART. But 着 as a standalone verb is rare in modern Chinese.

### Sentence-final particles
吗 (yes/no question), 呢 (follow-up/topic), 吧 (suggestion/uncertainty), 啊/呀/嘛/啦/哦 (tone) should be PART when sentence-final. Standing alone as an exclamation (啊！), they are INTJ.

### 在 (zài) — highly ambiguous
- VERB: locative copula (他在家 = he is at home)
- ADP: preposition (在学校学习 = study at school)
- ADV: progressive aspect marker (他在吃饭 = he is eating)
When 在 appears directly before a verb, it's typically ADV (progressive). When before a noun/place, it's ADP. When it IS the main predicate with a location, it's VERB.

### 把 and 被 — grammatical markers
把: most commonly ADP (disposal/object-fronting construction: 把书放下 = put the book down). Rarely VERB "to hold/guard". As measure word after a number/demonstrative (一把刀), it should be NOUN.
被: most commonly ADP (passive marker: 被打了 = was hit, 被老师批评了 = was criticized by the teacher). Rarely VERB "to cover" in modern Chinese.

### 是 (shì) as copula
VERB when linking subject to predicate (e.g., '他是老师'). AUX only in 是...的 focus constructions (e.g., '他是昨天来的').

### Modal verbs
会/能/能够/可以/要/想/应该/必须/敢/肯/愿意: AUX when modifying another verb (我会说中文 → 会 AUX), VERB when standalone with a direct object or sentence-final (我要这个, 我不会 → VERB). The pipeline enforces the adjacent cases deterministically — please apply the same rule when the governed verb sits further away (你应该把它放下 → 应该 AUX).

### Measure words/classifiers (量词)
个, 只, 条, 张, 本, 杯, etc. after a number or demonstrative (这/那/哪/每/几) should be tagged NOUN.

### Localizers (方位词)
Single-character localizers after a noun (桌子上, 房间里): 上/里/下/中/外 → ADP. Two-character position words (上面, 里面, 旁边, 中间) → NOUN.

### Negation
不 and 没 should be ADV (不 standing entirely alone as a reply — 不。 — is INTJ). 没有 can be ADV (negation: '没有去过') or VERB (non-possession: '我没有钱').

### Demonstratives
这/那 before a noun or measure word: DET. Standing alone: PRON.

### Proper nouns
Names of people, places, and organizations — Chinese or transliterated — are PROPN: 爱德华, 洛汗, 刚铎, 明尼苏达州, 张伟. Fictional place/people names from films count. A transliterated name is not a common NOUN just because it is unfamiliar."#
        }
        Language::Thai => {
            r#"

Thai-specific rules — please follow these carefully. Thai is written without spaces between words; a dictionary-based segmenter (PyThaiNLP newmm) has already proposed a segmentation, and your job is to review it. The POS proposal comes from a weak tagger — expect to correct tags freely.

## Tokenization: a token is a word a learner can look up

Every token is shown to a learner on its own, with its own dictionary entry. The guiding principle:

**Keep a boundary only when both sides are words a learner could be shown independently. Merge only when the pieces are not independent words — or when the combination is itself the dictionary word.**

Thai DOES use spaces — between phrases and clauses, not words. A space always goes in the preceding token's `whitespace` field, never in a token's text and never as its own token.

### Merge: fragments of unknown words

The segmenter's dictionary doesn't know transliterated names and shreds them into meaningless fragments: มินนิอาโปลิส (Minneapolis) comes out as มิ|น|นิ|อาโป|ลิ|ส. A single Thai character is almost never a real word — a run of short nonsense fragments is one word, usually a PROPN. Merge the whole run back into one token: มินนิอาโปลิส, เอ็ดเวิร์ด, เน็ตฟลิกซ์. Merge exactly the fragments that belong to the name — check against the original sentence — and never absorb the real word next to it (in ไป|มินนิอาโปลิส the ไป is its own VERB token).

### One token each (do not split)

- Greetings and formulas: สวัสดี, ขอบคุณ, ขอโทษ, ไม่เป็นไร — single tokens (the dictionary lists them; their meaning is not compositional). But a compositional greeting PHRASE stays split: สุขสันต์|วันคริสต์มาส (happy + Christmas day), ราตรี|สวัสดิ์ stays as the dictionary segments it.
- Question words and deictic locations: อะไร, ทำไม, ยังไง, อย่างไร, ที่ไหน, เมื่อไหร่, ที่นี่, ที่นั่น, ที่โน่น — one token each (do not split ที่ off of ที่นี่/ที่นั่น/ที่ไหน).
- Lexicalized compounds the dictionary lists: น้ำตา (tears), น้ำแข็ง (ice), รถไฟ (train), ไฟฟ้า (electricity), เมื่อวาน (yesterday), พรุ่งนี้ (tomorrow), หนังสือ (book).
- Lexicalized การ-/ความ- nominalizations: ความสุข (happiness), ความรัก (love), ความจริง (truth), การบ้าน (homework) — dictionary words, keep whole.
- Personal names, Thai or transliterated: one PROPN token; never decompose a person's name.
- Polysyllabic loanwords: คอมพิวเตอร์, แท็กซี่, โซฟา — one token each.
- Lexicalized โรง-/ร้าน- compounds: โรงแรม (hotel), โรงเรียน (school), ร้านอาหาร (restaurant) — dictionary words, keep whole.

### Separate tokens (do not merge)

- Compositional verb + object collocations, even when the dictionary lists them: กินข้าว → กิน|ข้าว (eat + rice), ปิดไฟ → ปิด|ไฟ (close + light), มาสาย → มา|สาย (come + late), ดูหนัง → ดู|หนัง, อาบน้ำ → อาบ|น้ำ. Each side is a word the learner should meet on its own.
- Compositional role/agent compounds: ผู้ดูแล|ระบบ (administrator + system), หัวหน้า|ทีม (head + team) — split when each side is a word and the meaning is transparent.
- The repetition mark ๆ is always its own token (PART), attached to nothing: เร็ว|ๆ, มาก|ๆ.
- Productive (non-lexicalized) การ-/ความ- nominalizations: การ|วิ่ง (the running). But when the whole is itself a dictionary word (ความเร็ว "speed"), keep it whole. When unsure whether it's lexicalized, keep the segmenter's proposal.
- Pronoun/noun sequences the segmenter glued: ผมชื่อ → ผม|ชื่อ.
- Number | classifier: สอง|คน, สาม|ตัว — the classifier is its own learnable word (NOUN).
- Serial verbs and verb chains: ไป|ซื้อ (go buy), อยาก|กิน (want to eat) — each verb (or preverbal marker) is its own token.

### Never rewrite the text

Concatenating the tokens' text + whitespace in order must reproduce the sentence exactly. Colloquial spellings stay exactly as written: เค้า (for เขา), มั้ย (for ไหม), ป่ะ, จิงๆ, งับ — never normalize them, in the text OR the lemma. Never insert, delete, or reorder characters.

## Lemmatization

Thai words do not inflect — the lemma is always identical to the surface form of the token, for every token, including colloquial spellings, particles, punctuation, and proper nouns. (The pipeline enforces this deterministically — a differing lemma is discarded.)

## Part of Speech

### Negation and particles (PART)
- ไม่ (negator) is always PART (enforced deterministically). Same for มิ.
- Politeness particles: ครับ, ค่ะ, คะ, นะ, จ้ะ, จ๊ะ, ฮะ (deterministic), plus สิ, เถอะ, หรอก, ล่ะ, น่า, เลย when used as sentence-final mood softeners → PART.
- Question particles at the end of the sentence: ไหม, มั้ย, เหรอ, หรอ, รึเปล่า (as segmented) → PART. Mid-sentence ไหม is the noun "silk".

### Preverbal markers (AUX)
จะ (irrealis/future — always AUX), กำลัง (progressive), เคย (experiential), ต้อง (must), ควร (should), อาจ (may), คง (probably), มัก (tend to), น่าจะ (ought to), ย่อม — AUX when they modify a following verb (the adjacent case is enforced deterministically; apply the same rule when the verb sits further right). อยาก (want to) before a verb → AUX. A modal standing alone as an answer (ได้! = "sure, can do") is VERB.

### ได้ — three readings, two tags
- Before a verb: AUX, past attainment (ได้ไป = got to go / did go)
- After a verb: AUX, potential (ไปได้ = can go, ทำไม่ได้ = can't do, รู้ได้ไง = how would you know)
- Main verb "get/receive" (ได้เงิน = get money) → VERB
ได้ is never ADV or anything else — AUX or VERB only.

### อยู่ and เป็น
- อยู่ as main verb "be at / live" (บ้านอยู่ไกล) → VERB; postverbal continuous marker (นั่งอยู่ = is sitting) → AUX.
- เป็น and คือ as copula → VERB (matching this dataset's convention for Chinese 是; not AUX). Postverbal เป็น "know how to" (พูดเป็น) → AUX.
- แล้ว (already / and then) → ADV.

### Classifiers (ลักษณนาม)
คน, ตัว, อัน, เล่ม, ใบ, แก้ว, ครั้ง, ที, ลูก, คัน after a number or in the noun-classifier-demonstrative frame (คนนี้... note คนนี้ segments คน|นี้) → NOUN.

### ที่ — the most overloaded word
- ADP "at/in" before a place: ที่|บ้าน
- SCONJ as relativizer/complementizer: คน|ที่|มา (the person who came), ขอโทษ|ที่|มา|สาย (sorry that I'm late)
- NOUN "place, plot of land": ที่|ของ|เขา
- Ordinal marker before a numeral: ที่|สอง (second) → ADJ-forming; tag ที่ as PART here.

### ให้
- Main verb "give": ให้|เงิน → VERB
- Benefactive "for/to" after a verb phrase: ทำ|ให้|แม่ → ADP
- Causative ("make/let someone do"): ทำให้ as segmented; the marker ให้ before a clause → SCONJ.
- Manner/result complement (verb ให้ adjective, "do it such that..."): พูด|ให้|ชัดเจน → SCONJ.
- Jussive at the start of a clause ("let/may...", including the exclamation ให้ตายสิ/เหอะ): → AUX.
These five cover every use — ให้ is never ADV or PART. When unsure between ADP and SCONJ, prefer ADP.

### Pronouns
ผม, ฉัน, ดิฉัน, คุณ, เธอ, เขา, เค้า, มัน, เรา, พวกเรา, พวกเขา, แก, กู, มึง → PRON. Kinship/occupation words used as address forms (แม่, พ่อ, พี่, ลุง, ป้า, หมอ) stay NOUN. Reciprocal กัน after a verb (รัก|กัน = love each other) → PRON.

### Linkers and adverbs
ก็ (then/also/so) → ADV. ยัง (still), เพิ่ง (just), ค่อย, มาก, จริงๆ → ADV. แต่, และ, หรือ, เพราะ, ถ้า → CCONJ/SCONJ as usual.

### Proper nouns
Names of people, places, organizations, and works — Thai or transliterated — are PROPN: เอ็ดเวิร์ด, กรุงเทพ, เชียงใหม่, เน็ตฟลิกซ์. Fictional names from films count. A transliterated name is not a common NOUN just because it is unfamiliar — and it is definitely not a run of single-character fragments."#
        }
        Language::Japanese => {
            r#"

Japanese-specific rules — please follow these carefully. Japanese presents unique challenges because it has no spaces between words, uses three writing systems simultaneously, and has extensive agglutinative morphology.

## Tokenization: a token is a word, not a morpheme

Every token is shown to a learner on its own, with its own dictionary entry. So the guiding principle is:

**Keep a boundary only when both sides are words a learner could be shown independently. Merge whenever splitting would strand a piece that is not a word.**

A verb or adjective and the entire chain of bound auxiliaries hanging off it are ONE token, however many morphemes deep the chain runs. 食べました is a word the same way 食べた is; so is 食べさせられたくなかった. Splitting them strands pieces — 食べ, まし, だっ, られ — that are not words in any sense: nobody can be shown 食べ on its own and told to learn it. (Word-internal morphology is handled by a separate morpheme layer, not by tokenization.)

Sanity test for a proposed boundary: could は or も be inserted there in natural Japanese? 食べてはいる is fine, so 食べて|いる is a real word boundary. ✗食べはました is not Japanese, so 食べました has no internal boundary. The test is one-directional: successful insertion proves a boundary, but failed insertion does not force a merge. The copula family is the standing exception — ✗学生はです fails only because は slips inside the copula instead (学生ではある), yet 学生|です still splits, because です is a word on its own.

### One token — merge (a split would strand a non-word)

All of these are ONE token each:

- Plain, polite, and negative inflection: 食べる, 食べた, 食べます, 食べました, 食べません, 食べない, 食べなかった — one VERB token each, lemma 食べる. NEVER 食べ + ました, never 食べ + ない.
- The polite negative past is bound end to end: 食べませんでした is ONE token (VERB, lemma 食べる).
- Godan 音便 forms: 書いて, 泳いで, 読んだ, 待った, 行って — one VERB token each (lemma 書く, 泳ぐ, 読む, 待つ, 行く).
- て-forms are complete conjugated words: 食べて, 見て, して, 書いて — the て/で is the tail of the verb's own token, never a separate token after a verb stem.
- Desiderative, causative, passive, volitional, imperative chains: 食べたい, 食べたかった, 食べさせる, 食べられる, 壊された, 食べさせられたくなかった, 行こう, 食べよう, 食べなさい, 見せなさい — one VERB token each, lemma the dictionary form (食べる, 壊す, 行く, 見せる).
- Conditional and listing forms: 食べたら, 言ったら, 食べれば, 高ければ, 読んだり — one token.
- ながら attaches to a stem that is not a word: 食べながら is one VERB token (lemma 食べる).
- い-adjective inflection: 美しかった, 高くない, よかった — one ADJ token each (lemma 美しい, 高い, 良い — よい and いい are one word and share the lemma 良い).
- Stem-attaching likelihood そう: 降りそう (VERB, lemma 降る), 高そう (ADJ, lemma 高い) — 降り and 高 alone are not words. (Hearsay そう after a complete form is different — see below.)
- Contracted casual forms: 見てる (ている→てる), 食べちゃった (てしまう→ちゃう), やっとく (ておく→とく) — one VERB token each; no split point leaves two words. The lemma is the plain verb: 見る, 食べる, やる.
- Contracted ていく/てくる keep direction in the lemma: てった/てく forms reference 行く (going AWAY); てきた/てくる forms reference 来る (coming TOWARD).
- Honorific prefix お/ご/御 + its word: お誕生日, ご予定, お名前, お呼び — one token each; お and ご are not words on their own. Keep the analyzer's normalized lemma (御誕生日, 御予定).
- Number + counter: 一つ, 二本, 三人, 一週間, 何泊 — one NOUN token each (see the counter section below). Keep the analyzer's numeral lemma (1つ, 3人).

The lemma of a merged token is the dictionary form of its head (食べませんでした → 食べる, 書かなかった → 書く, 美しかった → 美しい), and the POS is the head's (VERB for verbs, ADJ for adjectives).

### Separate tokens — the boundary survives (both sides are words)

- て-form | auxiliary verb: 食べて|いる, 読んで|しまう, 待って|ください, 帰って|きた, やって|みた, 持って|いく, 食べて|ほしい, 書いて|ある. The piece after the て-form is a real word with its own dictionary entry. Tag it AUX and keep the analyzer's normalized lemma: 居る (for いる/いた/います), 仕舞う (しまう), 下さる (ください), 来る (きた/きて), 行く (いった/いって), 見る (みた/みる), おく, 上げる (あげる), 呉れる (くれる), 貰う (もらう), 欲しい (ほしい). Do NOT respell these into kana — the lemma is an identifier (see the lemma section), and rewriting 呉れる to くれる splits one word into two entries.
- Noun | copula: 学生|です, 猫|だ, 学生|でした, 学生|だった, 学生|なら. The negated copula splits one step further: 学生|じゃ|ない, 静か|で|ない, 学生|じゃ|なかった — じゃ/で is the copula (AUX, lemma だ) and the negation is a word of its own (ADJ, lemma 無い). Never emit じゃない as one token: its lemma would be だ and the negation would vanish from the lemma entirely.
- Adjective | politeness です: 美しい|です, 静か|です, 高かった|です — the adjective is already a complete word; です only adds politeness. Never merge です into an adjective.
- Complete predicate | でしょう/だろう: 行く|でしょう, 美しかった|だろう — でしょう/だろう is one AUX token, lemma だ.
- Hearsay そう and evidential らしい/みたい after a COMPLETE form: 降る|そう|だ, 降った|らしい, 本当|らしい — the predicate is already finished, so そう/らしい/みたい stand alone (AUX). Contrast the stem-attaching likelihood そう above: 降りそう merges because 降り is not a word.
- Na-adjective | な: 綿密|な, 変|な, 静か|な — な is a separate token (AUX); the stem is the word (lemma 綿密だ, 変だ, 静かだ).
- Noun | する verb: 勉強|した, 確認|して, 愛|する, 延期|された, 勉強|しました — the noun is a word (NOUN, lemma 勉強) and the する-form is ONE word (VERB, lemma する). Never split した into し+た, and never merge the noun into the verb. 勉強している → 勉強|して|いる (three tokens).
- Noun | plural suffix たち/達: 子供|たち, 男|達, 私|たち — the suffix is its own token (NOUN, lemma 達, the shared identifier for both spellings). But the ら plurals are single words: 彼ら, 僕ら, これら, それら, 奴ら are ONE token each (PRON/NOUN, lemma as the analyzer normalized it, e.g. 彼等), never 彼|ら. And 友達 is one word (it is not a plural).
- Proper name | title suffix: 田中|さん, 田中|様, 山田|くん, 花子|ちゃん — titles split off names (title token: NOUN; keep the analyzer's lemma). But a title fused into an ordinary noun makes a single word: 皆さん, お客さん, お客様, お母さん, お父さん, 神様, 王様, 赤ちゃん are ONE NOUN token each, never 皆|さん or 神|様.
- Nominalizer の: 食べる|の|が|好き; explanatory んです: 食べた|ん|です (ん is the contracted nominalizer, lemma の).
- Particles always split, and a complete predicate before a particle keeps its full form: 高く|は|ない (explicit contrast splits the negation — and this ない, standing alone, is the ADJ ない), 食べて|も, 行く|し.
- Idiomatic compounds split when their parts are words: 気|に|入った (NOUN + ADP + VERB, lemma 入る), 愛|する. Idiomatic meaning is reconstructed at a future vocabulary layer, not by tokenization.

### Particles

Particles (助詞) are always separate tokens:
- Case particles: が, を, に, へ, で, と, から, まで, より → ADP
- Topic/contrast: は, も → ADP
- Genitive: の (after noun, e.g., 猫の名前) → ADP
- Sentence-final: か, ね, よ, な, ぞ, わ, さ → PART
- Conjunctive after a complete form: が, けど, けれど, し, ので, のに → SCONJ. (ながら is NOT in this class for verbs — it attaches to a bare stem, so 食べながら is one token.)

の has two distinct functions:
- Genitive (after noun): ADP — 猫の名前 = cat's name
- Nominalizer (after verb/adj): SCONJ or PART — 食べるのが好き = I like eating, 鍵を捜すのを手伝って = help me look for the key
Do NOT tag nominalizer の as ADP. The test: if の follows a verb or adjective and turns the clause into a noun phrase, it's a nominalizer (SCONJ/PART). If it follows a noun showing possession/attribution, it's genitive (ADP).

Quotative って (casual と): ADP, lemma って — its own dictionary word; do not lemmatize it to と. (と itself keeps lemma と.)

### Indefinite compounds 何か/誰か/どこか/いつか

何か (something), 誰か (someone), どこか (somewhere), いつか (someday/sometime) are dictionary words — ONE token each, with the か inside: 何か|食べたい, 何か|する|？, 誰か|助けて. PRON for 何か/誰か/どこか, ADV for いつか; the lemma is the whole word (何か, 誰か…). Do not split 何|か just because bare 何 would also parse — the sentence's meaning decides, not the analyzer's mood.

The split survives only when か is the question particle of an embedded question — when the clause means "what/who it is", not "something/someone": 彼が誰|か|知らない (I don't know WHO he is), これが何|か|教えて (tell me WHAT this is). Test with the translation: "some-" → one token; an indirect question word → split.

Examples:
- "東京に行く" → "東京" (NOUN) + "に" (ADP) + "行く" (VERB)
- "猫が好きです" → "猫" (NOUN) + "が" (ADP) + "好き" (ADJ, lemma "好きだ") + "です" (AUX, lemma "だ")
- "食べるのが好き" → "食べる" (VERB) + "の" (SCONJ, nominalizer) + "が" (ADP) + "好き" (ADJ, lemma "好きだ")

## Lemmatization

Every verb and adjective lemma should be the dictionary form. For verbs, this is the る/う-ending form. For い-adjectives, this is the い-ending form. For な-adjectives, the lemma includes だ (e.g., 静かだ, not 静か). Please don't leave a conjugated form as the lemma.

Verbs (dictionary form ends in -う row kana):
- 食べました → one token, lemma "食べる"
- 飲んだ → lemma "飲む"
- 行って → lemma "行く"
- 書かない → lemma "書く"
- 勉強した → "勉強" (NOUN, lemma "勉強") + "した" (VERB, lemma "為る" — the identifier for する, see above)

い-adjectives (dictionary form ends in い):
- 高くない → one token, lemma "高い"
- よかった → lemma "良い" (よい and いい are the same word; 良い is the shared identifier)
- 大きな → lemma "大きい"

The adverb よく (well/often, ADV) → lemma "良く", the analyzer's normalized adverb entry — not 良い (that lemma belongs to the adjective) and not よく.

な-adjectives (lemma includes だ to parallel verb dictionary forms):
- 静かな → "静か" (ADJ, lemma "静かだ") + "な" (AUX)
- きれいだった → "きれい" (ADJ, lemma "奇麗だ" — the analyzer's normalized spelling + だ; do not respell it) + "だった" (AUX, lemma "だ")
- 便利な → lemma "便利だ"

Note: na-adjective words can be either ADJ or NOUN depending on context:
Many na-adjective words also function as nouns. The POS depends on the grammatical role:
- 人気がある (popularity EXISTS) → 人気 is NOUN (lemma "人気"), が marks it as subject of ある
- 人気な歌手 (popular singer) → 人気 is ADJ (lemma "人気だ"), modifying 歌手
- 人気だ (is popular) → 人気 is ADJ (lemma "人気だ"), predicate
- 危険に気づく (notice the DANGER) → 危険 is NOUN (lemma "危険"), に marks it
- 危険な場所 (dangerous place) → 危険 is ADJ (lemma "危険だ"), modifying 場所
- 必要がある (there is a NEED) → 必要 is NOUN (lemma "必要"), subject of ある
- 必要な書類 (necessary documents) → 必要 is ADJ (lemma "必要だ"), modifying 書類
The rule: if followed by が/を/の (case particles treating it as a noun), it's NOUN. If followed by な (modifying a noun) or だ/です/でした/だった (predicate), it's ADJ. If a word is tagged ADJ with a だ-ending lemma but appears in neither context, double-check whether it's actually a noun being used predicatively (e.g., '絶品' is a noun, not a na-adjective) — only tag as ADJ if you're confident the word genuinely belongs to the na-adjective class.

Words that look like na-adjectives but are always NOUN:
- 好み (preference/taste): NOUN, not ADJ. "好みの問題" = a matter of preference. Lemma "好み".
- みんな (everyone): PRON or NOUN, not ADV. "みんな来て" = everyone come.

### Lexicalized に-adverbs: 確かに, 本当に, 別に

These are single ADV tokens — never split the に off. 別に is ALWAYS ADV with lemma 別に, in both senses (別に払う "separately", 別に急いでない "(not) particularly") — never the noun 別 and never the na-adjective 別だ; those readings belong to the bare token 別. Bare 確か as a sentence adverb (確かこの店だ "if I remember right, ...") is ADV with lemma 確か — a different word from both the na-adjective 確かだ and the adverb 確かに.

A repeated stem never changes the analysis: in 確かこの店だが、確かにおいしい, or 本当に本当のことを言って, or 別の時間に来て、別に急いでない, analyze each occurrence on its own — the bare 確か/本当/別 nearby must not bleed into the analysis of 確かに/本当に/別に, nor the other way around.

Honorific/humble verb forms keep their own lemma (they are distinct dictionary entries):
- いらっしゃいます → lemma "いらっしゃる"
- おっしゃった → lemma "おっしゃる"
- 召し上がる → lemma "召し上がる"
- ございます → lemma "ござる" (though ござる is archaic in isolation, we use it as the lemma for paradigm consistency — ございます/ございました/ございません are all ござる forms, and this matches how every other verb in the pipeline uses the bare dictionary form)

### The lemma is an identifier, not a display form

Lemmas are never shown to a learner. They exist to group the inflections and spelling variants of one word onto a single entry, so that やっぱり, やはり and やっぱ all resolve together. Judge a lemma by whether it groups correctly, not by whether it reads like something a person would write.

The analyzer's normalized forms therefore look odd in a few places, and they are correct as they stand. Leave them:
- する → 為る — the kanji spelling, even though virtually nobody writes する in kanji in modern Japanese
- いる → 居る — same
- ASCII is upper-cased, so Tom → TOM

Do fix a lemma that is flatly wrong: the wrong word entirely, the wrong verb, or a conjugated form left sitting in the lemma field (食べました → 食べる, never 食べまし). That is a real error. Archaic kanji and capitalization are not — resist the urge to tidy them into する, いる and Tom, because doing so splits one entry into two.

## Part of Speech

### A merged predicate keeps its head's POS
食べました, 書かなかった, 行こう are VERB; 美しかった, 高くない are ADJ. Bound auxiliaries never surface as their own tokens — if you find yourself emitting an AUX with lemma ます, た, たい, ない, れる, られる, せる, させる, or う, it should have been merged into the predicate before it.

### だ/です copula: always AUX with lemma "だ"
です is the polite form of だ. All copula tokens (だ, です, でした, だった, でしょう, なら) have lemma "だ".

### ない: ADJ (lemma 無い) when it stands alone — as a predicate (時間がない), after explicit contrast (高く|は|ない), or negating the copula (学生|じゃ|ない, 静か|で|ない) — merged into the predicate when it is the negation suffix (食べない, 高くない)

### い-adjectives vs な-adjectives
Both are tagged ADJ. い-adjectives conjugate (高い→高くない, one token). な-adjectives don't conjugate — they use だ/です, which stay separate tokens. Na-adjective lemmas include だ.

### いる/ある
VERB for existence (猫がいる, 本がある). AUX after て-form (食べて|いる, 書いて|ある).

### らしい: a word of its own after nouns and complete predicates
本当|らしい, 男|らしい, 降った|らしい — split, tagged AUX (or ADJ), not dep:fixed.

### 必要: context-dependent
ADJ in 必要だ/必要な (na-adjective). NOUN in 必要がある (subject of ある).

### Number+counter compounds stay together
- "三人" → one token (NOUN, lemma "3人")
- "五冊" → one token (NOUN, lemma "5冊")
- "一つ", "一週間", "何泊" → one token each (NOUN, lemma "1つ", "1週間", "何泊")
- 一番 → one token, lemma "1番" (ADV "most" or NOUN "number one"); 一人 → lemma "1人"
Never split the numeral from its counter (no 一|つ), and keep the analyzer's arabic-numeral lemma — it groups 一つ and 1つ onto one entry. The arabic numeral stays in the lemma even when the word functions as an adverb (一番).

### Proper nouns never decompose
- "トム" → one token (PROPN)
- "田中さん" → "田中" (PROPN) + "さん" (NOUN)"#
        }

        Language::Hindi => {
            r#"

Hindi-specific rules — please follow these carefully:

## Lemmatization

Verb lemmas should be the infinitive form ending in -ना:
- खाता/खाती/खाते → lemma "खाना" (to eat)
- जाता/गया/जाएगा → lemma "जाना" (to go)
- करता/किया/करेगा → lemma "करना" (to do)
- बोलता/बोला/बोलेगा → lemma "बोलना" (to speak)
- है/हैं/हूँ/था/थी/थे → lemma "होना" (to be)

Noun lemmas should be the direct case singular form. Oblique and plural forms must be lemmatized back:
- लड़के (oblique) → lemma "लड़का"
- लड़कों (plural oblique) → lemma "लड़का"
- बच्चों → lemma "बच्चा"
- लड़कियों → lemma "लड़की"

### Pronoun and possessive lemmatization — be consistent
All pronoun forms (including oblique, possessive, and fused postposition forms) lemmatize to the **base nominative pronoun**:
- मुझे/मुझको/मुझसे/मेरा/मेरी/मेरे → lemma "मैं" (not "मेरा")
- तुझे/तुझको/तेरा/तेरी/तेरे → lemma "तू"
- तुम्हें/तुम्हारा/तुम्हारी/तुम्हारे → lemma "तुम" (not "तुम्हारा")
- आपको/आपसे/आपका/आपकी/आपके → lemma "आप" (not "आपका")
- उसे/उसको/उससे/उसका/उसकी/उसके/उसने → lemma "वह" (not "उसका")
- इसे/इसको/इससे/इसका/इसकी/इसके/इसने → lemma "यह"
- उन्हें/उनसे/उनका/उनकी/उनके/उन्होंने/वे → lemma "वह"
- इन्हें/इनसे/इनका/इनकी/इनके/इन्होंने/ये → lemma "यह"
- हमें/हमसे/हमारा/हमारी/हमारे/हमने → lemma "हम" (not "हमारा")

वे/ये are the plural/honorific forms of वह/यह. We collapse them to the singular base lemma (वह/यह) because the singular form is the dictionary headword. The singular/honorific distinction is carried by the surface form and morphological features, not the lemma.

Possessives (मेरा/मेरी/मेरे, तुम्हारा/तुम्हारी/तुम्हारे, उसका/उसकी/उसके, etc.) should always be tagged DET, regardless of syntactic position. A learner should see the same word form filed under the same category. Their lemma is the base pronoun.

Reflexive possessive अपना/अपने/अपनी/अपनों ("one's own") always has lemma "अपना". Do not lemmatize to "आप" — अपना is its own dictionary entry distinct from the pronoun आप.

### Fused pronoun+postposition tokens
मुझे, तुमसे, उससे, हमने, उन्होंने — these are kept as single tokens because they are genuinely fused forms (not simply concatenated like noun+postposition). The lemma should be the base pronoun. The postposition information is encoded in the morphological form. Learners must learn these as distinct forms of the pronoun.

Postposition lemmas should be themselves: में, पर, को, से, के, का, की, ने, तक

## Part of Speech

### होना (to be): two POS tags

**VERB** — होना asserts that something exists or comes into being. The key: होना introduces or asserts the subject's existence/possession, rather than describing a known subject.
- Existence: "यहाँ शांति है" → है is VERB — asserting peace exists
- Possession: "तुम्हारे पास कंबल हैं" → हैं is VERB — asserting blankets exist in your possession
- Inchoative: "प्यार हो गया" → हो is VERB, गया is AUX — something came into being
- N/ADJ+होना verbalizer: "सिट्टी पिट्टी गुम होना" → होना is VERB — गुम होना means "to become lost"
- "छाती चौड़ी होना" → होना is VERB — "chest becoming wide" (inchoative)
- "बाल बाँका न होना" → होना is VERB — "not a hair becoming crooked"

Note the contrast with करना: N/ADJ+करना (agentive, करना is AUX verbalizer) vs N/ADJ+होना (involuntary/inchoative, होना is VERB because it carries the becoming/existence meaning).

**AUX** — होना is grammatical glue. The subject is already established and होना just links it to a description, or stacks on another verb for tense.
- Copula: "टॉम अकेला है" → है is AUX — linking Tom (known) to अकेला
- Copula: "मैं ईरान में था" → था is AUX — linking मैं (known) to location
- Tense: "वह खा रहा है" → है is AUX — stacking on another verb
- Tense: "बिल्ली सोती है" → है is AUX — habitual tense marker

The test: is होना asserting that something exists or comes into being? → VERB. Is होना describing a known subject or marking tense? → AUX.

Copula examples (AUX):
- "वह शिक्षक है" → है is AUX (he [known] is a teacher)
- "टॉम अकेला है" → है is AUX
- "मैं ईरान में था" → था is AUX (I [known] was in Iran)
- "वह शर्मीली है" → है is AUX
- "यह आसान था" → था is AUX

Existential/inchoative examples (VERB):
- "यहाँ शांति है" → है is VERB (peace exists here)
- "तुम्हारे पास कंबल हैं" → हैं is VERB (blankets exist in your possession)
- "प्यार हो गया" → हो is VERB (love came into being)

Tense auxiliary examples (AUX):
- "बिल्ली सोती है" → सोती is VERB, है is AUX
- "वह खा रहा है" → खा is VERB, रहा is AUX, है is AUX
- "पंछी उड़ते हैं" → उड़ते is VERB, हैं is AUX

### रहा/रही/रहे: progressive aspect marker
In compound tenses (V + रहा + होना), रहा is the progressive aspect marker and should be AUX, not ADJ. Pipelines frequently tag it as ADJ because it agrees in gender/number. Examples:
- वह खा रहा है (he is eating): खा = VERB, रहा = AUX, है = AUX
- वह पढ़ रही थी (she was reading): पढ़ = VERB, रही = AUX, थी = AUX

### Compound verbs (light verbs)
Hindi extensively uses compound verbs where a main verb stem combines with a light verb. The light verb should be AUX when adding aspectual meaning:
- खा लिया (completive: ate up) — लिया is AUX
- चल दिया (sudden start) — दिया is AUX
- बैठ गया (unintentional/sudden: sat down) — गया is AUX
When used independently, these are VERB: लेना = to take, देना = to give, जाना = to go.

### Multiword verb lemmas
Each token gets its own single-word lemma. No multiword lemmas. The multiword meaning is reconstructed at a later vocabulary layer. (POS follows the conjunct-verb rule below: the light verb is AUX.)
- "दिखाई देता है" → "दिखाई" (NOUN, lemma "दिखाई") + "देता" (AUX, lemma "देना") + "है" (AUX, lemma "होना")
- "पसंद करता है" → "पसंद" (NOUN, lemma "पसंद") + "करता" (AUX, lemma "करना") + "है" (AUX, lemma "होना")
- "मदद करो" → "मदद" (NOUN, lemma "मदद") + "करो" (AUX, lemma "करना")

### Noun/adjective + करना: करना is AUX (light verb)
When करना follows a noun or participial adjective to form a compound verb, करना is AUX and the noun is the lexical head:
- "प्रशंसा करना" (to praise) → प्रशंसा is NOUN, करना is AUX
- "मदद करना" (to help) → मदद is NOUN, करना is AUX
- "प्रयत्न करना" (to attempt) → प्रयत्न is NOUN, करना is AUX
- "शिकार करना" (to hunt) → शिकार is NOUN, करना is AUX
- "व्यापार करना" (to trade) → व्यापार is NOUN, करना is AUX
- "प्रेक्षित करना" (to observe) → प्रेक्षित is ADJ, करना is AUX
- "प्यार करना" (to love) → प्यार is NOUN, करना is AUX
- "कोशिश करना" (to try) → कोशिश is NOUN, करना is AUX

Other verbs after nouns (not करना) carry real semantic content and stay VERB:
- "ख़याल रखना" (to take care) → रखना is VERB — it means "to keep/maintain"
- "ठोकर मारना" (to kick) → मारना is VERB — it means "to hit"
- "गाली देना" (to abuse) → देना is AUX — it's a light verb like करना
- "जैसी करनी वैसी भरनी" → करनी and भरनी are nominalized forms (NOUN), not auxiliaries

### Compound verb direction: main verb is VERB, directional/aspectual is AUX
In V + जाना/लेना/देना compounds, the first verb (carrying the core meaning) is VERB, and the second verb (adding aspectual/directional meaning) is AUX:
- "निकल जाना" → "निकल" (VERB, lemma "निकलना") + "जाना" (AUX, lemma "जाना")
- "ले जाना" → "ले" (VERB, lemma "लेना") + "जाना" (AUX) — लेना carries "take", जाना adds "away"
- "पा लेना" → "पा" (VERB, lemma "पाना") + "लेना" (AUX)
- "दम तोड़ देना" → "तोड़" (VERB, lemma "तोड़ना") + "देना" (AUX)

### Multiword proper nouns: single token
Place names and person names that span multiple words should be a single PROPN token:
- "मेक्सिको नगर" → one token: "मेक्सिको नगर" (PROPN, lemma "मेक्सिको नगर")
- "इचिरो तानाका" → one token: "इचिरो तानाका" (PROPN)

### PROPN vs PRON disambiguation
मेरी/मेरे followed by को/ने/से is almost certainly a proper noun (मैरी = Mary), not a possessive pronoun. Possessive pronouns don't take direct postpositions — you'd say मुझको/मुझे (oblique pronoun + postposition), not मेरी को. Similarly, हरी को is likely "to Hari" (PROPN), not "green to" (ADJ).

### Postpositions
Simple postpositions should always be ADP: में, पर, को, से, के, का, की, ने, तक, द्वारा.

Compound postpositions (के लिए, के साथ, के बारे में, etc.) should be consistently split into separate ADP tokens:
- "के लिए" → "के" (ADP) + "लिए" (ADP) — both parts are ADP
- "के साथ" → "के" (ADP) + "साथ" (ADP)
- "के बारे में" → "के" (ADP) + "बारे" (ADP) + "में" (ADP)
- "के बाद" → "के" (ADP) + "बाद" (ADP)
Use "के लिए" consistently (not "के लिये" — both spellings exist but pick one).

### Focus/emphasis particles
ही (only/emphasis), भी (also/even) should be PART — not ADV.
तो (then/emphasis) should be PART or CCONJ — not ADV.

### Negation
नहीं, न, and मत should consistently be ADV (not PART).

### चाहिए: lemma is "चाहिए", not "चाहना"
चाहिए means "is needed/should" while चाहना means "to want." A learner looking up चाहना will find "to want," not "to need." Most Hindi dictionaries list चाहिए as its own entry. Use lemma "चाहिए" for चाहिए, and lemma "चाहना" for चाहता/चाहती/चाहते (forms of "to want").

चाहिए's POS is decided by context, and the pipeline enforces this rule deterministically — please apply it exactly:
- After an infinitive (a verb form ending -ना/-नी/-ने, possibly with नहीं or a focus particle in between): deontic "should" → AUX. "तुम्हें जाना चाहिए" → जाना (VERB) + चाहिए (AUX). "ऐसा नहीं करना चाहिए" → करना (VERB) + चाहिए (AUX).
- Otherwise: main predicate of need, "X को Y चाहिए" → VERB. "मुझे चाय चाहिए" → चाहिए is VERB. "तुम्हें किसलिए चाहिए?" → VERB.

### ठीक and अच्छा: ADJ vs ADV vs INTJ
- Predicative or attributive → ADJ: "यह ठीक है", "मेरा अच्छा दोस्त". This includes the complement of लगना: "आप ठीक लग रहे हो", "मुझे अच्छा लगा" → ADJ, not ADV (enforced deterministically).
- Manner adverb modifying another verb → ADV: "वह टेनिस अच्छा खेलता है", "ठीक कहा".
- Standalone discourse formula → INTJ: "ठीक है।" as a bare reply ("Okay."), "अच्छा!" ("Oh, I see!"), "बहुत अच्छा!" ("Very good!"). If the clause has its own subject ("यह ठीक है"), it is ADJ, not INTJ.

### Common lemma errors to watch for
- लाइए → lemma "लाना" (to bring), not "लेना" (to take). लाइए is the honorific imperative of लाना.
- किसी/किसीने/किसीको → lemma "कोई" (oblique form → base indefinite pronoun)
- किस/किसे/किसने/किसको/किससे → lemma "कौन" (oblique of कौन "which/who", not क्या "what"). "किस वजह से" = "for which reason"
- जनता = NOUN "the public" (lemma जनता). जानता = VERB "knows" (lemma जानना). These are different words.
- जाने after ईश्वर/खुदा/भगवान/कौन = subjunctive of जानना (to know), lemma "जानना". "ईश्वर जाने" = "God knows", not "God goes." Also "कौन जाने" = "who knows."
- और = CCONJ (conjunction "and") or ADV (adverb "more"). It is never ADJ.
- कैसे as ADV ("how") → lemma "कैसे" (its own headword). कैसा/कैसी (ADJ "what kind") → lemma "कैसा".
- रहा/रही/रहे/रहीं are forms of रहना only — never lemma "करना" or "होना".
- Simple postpositions lemmatize to themselves: के → lemma "के" (never "का").

### वाला/वाली/वाले
Multifunctional — tag based on context:
- ADJ when forming adjectives: दूध वाला (the milk one)
- AUX/PART when marking near-future: जाने वाला है (is about to go)
- DET when specifying: वह वाला (that one)"#
        }
        _ => "",
    }
}

/// Use GPT to clean/correct an NLP analyzed sentence
pub async fn clean_sentence_with_llm(
    language: Language,
    sentence: &NlpAnalyzedSentence,
    suspicious_reasons: Vec<String>,
    chat_client: &ChatClient,
) -> anyhow::Result<Vec<SimplifiedTokenPrime>> {
    let suspicion_context = if !suspicious_reasons.is_empty() {
        let reason = suspicious_reasons.into_iter().enumerate().fold(
            String::new(),
            |mut acc, (idx, reason)| {
                use std::fmt::Write;
                if acc.is_empty() {
                    format!("{idx}. {reason}")
                } else {
                    write!(acc, "\n{idx}. {reason}").unwrap();
                    acc
                }
            },
        );
        format!(
            "\n\nPlease keep the following in mind: {reason}\nPlease review these points one by one and correct them (only if necessary). There may be additional issues that are not listed here."
        )
    } else {
        String::new()
    };

    let language_tips = language_specific_tips(language);

    let system_prompt = format!(
        r#"You are an expert in {language} NLP analysis. Your task is to review and potentially correct an automatically-generated NLP analysis of a {language} sentence.

The analysis consists of tokens, where each token has:

{{
    "1. text": string, // the word as it appears (including contractions, so "l'" should be "l", not "le", and "don't" should be "do" and "n't").
    "2. whitespace": string, // any whitespace after the word. if you need a non-breaking space (used in some languages), use "[nbspace]" in the whitespace field.
    "3. pos": string, // part of speech. (e.g., Noun, Verb, Aux, Adj, Adv, Det, Pron, Propn, etc.)
    "4. lemma": string, // the dictionary/base/standardized form of the word
}}

Common issues to avoid:
- Lemmas that are incorrect (e.g., pronouns with wrong base forms)
- Part of speech tags that don't match the word
- Capitalized words getting confused for proper nouns just because they are capitalized
- Capitalization issues in lemmas (lemmas should generally be lowercase, except when the case is meaningful as in proper nouns and German nouns)
- Lemmas that contain spaces (usually errors)
- Lemmas that do not convert the word to its dictionary form
- Lemmas that do not convert the word to its masculine singular form (if applicable)
- Contractions with themselves as lemmas (e.g., "l'" with lemma "l'" instead of "le")
- Unncessary combinations. e.g. "qu'est-ce" can be four tokens, "qu''/"que", "est"/"être", "-"/"-", and "ce"/"ce", and doesn't need to be combined into a single token. Similar for "c'est-ce" (should be "c''/"ce", "est"/"être", "-"/"-", and "ce"/"ce"), est-ce que (should be "est"/"être", "-"/"-", "ce"/"ce". "que"/"que"), etc.
- Unjoined multiword proper nouns (e.g. "Croissant Fertile" should be one token, "Croissant Fertile", not two tokens, "Croissant" and "Fertile")

The text of the word should always be the same as it appears in the sentence (including hyphens, apostrophes, etc.) The goal is that you can concatenate the tokens + whitespace in the order they appear in your output to get the original sentence.

Hyphenated words should usually be split into three separate tokens. For example, "can-do" should be split into "can", "-", "do". "toi-même" should be split into "toi", "-", "même".

Review the analysis carefully. If you find errors, correct them. If the analysis is already correct, return it unchanged. In either case, you will return all tokens in the sentence. You are the ultimate authority on the correct analysis of the sentence, and your response should stand alone.{language_tips}

Think through your analysis, and finally provide the corrected token list. Remember, the provided analysis likely has errors. If it was likely to be good, we would not need you!"#
    );

    // Convert DocTokens to SimplifiedTokens for the prompt
    let simplified_tokens: Vec<SimplifiedTokenPrime> = sentence
        .doc
        .iter()
        .map(|token| SimplifiedTokenPrime {
            text: token.text.clone(),
            whitespace: if token.whitespace.clone() == "\u{00A0}" {
                "[nbspace]".to_string()
            } else {
                token.whitespace.clone()
            },
            pos: token.pos,
            lemma: token.lemma.clone(),
        })
        .collect();

    // The per-sentence suspicion notes ride in the user message, not the system prompt.
    // The system prompt must stay byte-identical across sentences so provider-side
    // prompt caching can reuse it: gpt-5.6+ caches exact prefixes at the latest user
    // message and re-bills every unique prefix as a paid cache write, and even the
    // older longest-prefix models stop matching at the first divergent byte — volatile
    // content in the system prompt would un-cache the entire tips block after it.
    let user_prompt = format!(
        "Sentence: \"{}\"\n\nCurrent NLP analysis:\n{}{suspicion_context}",
        sentence.sentence,
        serde_json::to_string_pretty(&simplified_tokens)?
    );

    let response: NlpCorrectionResponse = chat_client
        .chat_with_system_prompt(system_prompt, user_prompt)
        .await?;

    let corrected_tokens: Vec<SimplifiedTokenPrime> = response
        .corrected_tokens
        .into_iter()
        .map(|token| SimplifiedTokenPrime {
            whitespace: if token.whitespace == "[nbspace]" {
                "\u{00A0}".to_string()
            } else {
                token.whitespace
            },
            pos: if token.text == "-" {
                PartOfSpeechTag::Punct
            } else {
                token.pos
            },
            text: token.text,
            lemma: token.lemma,
        })
        .collect();

    Ok(corrected_tokens)
}

/// Double-check LLM output by feeding it back with specific concerns.
pub async fn double_check_with_llm(
    language: Language,
    sentence: &str,
    tokens: &[SimplifiedTokenPrime],
    reasons: Vec<String>,
    chat_client: &ChatClient,
) -> anyhow::Result<Vec<SimplifiedTokenPrime>> {
    let reason_list = reasons
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{i}. {r}"))
        .collect::<Vec<_>>()
        .join("\n");

    let language_tips = language_specific_tips(language);

    let system_prompt = format!(
        r#"You are an expert in {language} NLP analysis doing a quality review. A previous model already analyzed this sentence, but we suspect some issues remain. Your job is to review the analysis and fix any problems.

The analysis consists of tokens, where each token has:

{{
    "1. text": string, // the word as it appears
    "2. whitespace": string, // whitespace after the word. use "[nbspace]" for non-breaking spaces.
    "3. pos": string, // part of speech
    "4. lemma": string, // dictionary/base form
}}

Specific concerns about the current analysis are listed alongside the sentence. Please review those concerns carefully and correct the analysis if needed. Return all tokens, not just the changed ones. The text of each token must remain exactly as it appears in the original sentence. The goal is that you can concatenate the tokens + whitespace in the order they appear in your output to get the original sentence.{language_tips}"#
    );

    let simplified_tokens: Vec<SimplifiedTokenPrime> = tokens
        .iter()
        .map(|token| SimplifiedTokenPrime {
            text: token.text.clone(),
            whitespace: if token.whitespace == "\u{00A0}" {
                "[nbspace]".to_string()
            } else {
                token.whitespace.clone()
            },
            pos: token.pos,
            lemma: token.lemma.clone(),
        })
        .collect();

    // Concerns are per-sentence, so they go in the user message — the system prompt
    // stays byte-identical across sentences for provider-side prompt caching (see the
    // matching note in clean_sentence_with_llm).
    let user_prompt = format!(
        "Sentence: \"{sentence}\"\n\nCurrent analysis:\n{}\n\nSpecific concerns:\n{reason_list}",
        serde_json::to_string_pretty(&simplified_tokens)?
    );

    let response: NlpCorrectionResponse = chat_client
        .chat_with_system_prompt(system_prompt, user_prompt)
        .await?;

    let corrected_tokens: Vec<SimplifiedTokenPrime> = response
        .corrected_tokens
        .into_iter()
        .map(|token| SimplifiedTokenPrime {
            whitespace: if token.whitespace == "[nbspace]" {
                "\u{00A0}".to_string()
            } else {
                token.whitespace
            },
            pos: if token.text == "-" {
                PartOfSpeechTag::Punct
            } else {
                token.pos
            },
            text: token.text,
            lemma: token.lemma,
        })
        .collect();

    Ok(corrected_tokens)
}

/// Use GPT to parse dependency relations for a sentence
///
/// TODO(clitic-labeling consistency): the gold parses this produces — and
/// therefore lexide, which is trained on them — label Romance reflexive/object
/// clitics inconsistently. Observed in the French corpus, the same pronominal
/// construction analyzed three ways:
///   "je saurai me passer de vous"       → me  lemma "me", dep obj
///   "Mon peuple a appris à s'en passer" → s'  lemma "se", dep iobj
///   "Je me suis occupé de ce problème"  → me  lemma "me", dep expl:pv
/// UD's convention for inherently pronominal verbs is `expl:pv`; we should pick
/// a policy (dep label, and whether the clitic lemmatizes to "se" across the
/// paradigm) and enforce it here with explicit rules + examples in the prompt,
/// the way the classify pass does for POS ambiguities, then regenerate/patch
/// gold and retrain lexide. Until then, downstream consumers that match on a
/// clitic's lemma or dep (e.g. multiword-term unification of "me passer de" /
/// "s'en passer") must accept lemma-sets and dep-sets, not exact values.
///
/// TODO(pos/dep self-consistency): a related, mechanically detectable error
/// class — tokens whose POS contradicts their dep label. In the French corpus
/// lexide emits ~8.9k tokens with dep=det but pos!=DET; the unambiguous
/// subclass is articles POS-tagged PRON (les/le/la/l' as PRON with a det
/// edge, ~2.7k tokens), concentrated in verbless exclamations ("Les nuls !",
/// "Les enculés !") where there is no verb to attach a clitic to. Sense
/// discovery surfaced this as a spurious "article défini" sense of les@PRON.
/// A dep/POS consistency check over the gold (det edge => DET head-side POS,
/// modulo UD predeterminer conventions — tout/beaucoup need checking against
/// the guidelines before being called errors) would catch these before they
/// train lexide.
pub async fn parse_dependencies_with_llm(
    language: Language,
    sentence: &str,
    tokens: &[SimplifiedTokenPrime],
    chat_client: &ChatClient,
) -> anyhow::Result<DependencyParseResponse> {
    let system_prompt = format!(
        r#"You are an expert in {language} syntax and dependency grammar (Universal Dependencies). Your task is to analyze the dependency structure of a {language} sentence.

For each token in the sentence, you need to identify:
1. Its dependency relation (e.g., nsubj, obj, det, etc.)
2. Its head (the index of the token it depends on, or 0 for the root)

Universal Dependencies relation types include:
acl, acl:relcl, advcl, advcl:relcl, advmod, advmod:emph, advmod:lmod, amod, appos, aux, aux:pass, case, cc, cc:preconj, ccomp, clf, compound, compound:lvc, compound:prt, compound:redup, compound:svc, conj, cop, csubj, csubj:outer, csubj:pass, dep, det, det:numgov, det:nummod, det:poss, discourse, dislocated, expl, expl:impers, expl:pass, expl:pv, fixed, flat, flat:foreign, flat:name, goeswith, iobj, list, mark, nmod, nmod:poss, nmod:tmod, nsubj, nsubj:outer, nsubj:pass, nummod, nummod:gov, obj, obl, obl:agent, obl:arg, obl:lmod, obl:tmod, orphan, parataxis, punct, reparandum, root, vocative, xcomp

Important rules:
- Exactly one token should have "root" as its dependency and 0 as its head
- All other tokens should have a head pointing to another token's index (1-based)
- The dependency structure should form a valid tree

Think through the sentence structure, then provide the dependency analysis for each token."#
    );

    // Build the indexed token list
    let mut indexed_tokens = String::new();
    for (i, token) in tokens.iter().enumerate() {
        indexed_tokens.push_str(&format!(
            "{}. {} ({}) ({})\n",
            i + 1,
            token.text,
            token.lemma,
            token.pos
        ));
    }

    let user_prompt = format!(
        "Sentence: \"{sentence}\"\n\nTokens:\n{indexed_tokens}\n\nProvide the dependency analysis for each token."
    );

    let response: DependencyParseResponse = chat_client
        .chat_with_system_prompt(system_prompt, user_prompt)
        .await?;

    Ok(response)
}
