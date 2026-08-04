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

/** BCP 47 tag for the HTML `lang` attribute. */
export function languageToBcp47(language: Language): string {
  return LANGUAGES[language].bcp47;
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
