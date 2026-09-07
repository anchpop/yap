// Web-specific styles and locale aliases, joined with shared language metadata
// from Rust. After changing learning_metadata.rs, rebuild WASM and run
// `pnpm generate:metadata` in yap-frontend; both builds check for stale metadata.
// This module has no WASM runtime dependency, so the MCP widget can share it.
import { LANGUAGE_METADATA } from "./learning-metadata.generated";
import type { Language, LanguageMetadata } from "../../../yap-frontend-rs/pkg";

export interface LanguageColors {
  primary: string;
  secondary: string;
  accent: string;
  gradient: string;
}

export interface LanguageMeta extends LanguageMetadata {
  /** Browser locale aliases, lowercased. Bare zh defaults to simplified Chinese. */
  browserCodes: string[];
  /** CSS flag colors for card accents and hover washes. */
  colors: LanguageColors;
}

export const LANGUAGES: Record<Language, LanguageMeta> = {
  English: {
    ...LANGUAGE_METADATA.English,
    browserCodes: ["en"],
    colors: {
      primary: "#012169",
      secondary: "#FFFFFF",
      accent: "#C8102E",
      gradient: "linear-gradient(90deg, #012169 33%, #FFFFFF 33% 66%, #C8102E 66%)",
    },
  },
  French: {
    ...LANGUAGE_METADATA.French,
    browserCodes: ["fr"],
    colors: {
      primary: "#002395",
      secondary: "#FFFFFF",
      accent: "#ED2939",
      gradient: "linear-gradient(90deg, #002395 33%, #FFFFFF 33% 66%, #ED2939 66%)",
    },
  },
  Spanish: {
    ...LANGUAGE_METADATA.Spanish,
    browserCodes: ["es"],
    colors: {
      primary: "#C60B1E",
      secondary: "#FFC400",
      accent: "#C60B1E",
      gradient: "linear-gradient(180deg, #C60B1E 25%, #FFC400 25% 75%, #C60B1E 75%)",
    },
  },
  German: {
    ...LANGUAGE_METADATA.German,
    browserCodes: ["de"],
    colors: {
      primary: "#000000",
      secondary: "#DD0000",
      accent: "#FFCE00",
      gradient: "linear-gradient(180deg, #000000 33%, #DD0000 33% 66%, #FFCE00 66%)",
    },
  },
  Italian: {
    ...LANGUAGE_METADATA.Italian,
    browserCodes: ["it"],
    colors: {
      primary: "#009246",
      secondary: "#FFFFFF",
      accent: "#CE2B37",
      gradient: "linear-gradient(90deg, #009246 33%, #FFFFFF 33% 66%, #CE2B37 66%)",
    },
  },
  Portuguese: {
    ...LANGUAGE_METADATA.Portuguese,
    browserCodes: ["pt"],
    colors: {
      primary: "#009B3A",
      secondary: "#FFDF00",
      accent: "#002776",
      gradient: "linear-gradient(135deg, #009B3A 40%, #FFDF00 40% 60%, #002776 60%)",
    },
  },
  Russian: {
    ...LANGUAGE_METADATA.Russian,
    browserCodes: ["ru"],
    colors: {
      primary: "#FFFFFF",
      secondary: "#0039A6",
      accent: "#D52B1E",
      gradient: "linear-gradient(180deg, #FFFFFF 33%, #0039A6 33% 66%, #D52B1E 66%)",
    },
  },
  Korean: {
    ...LANGUAGE_METADATA.Korean,
    browserCodes: ["ko"],
    colors: {
      primary: "#003478",
      secondary: "#FFFFFF",
      accent: "#C60B1E",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #C60B1E 50%)",
    },
  },
  Japanese: {
    ...LANGUAGE_METADATA.Japanese,
    browserCodes: ["ja"],
    colors: {
      primary: "#FFFFFF",
      secondary: "#BC002D",
      accent: "#BC002D",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #BC002D 50%)",
    },
  },
  ChineseSimplified: {
    ...LANGUAGE_METADATA.ChineseSimplified,
    browserCodes: ["zh", "zh-hans", "zh-cn", "zh-sg"],
    colors: {
      primary: "#DE2910",
      secondary: "#FFDE00",
      accent: "#DE2910",
      gradient: "linear-gradient(135deg, #DE2910 50%, #FFDE00 50%)",
    },
  },
  ChineseTraditional: {
    ...LANGUAGE_METADATA.ChineseTraditional,
    browserCodes: ["zh-hant", "zh-tw", "zh-hk", "zh-mo"],
    colors: {
      primary: "#000095",
      secondary: "#FFFFFF",
      accent: "#FE0000",
      gradient: "linear-gradient(135deg, #000095 50%, #FE0000 50%)",
    },
  },
  Hindi: {
    ...LANGUAGE_METADATA.Hindi,
    browserCodes: ["hi"],
    colors: {
      primary: "#FF9933",
      secondary: "#FFFFFF",
      accent: "#138808",
      gradient: "linear-gradient(180deg, #FF9933 33%, #FFFFFF 33% 66%, #138808 66%)",
    },
  },
  Thai: {
    ...LANGUAGE_METADATA.Thai,
    browserCodes: ["th"],
    colors: {
      primary: "#A51931",
      secondary: "#F4F5F8",
      accent: "#2D2A4A",
      gradient: "linear-gradient(180deg, #A51931 17%, #F4F5F8 17% 33%, #2D2A4A 33% 67%, #F4F5F8 67% 83%, #A51931 83%)",
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
