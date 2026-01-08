import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"
import { get_audio, invalidate_audio_cache, type AudioRequest, type Language } from '../../../yap-frontend-rs/pkg'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
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
}

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
}

export function isoCodeToLanguage(isoCode: string): Language | null {
  const isoToLanguage: Record<string, Language> = {
    'fra': 'French',
    'eng': 'English',
    'spa': 'Spanish',
    'kor': 'Korean',
    'deu': 'German',
  }
  return isoToLanguage[isoCode] || null
}

export function getLanguageFlag(isoCodeOrLanguage: string): string {
  // Check if it's already a Language type
  if (isoCodeOrLanguage in languageFlags) {
    return languageFlags[isoCodeOrLanguage as Language]
  }
  // Otherwise convert from ISO code
  const language = isoCodeToLanguage(isoCodeOrLanguage)
  return language ? languageFlags[language] : '🌐'
}

export function getLanguageName(isoCodeOrLanguage: string): string {
  // Check if it's already a Language type
  if (isoCodeOrLanguage in nativeLanguageNames) {
    return nativeLanguageNames[isoCodeOrLanguage as Language]
  }
  // Otherwise convert from ISO code
  const language = isoCodeToLanguage(isoCodeOrLanguage)
  return language ? nativeLanguageNames[language] : isoCodeOrLanguage
}

// Convert Language to ISO 639-1 2-letter language code for HTML lang attribute
export function languageToIso6391(language: Language): string {
  const languageToIso: Record<Language, string> = {
    French: 'fr',
    English: 'en',
    Spanish: 'es',
    Korean: 'ko',
    German: 'de',
    Chinese: 'zh',
    Japanese: 'ja',
    Russian: 'ru',
    Portuguese: 'pt',
    Italian: 'it',
  }
  return languageToIso[language]
}

export const profilerOnRender = (id: string, phase: string, actualDuration: number, baseDuration: number, startTime: number, commitTime: number) => {
  void id
  void phase
  void actualDuration
  void baseDuration
  void startTime
  void commitTime
  // console.log(`id:`, id, `, phase:`, phase, `, actualDuration:`, actualDuration, `, baseDuration:`, baseDuration, `, startTime:`, startTime, `, commitTime:`, commitTime);
}

let isPlayingAudio = false;

export async function playAudio(audioRequest: AudioRequest, accessToken: string | undefined, needsAuth: () => void): Promise<void> {
  if (isPlayingAudio) {
    console.log('Audio already playing, skipping...');
    return;
  }

  isPlayingAudio = true;
  try {
    const audioData = await get_audio(audioRequest, accessToken);
    
    const audioBlob = new Blob([audioData], { type: 'audio/mpeg' });
    const audioUrl = URL.createObjectURL(audioBlob);
    
    const audio = new Audio(audioUrl);
    
    return new Promise((resolve, reject) => {
      let errorHandled = false;

      const invalidateCache = () => {
        void (async () => {
          try {
            await invalidate_audio_cache(audioRequest);
          } catch (invalidateError) {
            console.error('Failed to invalidate audio cache:', invalidateError);
          }
        })();
      };

      const handlePlaybackFailure = (error: unknown) => {
        if (errorHandled) return;
        errorHandled = true;

        // Only invalidate cache for actual audio file errors, not autoplay restrictions
        const isNotAllowedError = error instanceof Error && error.name === 'NotAllowedError';
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
        URL.revokeObjectURL(audioUrl);
        resolve();
      };

      audio.onerror = () => {
        handlePlaybackFailure(new Error('Audio playback failed'));
      };

      audio
        .play()
        .catch((error) => {
          handlePlaybackFailure(error);
        });
    });
  } catch (error) {
    if (typeof error === 'string' && error.includes('400')) {
      needsAuth();
    }
    console.error('Failed to play audio:', error);
    throw error;
  } finally {
    isPlayingAudio = false;
  }
}
