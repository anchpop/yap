// Helpers with no WASM/runtime dependencies, split out of utils.ts so the
// MCP widget (which must not bundle the WASM module) can share them.
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { Language } from "../../../yap-frontend-rs/pkg";
import {
  LANGUAGES,
  isLanguage,
  isoCodeToLanguage,
  mapLanguages,
} from "./languages";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// Language utility functions. The data itself lives in ./languages — these
// are the shorthands that callers reach for most often.
export const languageFlags = mapLanguages((meta) => meta.flag);

export const nativeLanguageNames = mapLanguages((meta) => meta.nativeName);

// Accepts either a `Language` variant name or a pipeline ISO code, since
// server-side stats hand back the latter.
export function getLanguageFlag(isoCodeOrLanguage: string): string {
  const language = isLanguage(isoCodeOrLanguage)
    ? isoCodeOrLanguage
    : isoCodeToLanguage(isoCodeOrLanguage);
  return language ? LANGUAGES[language].flag : "🌐";
}

export function getLanguageName(isoCodeOrLanguage: string): string {
  const language = isLanguage(isoCodeOrLanguage)
    ? isoCodeOrLanguage
    : isoCodeToLanguage(isoCodeOrLanguage);
  return language ? LANGUAGES[language].nativeName : isoCodeOrLanguage;
}

/**
 * The canonical ISO 639-1 code — use this for every comparison and lookup.
 * Identical to Rust's `Language::iso_639_1`, so both Chinese variants give
 * "zh", which is what movie and book metadata's `original_language` holds.
 */
export function languageToIso6391(language: Language): string {
  return LANGUAGES[language].iso6391;
}

/**
 * Value for an HTML `lang` attribute, and nothing else. Appends the script
 * subtag where one is needed, because `lang` drives Han glyph selection:
 * Traditional Chinese tagged as bare "zh" can render with mainland glyph
 * forms. Never compare against this — see `languageToIso6391`.
 */
export function languageToLangAttr(language: Language): string {
  const { iso6391, script } = LANGUAGES[language];
  return script ? `${iso6391}-${script}` : iso6391;
}

export const profilerOnRender = (
  id: string,
  phase: string,
  actualDuration: number,
  baseDuration: number,
  startTime: number,
  commitTime: number,
) => {
  void id;
  void phase;
  void actualDuration;
  void baseDuration;
  void startTime;
  void commitTime;
  // console.log(`id:`, id, `, phase:`, phase, `, actualDuration:`, actualDuration, `, baseDuration:`, baseDuration, `, startTime:`, startTime, `, commitTime:`, commitTime);
};
