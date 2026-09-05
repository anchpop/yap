// Presentation data, joined with canonical codes and course maturity generated
// from Rust. After changing learning_metadata.rs, rebuild WASM and run
// `pnpm generate:metadata` in yap-frontend; both builds check for stale metadata.
// This module has no WASM runtime dependency, so the MCP widget can share it.
import { LANGUAGE_LEARNING_METADATA } from "./learning-metadata.generated";
import type { Language, LanguageLearningMetadata } from "../../../yap-frontend-rs/pkg";

export interface LanguageColors {
  primary: string;
  secondary: string;
  accent: string;
  gradient: string;
}

export interface LanguageMeta extends LanguageLearningMetadata {
  /** Emoji flag. */
  flag: string;
  /** The language's name in itself, e.g. "Français". */
  nativeName: string;
  /** English name, disambiguated where a script split needs it. */
  englishName: string;
  /**
   * English name as it reads in a sentence ("your Chinese learning"), where
   * the script disambiguation `englishName` carries would be noise.
   */
  commonName: string;
  /** The speakers, as an adjective: "how X people talk". */
  people: string;
  /** Short badge shown next to dictionary results. */
  badge: string;
  /**
   * ISO 15924 script, set only where a bare `iso6391` would leave Han glyph
   * selection ambiguous. Presentation only: this belongs in the HTML `lang`
   * attribute (see `languageToLangAttr`) and nowhere else. Putting it into a
   * comparison is what silently broke movie filtering for Chinese.
   */
  script?: string;
  /**
   * `navigator.language` values that mean this language, lowercased. The bare
   * base subtag ("zh") belongs to whichever variant is the sane default.
   */
  browserCodes: string[];
  /**
   * Characters offered by the on-screen accent keyboard. Empty when the
   * language needs a system keyboard rather than a handful of extra keys.
   */
  accentedCharacters: string[];
  /**
   * How the mobile keyboard tip describes this language's characters
   * ("...to easily type Cyrillic characters"). Null suppresses the tip.
   * Prose for a human; unrelated to the ISO 15924 `script` above.
   */
  characterType: string | null;
  /** "I speak X", written in X. */
  iSpeak: string;
  /** "Yap.Town", localized. */
  yaptownName: string;
  /** "Let's go!", localized. */
  letsGo: string;
  /** Flag colors, used for card accents and hover washes. */
  colors: LanguageColors;
}

export const LANGUAGES: Record<Language, LanguageMeta> = {
  English: {
    ...LANGUAGE_LEARNING_METADATA.English,
    flag: "🇬🇧",
    nativeName: "English",
    englishName: "English",
    commonName: "English",
    people: "English",
    badge: "EN",
    browserCodes: ["en"],
    accentedCharacters: [],
    characterType: null,
    iSpeak: "I speak English",
    yaptownName: "Yap.Town",
    letsGo: "Let's go!",
    colors: {
      primary: "#012169",
      secondary: "#FFFFFF",
      accent: "#C8102E",
      gradient:
        "linear-gradient(90deg, #012169 33%, #FFFFFF 33% 66%, #C8102E 66%)",
    },
  },
  French: {
    ...LANGUAGE_LEARNING_METADATA.French,
    flag: "🇫🇷",
    nativeName: "Français",
    englishName: "French",
    commonName: "French",
    people: "French",
    badge: "FR",
    browserCodes: ["fr"],
    accentedCharacters: [
      "à",
      "â",
      "é",
      "è",
      "ê",
      "ë",
      "î",
      "ï",
      "ô",
      "ù",
      "û",
      "ü",
      "ÿ",
      "ç",
      "œ",
      "æ",
    ],
    characterType: "accented",
    iSpeak: "Je parle français",
    yaptownName: "Yap.Ville",
    letsGo: "Allons-y !",
    colors: {
      primary: "#002395",
      secondary: "#FFFFFF",
      accent: "#ED2939",
      gradient:
        "linear-gradient(90deg, #002395 33%, #FFFFFF 33% 66%, #ED2939 66%)",
    },
  },
  Spanish: {
    ...LANGUAGE_LEARNING_METADATA.Spanish,
    flag: "🇪🇸",
    nativeName: "Español",
    englishName: "Spanish",
    commonName: "Spanish",
    people: "Spanish",
    badge: "ES",
    browserCodes: ["es"],
    accentedCharacters: ["á", "é", "í", "ó", "ú", "ü", "ñ", "¿", "¡"],
    characterType: "accented",
    iSpeak: "Hablo español",
    yaptownName: "Yap.Ciudad",
    letsGo: "¡Vamos!",
    colors: {
      primary: "#C60B1E",
      secondary: "#FFC400",
      accent: "#C60B1E",
      gradient:
        "linear-gradient(180deg, #C60B1E 25%, #FFC400 25% 75%, #C60B1E 75%)",
    },
  },
  German: {
    ...LANGUAGE_LEARNING_METADATA.German,
    flag: "🇩🇪",
    nativeName: "Deutsch",
    englishName: "German",
    commonName: "German",
    people: "German",
    badge: "DE",
    browserCodes: ["de"],
    accentedCharacters: ["ä", "ö", "ü", "ß", "Ä", "Ö", "Ü"],
    characterType: "accented",
    iSpeak: "Ich spreche Deutsch",
    yaptownName: "Yap.Stadt",
    letsGo: "Los geht's!",
    colors: {
      primary: "#000000",
      secondary: "#DD0000",
      accent: "#FFCE00",
      gradient:
        "linear-gradient(180deg, #000000 33%, #DD0000 33% 66%, #FFCE00 66%)",
    },
  },
  Italian: {
    ...LANGUAGE_LEARNING_METADATA.Italian,
    flag: "🇮🇹",
    nativeName: "Italiano",
    englishName: "Italian",
    commonName: "Italian",
    people: "Italian",
    badge: "IT",
    browserCodes: ["it"],
    accentedCharacters: ["à", "è", "é", "ì", "ò", "ù"],
    characterType: "accented",
    iSpeak: "Parlo italiano",
    yaptownName: "Yap.Città",
    letsGo: "Andiamo!",
    colors: {
      primary: "#009246",
      secondary: "#FFFFFF",
      accent: "#CE2B37",
      gradient:
        "linear-gradient(90deg, #009246 33%, #FFFFFF 33% 66%, #CE2B37 66%)",
    },
  },
  Portuguese: {
    ...LANGUAGE_LEARNING_METADATA.Portuguese,
    flag: "🇧🇷",
    nativeName: "Português",
    englishName: "Portuguese",
    commonName: "Portuguese",
    people: "Portuguese",
    badge: "PT",
    browserCodes: ["pt"],
    accentedCharacters: ["á", "é", "í", "ó", "ú", "â", "ê", "ô", "ã", "õ", "ç"],
    characterType: "accented",
    iSpeak: "Eu falo português",
    yaptownName: "Yap.Cidade",
    letsGo: "Vamos lá!",
    colors: {
      primary: "#009B3A",
      secondary: "#FFDF00",
      accent: "#002776",
      gradient:
        "linear-gradient(135deg, #009B3A 40%, #FFDF00 40% 60%, #002776 60%)",
    },
  },
  Russian: {
    ...LANGUAGE_LEARNING_METADATA.Russian,
    flag: "🇷🇺",
    nativeName: "Русский",
    englishName: "Russian",
    commonName: "Russian",
    people: "Russian",
    badge: "RU",
    browserCodes: ["ru"],
    accentedCharacters: [],
    characterType: "Cyrillic",
    iSpeak: "Я говорю по-русски",
    yaptownName: "Yap.Город",
    letsGo: "Пойдем!",
    colors: {
      primary: "#FFFFFF",
      secondary: "#0039A6",
      accent: "#D52B1E",
      gradient:
        "linear-gradient(180deg, #FFFFFF 33%, #0039A6 33% 66%, #D52B1E 66%)",
    },
  },
  Korean: {
    ...LANGUAGE_LEARNING_METADATA.Korean,
    flag: "🇰🇷",
    nativeName: "한국어",
    englishName: "Korean",
    commonName: "Korean",
    people: "Korean",
    badge: "KO",
    browserCodes: ["ko"],
    accentedCharacters: [],
    characterType: "hangul",
    iSpeak: "한국어를 합니다",
    yaptownName: "얍.타운",
    letsGo: "가자!",
    colors: {
      primary: "#003478",
      secondary: "#FFFFFF",
      accent: "#C60B1E",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #C60B1E 50%)",
    },
  },
  Japanese: {
    ...LANGUAGE_LEARNING_METADATA.Japanese,
    flag: "🇯🇵",
    nativeName: "日本語",
    englishName: "Japanese",
    commonName: "Japanese",
    people: "Japanese",
    badge: "JA",
    browserCodes: ["ja"],
    accentedCharacters: [],
    characterType: "Japanese",
    iSpeak: "日本語を話します",
    yaptownName: "Yap.町",
    letsGo: "行こう！",
    colors: {
      primary: "#FFFFFF",
      secondary: "#BC002D",
      accent: "#BC002D",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #BC002D 50%)",
    },
  },
  ChineseSimplified: {
    ...LANGUAGE_LEARNING_METADATA.ChineseSimplified,
    flag: "🇨🇳",
    nativeName: "简体中文",
    englishName: "Chinese (Simplified)",
    commonName: "Chinese",
    people: "Chinese",
    badge: "ZH",
    script: "Hans",
    // Bare "zh" lands here: simplified is the majority default.
    browserCodes: ["zh", "zh-hans", "zh-cn", "zh-sg"],
    accentedCharacters: [],
    characterType: "Chinese",
    iSpeak: "我说中文",
    yaptownName: "Yap.城",
    letsGo: "走吧！",
    colors: {
      primary: "#DE2910",
      secondary: "#FFDE00",
      accent: "#DE2910",
      gradient: "linear-gradient(135deg, #DE2910 50%, #FFDE00 50%)",
    },
  },
  ChineseTraditional: {
    ...LANGUAGE_LEARNING_METADATA.ChineseTraditional,
    flag: "🇹🇼",
    nativeName: "繁體中文",
    englishName: "Chinese (Traditional)",
    commonName: "Chinese",
    people: "Chinese",
    badge: "ZHT",
    script: "Hant",
    browserCodes: ["zh-hant", "zh-tw", "zh-hk", "zh-mo"],
    accentedCharacters: [],
    characterType: "Chinese",
    iSpeak: "我說中文",
    yaptownName: "Yap.城",
    letsGo: "走吧！",
    colors: {
      primary: "#000095",
      secondary: "#FFFFFF",
      accent: "#FE0000",
      gradient: "linear-gradient(135deg, #000095 50%, #FE0000 50%)",
    },
  },
  Hindi: {
    ...LANGUAGE_LEARNING_METADATA.Hindi,
    flag: "🇮🇳",
    nativeName: "हिन्दी",
    englishName: "Hindi",
    commonName: "Hindi",
    people: "Hindi-speaking",
    badge: "HI",
    browserCodes: ["hi"],
    accentedCharacters: [],
    characterType: "Devanagari",
    iSpeak: "मैं हिन्दी बोलता हूँ",
    yaptownName: "यैप.टाउन",
    letsGo: "चलो!",
    colors: {
      primary: "#FF9933",
      secondary: "#FFFFFF",
      accent: "#138808",
      gradient:
        "linear-gradient(180deg, #FF9933 33%, #FFFFFF 33% 66%, #138808 66%)",
    },
  },
  Thai: {
    ...LANGUAGE_LEARNING_METADATA.Thai,
    flag: "🇹🇭",
    nativeName: "ไทย",
    englishName: "Thai",
    commonName: "Thai",
    people: "Thai",
    badge: "TH",
    browserCodes: ["th"],
    accentedCharacters: [],
    characterType: "Thai",
    iSpeak: "ฉันพูดภาษาไทย",
    yaptownName: "Yap.เมือง",
    letsGo: "ไปกันเลย!",
    colors: {
      primary: "#A51931",
      secondary: "#F4F5F8",
      accent: "#2D2A4A",
      gradient:
        "linear-gradient(180deg, #A51931 17%, #F4F5F8 17% 33%, #2D2A4A 33% 67%, #F4F5F8 67% 83%, #A51931 83%)",
    },
  },
};

/** Every language, in the order the table declares them. */
export const ALL_LANGUAGES = Object.keys(LANGUAGES) as Language[];

/**
 * Project one field out of the table into its own lookup. The cast is safe —
 * and lives here alone — because the source keys are exactly `Language`.
 */
export function mapLanguages<T>(
  select: (meta: LanguageMeta) => T,
): Record<Language, T> {
  return Object.fromEntries(
    ALL_LANGUAGES.map((language) => [language, select(LANGUAGES[language])]),
  ) as Record<Language, T>;
}

const BY_ISO_CODE: Record<string, Language> = Object.fromEntries(
  ALL_LANGUAGES.map((language) => [LANGUAGES[language].isoCode, language]),
);

const BY_BROWSER_CODE: Record<string, Language> = Object.fromEntries(
  ALL_LANGUAGES.flatMap((language) =>
    LANGUAGES[language].browserCodes.map((code) => [code, language]),
  ),
);

/** Inverse of `LanguageMeta.isoCode`; null for a code we don't teach. */
export function isoCodeToLanguage(isoCode: string): Language | null {
  return BY_ISO_CODE[isoCode] ?? null;
}

/** True when the string is one of the `Language` enum's variant names. */
export function isLanguage(value: string): value is Language {
  return value in LANGUAGES;
}

/**
 * The `Language` implied by the browser's locale, or null if we don't teach
 * it. The full tag wins over the base subtag so zh-TW picks traditional
 * rather than falling through to simplified.
 */
export function detectBrowserLanguage(): Language | null {
  const browserLang = navigator.language || navigator.languages?.[0];
  if (!browserLang) return null;
  const tag = browserLang.toLowerCase();
  return BY_BROWSER_CODE[tag] ?? BY_BROWSER_CODE[tag.split("-")[0]] ?? null;
}
