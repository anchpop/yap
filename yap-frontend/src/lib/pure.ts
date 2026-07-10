// Helpers with no WASM/runtime dependencies, split out of utils.ts so the
// MCP widget (which must not bundle the WASM module) can share them.
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { Language } from "../../../yap-frontend-rs/pkg";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// Language utility functions
export const languageFlags: Record<Language, string> = {
  French: "🇫🇷",
  Spanish: "🇪🇸",
  Korean: "🇰🇷",
  English: "🇬🇧",
  German: "🇩🇪",
  Chinese: "🇨🇳",
  Japanese: "🇯🇵",
  Russian: "🇷🇺",
  Portuguese: "🇧🇷",
  Italian: "🇮🇹",
  Hindi: "🇮🇳",
};

export const nativeLanguageNames: Record<Language, string> = {
  English: "English",
  French: "Français",
  Spanish: "Español",
  Korean: "한국어",
  German: "Deutsch",
  Chinese: "中文",
  Japanese: "日本語",
  Russian: "Русский",
  Portuguese: "Português",
  Italian: "Italiano",
  Hindi: "हिन्दी",
};

export function isoCodeToLanguage(isoCode: string): Language | null {
  const isoToLanguage: Record<string, Language> = {
    fra: "French",
    eng: "English",
    spa: "Spanish",
    kor: "Korean",
    deu: "German",
    ita: "Italian",
    por: "Portuguese",
    rus: "Russian",
    hin: "Hindi",
  };
  return isoToLanguage[isoCode] || null;
}

export function getLanguageFlag(isoCodeOrLanguage: string): string {
  // Check if it's already a Language type
  if (isoCodeOrLanguage in languageFlags) {
    return languageFlags[isoCodeOrLanguage as Language];
  }
  // Otherwise convert from ISO code
  const language = isoCodeToLanguage(isoCodeOrLanguage);
  return language ? languageFlags[language] : "🌐";
}

export function getLanguageName(isoCodeOrLanguage: string): string {
  // Check if it's already a Language type
  if (isoCodeOrLanguage in nativeLanguageNames) {
    return nativeLanguageNames[isoCodeOrLanguage as Language];
  }
  // Otherwise convert from ISO code
  const language = isoCodeToLanguage(isoCodeOrLanguage);
  return language ? nativeLanguageNames[language] : isoCodeOrLanguage;
}

// Convert Language to ISO 639-1 2-letter language code for HTML lang attribute
export function languageToIso6391(language: Language): string {
  const languageToIso: Record<Language, string> = {
    French: "fr",
    English: "en",
    Spanish: "es",
    Korean: "ko",
    German: "de",
    Chinese: "zh",
    Japanese: "ja",
    Russian: "ru",
    Portuguese: "pt",
    Italian: "it",
    Hindi: "hi",
  };
  return languageToIso[language];
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
