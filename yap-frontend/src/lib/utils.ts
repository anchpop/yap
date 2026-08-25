import {
  get_audio,
  get_temp_audio,
  invalidate_audio_cache,
  type AudioRequest,
  type VoiceActorInfo,
} from "../../../yap-frontend-rs/pkg";

// Re-exported from the wasm bindings so callers import a single shared type
// (generated from the Rust `VoiceActorInfo`) rather than a hand-written copy.
export type { VoiceActorInfo };
export * from "./pure";

let currentAudio: HTMLAudioElement | null = null;
// Settles the in-flight play as an interruption, tearing the element down
// with its handlers already cleared. Tearing down via `src = ""` alone fires
// the element's error event, which the playback promise would treat as a bad
// clip — and evict a perfectly good entry from the OPFS audio cache.
let interruptCurrent: (() => void) | null = null;

function stopCurrentPlayback() {
  const interrupt = interruptCurrent;
  interruptCurrent = null;
  if (interrupt) {
    interrupt();
    return;
  }
  // No interrupt registered yet (playback not started): the element has no
  // handlers attached, so tearing it down directly can't fire anything.
  if (currentAudio) {
    try {
      currentAudio.pause();
      currentAudio.src = "";
    } catch {
      // ignore
    }
    currentAudio = null;
  }
}

function abortError(): DOMException {
  return new DOMException("Aborted", "AbortError");
}

export async function playAudio(
  audioRequest: AudioRequest,
  accessToken: string | undefined,
  needsAuth: () => void,
  onAudioElement?: (audio: HTMLAudioElement) => void | Promise<void>,
  signal?: AbortSignal,
  onVoiceActor?: (info: VoiceActorInfo) => void,
): Promise<void> {
  if (signal?.aborted) throw abortError();

  // If something else is already playing, stop it so the new request wins.
  stopCurrentPlayback();

  try {
    const result = await get_audio(audioRequest, accessToken);
    if (signal?.aborted) throw abortError();

    const audioData = result.bytes;
    const voiceActor = result.voice_actor;
    if (voiceActor) {
      onVoiceActor?.(voiceActor);
    }

    const audioBlob = new Blob([audioData], { type: "audio/mpeg" });
    const audioUrl = URL.createObjectURL(audioBlob);

    const audio = new Audio(audioUrl);
    currentAudio = audio;
    if (onAudioElement) {
      await onAudioElement(audio);
      // A play started during that await tears this element down via
      // stopCurrentPlayback (clearing currentAudio); continuing would call
      // play() on a dead element and misreport it as a playback failure.
      if (currentAudio !== audio) {
        URL.revokeObjectURL(audioUrl);
        throw abortError();
      }
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
        if (interruptCurrent === onAbort) interruptCurrent = null;
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
      // Being superseded by a new play tears down exactly like an abort —
      // crucially clearing onerror first, so the interruption isn't
      // misread as a defective clip and invalidated from the cache.
      interruptCurrent = onAbort;

      const handlePlaybackFailure = (error: unknown) => {
        if (settled) return;
        settled = true;
        if (interruptCurrent === onAbort) interruptCurrent = null;
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
        if (interruptCurrent === onAbort) interruptCurrent = null;
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
  onVoiceActor?: (info: VoiceActorInfo) => void,
): Promise<void> {
  if (signal?.aborted) throw abortError();

  stopCurrentPlayback();

  try {
    const result = await get_temp_audio(audioRequest, accessToken);
    if (signal?.aborted) throw abortError();

    const audioData = result.bytes;
    const voiceActor = result.voice_actor;
    if (voiceActor) {
      onVoiceActor?.(voiceActor);
    }

    const audioBlob = new Blob([audioData], { type: "audio/mpeg" });
    const audioUrl = URL.createObjectURL(audioBlob);

    const audio = new Audio(audioUrl);
    currentAudio = audio;

    return new Promise((resolve, reject) => {
      let settled = false;

      const onAbort = () => {
        if (settled) return;
        settled = true;
        if (interruptCurrent === onAbort) interruptCurrent = null;
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
      interruptCurrent = onAbort;

      audio.onended = () => {
        if (settled) return;
        settled = true;
        if (interruptCurrent === onAbort) interruptCurrent = null;
        signal?.removeEventListener("abort", onAbort);
        URL.revokeObjectURL(audioUrl);
        if (currentAudio === audio) currentAudio = null;
        resolve();
      };

      audio.onerror = () => {
        if (settled) return;
        settled = true;
        if (interruptCurrent === onAbort) interruptCurrent = null;
        signal?.removeEventListener("abort", onAbort);
        if (currentAudio === audio) currentAudio = null;
        reject(new Error("Audio playback failed"));
      };

      audio.play().catch((error) => {
        if (settled) return;
        settled = true;
        if (interruptCurrent === onAbort) interruptCurrent = null;
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
