use crate::deck_selection::DailyReviewTarget;
use language_utils::Language;

#[bridgerton::bridge(transparent)]
#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CourseMaturity {
    Stable,
    Beta,
    Alpha,
}

/// Shared language facts and copy for every client. CSS and browser APIs live in the web app.
#[bridgerton::bridge(transparent)]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageMetadata {
    pub iso_code: String,
    pub iso6391: String,
    pub status: CourseMaturity,
    /// Emoji flag.
    pub flag: String,
    /// The language's name in itself, e.g. Français.
    pub native_name: String,
    /// English name, disambiguated for script variants.
    pub english_name: String,
    /// English name as it reads in a sentence.
    pub common_name: String,
    /// The speakers, as an adjective.
    pub people: String,
    /// Short badge shown beside dictionary results.
    pub badge: String,
    /// ISO 15924 script qualifier where the base language code is ambiguous.
    pub script: Option<String>,
    /// Characters offered by an accent keyboard. Empty for languages needing a system keyboard.
    pub accented_characters: Vec<String>,
    /// Character description for keyboard guidance; None suppresses the tip.
    pub character_type: Option<String>,
    /// “I speak X”, written in X.
    pub i_speak: String,
    /// Localized Yap.Town name.
    pub yaptown_name: String,
    /// Localized “Let’s go!”.
    pub lets_go: String,
}

#[bridgerton::bridge]
pub fn get_language_metadata(language: Language) -> LanguageMetadata {
    use Language::*;
    match language {
        English => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Stable,
            flag: "🇬🇧".into(),
            native_name: "English".into(),
            english_name: "English".into(),
            common_name: "English".into(),
            people: "English".into(),
            badge: "EN".into(),
            script: None,
            accented_characters: vec![],
            character_type: None,
            i_speak: "I speak English".into(),
            yaptown_name: "Yap.Town".into(),
            lets_go: "Let's go!".into(),
        },
        French => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Stable,
            flag: "🇫🇷".into(),
            native_name: "Français".into(),
            english_name: "French".into(),
            common_name: "French".into(),
            people: "French".into(),
            badge: "FR".into(),
            script: None,
            accented_characters: vec![
                "à".into(),
                "â".into(),
                "é".into(),
                "è".into(),
                "ê".into(),
                "ë".into(),
                "î".into(),
                "ï".into(),
                "ô".into(),
                "ù".into(),
                "û".into(),
                "ü".into(),
                "ÿ".into(),
                "ç".into(),
                "œ".into(),
                "æ".into(),
            ],
            character_type: Some("accented".into()),
            i_speak: "Je parle français".into(),
            yaptown_name: "Yap.Ville".into(),
            lets_go: "Allons-y !".into(),
        },
        Spanish => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Stable,
            flag: "🇪🇸".into(),
            native_name: "Español".into(),
            english_name: "Spanish".into(),
            common_name: "Spanish".into(),
            people: "Spanish".into(),
            badge: "ES".into(),
            script: None,
            accented_characters: vec![
                "á".into(),
                "é".into(),
                "í".into(),
                "ó".into(),
                "ú".into(),
                "ü".into(),
                "ñ".into(),
                "¿".into(),
                "¡".into(),
            ],
            character_type: Some("accented".into()),
            i_speak: "Hablo español".into(),
            yaptown_name: "Yap.Ciudad".into(),
            lets_go: "¡Vamos!".into(),
        },
        German => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Stable,
            flag: "🇩🇪".into(),
            native_name: "Deutsch".into(),
            english_name: "German".into(),
            common_name: "German".into(),
            people: "German".into(),
            badge: "DE".into(),
            script: None,
            accented_characters: vec![
                "ä".into(),
                "ö".into(),
                "ü".into(),
                "ß".into(),
                "Ä".into(),
                "Ö".into(),
                "Ü".into(),
            ],
            character_type: Some("accented".into()),
            i_speak: "Ich spreche Deutsch".into(),
            yaptown_name: "Yap.Stadt".into(),
            lets_go: "Los geht's!".into(),
        },
        Italian => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Beta,
            flag: "🇮🇹".into(),
            native_name: "Italiano".into(),
            english_name: "Italian".into(),
            common_name: "Italian".into(),
            people: "Italian".into(),
            badge: "IT".into(),
            script: None,
            accented_characters: vec![
                "à".into(),
                "è".into(),
                "é".into(),
                "ì".into(),
                "ò".into(),
                "ù".into(),
            ],
            character_type: Some("accented".into()),
            i_speak: "Parlo italiano".into(),
            yaptown_name: "Yap.Città".into(),
            lets_go: "Andiamo!".into(),
        },
        Portuguese => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Beta,
            flag: "🇧🇷".into(),
            native_name: "Português".into(),
            english_name: "Portuguese".into(),
            common_name: "Portuguese".into(),
            people: "Portuguese".into(),
            badge: "PT".into(),
            script: None,
            accented_characters: vec![
                "á".into(),
                "é".into(),
                "í".into(),
                "ó".into(),
                "ú".into(),
                "â".into(),
                "ê".into(),
                "ô".into(),
                "ã".into(),
                "õ".into(),
                "ç".into(),
            ],
            character_type: Some("accented".into()),
            i_speak: "Eu falo português".into(),
            yaptown_name: "Yap.Cidade".into(),
            lets_go: "Vamos lá!".into(),
        },
        Russian => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇷🇺".into(),
            native_name: "Русский".into(),
            english_name: "Russian".into(),
            common_name: "Russian".into(),
            people: "Russian".into(),
            badge: "RU".into(),
            script: None,
            accented_characters: vec![],
            character_type: Some("Cyrillic".into()),
            i_speak: "Я говорю по-русски".into(),
            yaptown_name: "Yap.Город".into(),
            lets_go: "Пойдем!".into(),
        },
        Korean => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇰🇷".into(),
            native_name: "한국어".into(),
            english_name: "Korean".into(),
            common_name: "Korean".into(),
            people: "Korean".into(),
            badge: "KO".into(),
            script: None,
            accented_characters: vec![],
            character_type: Some("hangul".into()),
            i_speak: "한국어를 합니다".into(),
            yaptown_name: "얍.타운".into(),
            lets_go: "가자!".into(),
        },
        Japanese => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇯🇵".into(),
            native_name: "日本語".into(),
            english_name: "Japanese".into(),
            common_name: "Japanese".into(),
            people: "Japanese".into(),
            badge: "JA".into(),
            script: None,
            accented_characters: vec![],
            character_type: Some("Japanese".into()),
            i_speak: "日本語を話します".into(),
            yaptown_name: "Yap.町".into(),
            lets_go: "行こう！".into(),
        },
        ChineseSimplified => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇨🇳".into(),
            native_name: "简体中文".into(),
            english_name: "Chinese (Simplified)".into(),
            common_name: "Chinese".into(),
            people: "Chinese".into(),
            badge: "ZH".into(),
            script: Some("Hans".into()),
            accented_characters: vec![],
            character_type: Some("Chinese".into()),
            i_speak: "我说中文".into(),
            yaptown_name: "Yap.城".into(),
            lets_go: "走吧！".into(),
        },
        ChineseTraditional => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇹🇼".into(),
            native_name: "繁體中文".into(),
            english_name: "Chinese (Traditional)".into(),
            common_name: "Chinese".into(),
            people: "Chinese".into(),
            badge: "ZHT".into(),
            script: Some("Hant".into()),
            accented_characters: vec![],
            character_type: Some("Chinese".into()),
            i_speak: "我說中文".into(),
            yaptown_name: "Yap.城".into(),
            lets_go: "走吧！".into(),
        },
        Hindi => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇮🇳".into(),
            native_name: "हिन्दी".into(),
            english_name: "Hindi".into(),
            common_name: "Hindi".into(),
            people: "Hindi-speaking".into(),
            badge: "HI".into(),
            script: None,
            accented_characters: vec![],
            character_type: Some("Devanagari".into()),
            i_speak: "मैं हिन्दी बोलता हूँ".into(),
            yaptown_name: "यैप.टाउन".into(),
            lets_go: "चलो!".into(),
        },
        Thai => LanguageMetadata {
            iso_code: language.code().into(),
            iso6391: language.iso_639_1().into(),
            status: CourseMaturity::Alpha,
            flag: "🇹🇭".into(),
            native_name: "ไทย".into(),
            english_name: "Thai".into(),
            common_name: "Thai".into(),
            people: "Thai".into(),
            badge: "TH".into(),
            script: None,
            accented_characters: vec![],
            character_type: Some("Thai".into()),
            i_speak: "ฉันพูดภาษาไทย".into(),
            yaptown_name: "Yap.เมือง".into(),
            lets_go: "ไปกันเลย!".into(),
        },
    }
}

#[bridgerton::bridge(transparent)]
#[derive(serde::Serialize)]
pub struct DailyGoalOption {
    pub value: DailyReviewTarget,
    pub minutes: u32,
    /// Existing onboarding estimate, not a prediction from the scheduler.
    pub estimated_first_week_words: u32,
}

#[bridgerton::bridge]
pub fn get_daily_goal_options() -> Vec<DailyGoalOption> {
    use DailyReviewTarget::*;
    [Casual, Regular, Serious, Intense]
        .into_iter()
        .map(|value| {
            let minutes = value.target_seconds() / 60;
            DailyGoalOption {
                value,
                minutes,
                estimated_first_week_words: minutes * 5,
            }
        })
        .collect()
}
