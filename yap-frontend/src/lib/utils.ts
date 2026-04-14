import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import {
  get_audio,
  get_temp_audio,
  invalidate_audio_cache,
  type AudioRequest,
  type Language,
} from "../../../yap-frontend-rs/pkg";

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

let currentAudio: HTMLAudioElement | null = null;

function abortError(): DOMException {
  return new DOMException("Aborted", "AbortError");
}

export async function playAudio(
  audioRequest: AudioRequest,
  accessToken: string | undefined,
  needsAuth: () => void,
  onAudioElement?: (audio: HTMLAudioElement) => void | Promise<void>,
  signal?: AbortSignal,
): Promise<void> {
  if (signal?.aborted) throw abortError();

  // If something else is already playing, stop it so the new request wins.
  if (currentAudio) {
    try {
      currentAudio.pause();
      currentAudio.src = "";
    } catch {
      // ignore
    }
    currentAudio = null;
  }

  try {
    const audioData = await get_audio(audioRequest, accessToken);
    if (signal?.aborted) throw abortError();

    const audioBlob = new Blob([audioData], { type: "audio/mpeg" });
    const audioUrl = URL.createObjectURL(audioBlob);

    const audio = new Audio(audioUrl);
    currentAudio = audio;
    if (onAudioElement) {
      await onAudioElement(audio);
    }
    if (signal?.aborted) {
      try {
        audio.pause();
        audio.src = "";
      } catch {
        // ignore
      }
      URL.revokeObjectURL(audioUrl);
      if (currentAudio === audio) currentAudio = null;
      throw abortError();
    }

    return new Promise((resolve, reject) => {
      let settled = false;

      const invalidateCache = () => {
        void (async () => {
          try {
            await invalidate_audio_cache(audioRequest);
          } catch (invalidateError) {
            console.error("Failed to invalidate audio cache:", invalidateError);
          }
        })();
      };

      const onAbort = () => {
        if (settled) return;
        settled = true;
        audio.onended = null;
        audio.onerror = null;
        try {
          audio.pause();
          audio.src = "";
        } catch {
          // ignore
        }
        URL.revokeObjectURL(audioUrl);
        if (currentAudio === audio) currentAudio = null;
        reject(abortError());
      };
      signal?.addEventListener("abort", onAbort, { once: true });

      const handlePlaybackFailure = (error: unknown) => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", onAbort);

        // Only invalidate cache for actual audio file errors, not autoplay restrictions
        const isNotAllowedError =
          error instanceof Error && error.name === "NotAllowedError";
        if (!isNotAllowedError) {
          invalidateCache();
        }
        // Don't revoke URL on error - let it be garbage collected naturally
        // Revoking here can trigger audio.onerror cascade
        if (error instanceof Error) {
          reject(error);
        } else {
          reject(new Error(String(error)));
        }
      };

      audio.onended = () => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", onAbort);
        URL.revokeObjectURL(audioUrl);
        if (currentAudio === audio) currentAudio = null;
        resolve();
      };

      audio.onerror = () => {
        if (currentAudio === audio) currentAudio = null;
        handlePlaybackFailure(new Error("Audio playback failed"));
      };

      audio.play().catch((error) => {
        handlePlaybackFailure(error);
      });
    });
  } catch (error) {
    currentAudio = null;
    if (error instanceof DOMException && error.name === "AbortError") {
      throw error;
    }
    if (
      !signal?.aborted &&
      typeof error === "string" &&
      error.includes("400")
    ) {
      needsAuth();
    }
    console.error("Failed to play audio:", error);
    throw error;
  }
}

export async function playTempAudio(
  audioRequest: AudioRequest,
  accessToken: string | undefined,
  needsAuth: () => void,
  signal?: AbortSignal,
): Promise<void> {
  if (signal?.aborted) throw abortError();

  if (currentAudio) {
    try {
      currentAudio.pause();
      currentAudio.src = "";
    } catch {
      // ignore
    }
    currentAudio = null;
  }

  try {
    const audioData = await get_temp_audio(audioRequest, accessToken);
    if (signal?.aborted) throw abortError();

    const audioBlob = new Blob([audioData], { type: "audio/mpeg" });
    const audioUrl = URL.createObjectURL(audioBlob);

    const audio = new Audio(audioUrl);
    currentAudio = audio;

    return new Promise((resolve, reject) => {
      let settled = false;

      const onAbort = () => {
        if (settled) return;
        settled = true;
        audio.onended = null;
        audio.onerror = null;
        try {
          audio.pause();
          audio.src = "";
        } catch {
          // ignore
        }
        URL.revokeObjectURL(audioUrl);
        if (currentAudio === audio) currentAudio = null;
        reject(abortError());
      };
      signal?.addEventListener("abort", onAbort, { once: true });

      audio.onended = () => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", onAbort);
        URL.revokeObjectURL(audioUrl);
        if (currentAudio === audio) currentAudio = null;
        resolve();
      };

      audio.onerror = () => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", onAbort);
        if (currentAudio === audio) currentAudio = null;
        reject(new Error("Audio playback failed"));
      };

      audio.play().catch((error) => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", onAbort);
        reject(error);
      });
    });
  } catch (error) {
    currentAudio = null;
    if (error instanceof DOMException && error.name === "AbortError") {
      throw error;
    }
    if (
      !signal?.aborted &&
      typeof error === "string" &&
      error.includes("400")
    ) {
      needsAuth();
    }
    console.error("Failed to play temp audio:", error);
    throw error;
  }
}
