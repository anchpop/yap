// The single source of truth for per-language presentation data.
//
// Everything the UI needs to know about a language lives in one row of the
// table below. Adding a language to the Rust `Language` enum then breaks
// exactly one place — this file — instead of a dozen scattered lookup tables,
// several of which used to be `Record<string, ...>` and so went stale in
// silence rather than failing the build.
//
// No WASM/runtime dependencies (type-only import), so the MCP widget can
// share it — see lib/pure.ts.
import type { Language } from "../../../yap-frontend-rs/pkg";

export interface LanguageColors {
  primary: string;
  secondary: string;
  accent: string;
  gradient: string;
}

export interface LanguageMeta {
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
  /** Matches `Language::code` in language-utils: the data pipeline's code. */
  isoCode: string;
  /**
   * ISO 639-1 two-letter code. The canonical code for all logic and every
   * comparison — it matches Rust's `Language::iso_639_1` exactly, including
   * both Chinese variants collapsing to "zh", so the two sides can't drift.
   * Movie and book metadata's `original_language` holds these bare codes.
   */
  iso6391: string;
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
  /** How finished this language's course is, surfaced in the picker. */
  status: "stable" | "beta" | "alpha";
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
    flag: "🇬🇧",
    nativeName: "English",
    englishName: "English",
    commonName: "English",
    people: "English",
    badge: "EN",
    isoCode: "eng",
    iso6391: "en",
    browserCodes: ["en"],
    accentedCharacters: [],
    characterType: null,
    status: "stable",
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
    flag: "🇫🇷",
    nativeName: "Français",
    englishName: "French",
    commonName: "French",
    people: "French",
    badge: "FR",
    isoCode: "fra",
    iso6391: "fr",
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
    status: "stable",
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
    flag: "🇪🇸",
    nativeName: "Español",
    englishName: "Spanish",
    commonName: "Spanish",
    people: "Spanish",
    badge: "ES",
    isoCode: "spa",
    iso6391: "es",
    browserCodes: ["es"],
    accentedCharacters: ["á", "é", "í", "ó", "ú", "ü", "ñ", "¿", "¡"],
    characterType: "accented",
    status: "stable",
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
    flag: "🇩🇪",
    nativeName: "Deutsch",
    englishName: "German",
    commonName: "German",
    people: "German",
    badge: "DE",
    isoCode: "deu",
    iso6391: "de",
    browserCodes: ["de"],
    accentedCharacters: ["ä", "ö", "ü", "ß", "Ä", "Ö", "Ü"],
    characterType: "accented",
    status: "stable",
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
    flag: "🇮🇹",
    nativeName: "Italiano",
    englishName: "Italian",
    commonName: "Italian",
    people: "Italian",
    badge: "IT",
    isoCode: "ita",
    iso6391: "it",
    browserCodes: ["it"],
    accentedCharacters: ["à", "è", "é", "ì", "ò", "ù"],
    characterType: "accented",
    status: "beta",
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
    flag: "🇧🇷",
    nativeName: "Português",
    englishName: "Portuguese",
    commonName: "Portuguese",
    people: "Portuguese",
    badge: "PT",
    isoCode: "por",
    iso6391: "pt",
    browserCodes: ["pt"],
    accentedCharacters: ["á", "é", "í", "ó", "ú", "â", "ê", "ô", "ã", "õ", "ç"],
    characterType: "accented",
    status: "beta",
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
    flag: "🇷🇺",
    nativeName: "Русский",
    englishName: "Russian",
    commonName: "Russian",
    people: "Russian",
    badge: "RU",
    isoCode: "rus",
    iso6391: "ru",
    browserCodes: ["ru"],
    accentedCharacters: [],
    characterType: "Cyrillic",
    status: "alpha",
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
    flag: "🇰🇷",
    nativeName: "한국어",
    englishName: "Korean",
    commonName: "Korean",
    people: "Korean",
    badge: "KO",
    isoCode: "kor",
    iso6391: "ko",
    browserCodes: ["ko"],
    accentedCharacters: [],
    characterType: "hangul",
    status: "alpha",
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
    flag: "🇯🇵",
    nativeName: "日本語",
    englishName: "Japanese",
    commonName: "Japanese",
    people: "Japanese",
    badge: "JA",
    isoCode: "jpn",
    iso6391: "ja",
    browserCodes: ["ja"],
    accentedCharacters: [],
    characterType: "Japanese",
    status: "alpha",
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
    flag: "🇨🇳",
    nativeName: "简体中文",
    englishName: "Chinese (Simplified)",
    commonName: "Chinese",
    people: "Chinese",
    badge: "ZH",
    isoCode: "zho-hans",
    iso6391: "zh",
    script: "Hans",
    // Bare "zh" lands here: simplified is the majority default.
    browserCodes: ["zh", "zh-hans", "zh-cn", "zh-sg"],
    accentedCharacters: [],
    characterType: "Chinese",
    status: "alpha",
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
    flag: "🇹🇼",
    nativeName: "繁體中文",
    englishName: "Chinese (Traditional)",
    commonName: "Chinese",
    people: "Chinese",
    badge: "ZHT",
    isoCode: "zho-hant",
    iso6391: "zh",
    script: "Hant",
    browserCodes: ["zh-hant", "zh-tw", "zh-hk", "zh-mo"],
    accentedCharacters: [],
    characterType: "Chinese",
    status: "alpha",
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
    flag: "🇮🇳",
    nativeName: "हिन्दी",
    englishName: "Hindi",
    commonName: "Hindi",
    people: "Hindi-speaking",
    badge: "HI",
    isoCode: "hin",
    iso6391: "hi",
    browserCodes: ["hi"],
    accentedCharacters: [],
    characterType: "Devanagari",
    status: "alpha",
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
    flag: "🇹🇭",
    nativeName: "ไทย",
    englishName: "Thai",
    commonName: "Thai",
    people: "Thai",
    badge: "TH",
    isoCode: "tha",
    iso6391: "th",
    browserCodes: ["th"],
    accentedCharacters: [],
    characterType: "Thai",
    status: "alpha",
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
