// Widget replacements for yap-frontend's playAudio/playTempAudio: identical
// signatures and playback semantics, but bytes come from the server's
// app-only `get_audio` tool over the bridge (which runs the app's own
// resolution: human recording first, TTS fallback) instead of the WASM/OPFS
// path. A memory cache stands in for OPFS, and audio is fed to the element
// as a data: URI — the MCP Apps CSP allows `media-src data:` but says
// nothing about blob:.
import type { AudioRequest, VoiceActorInfo } from "../../../yap-frontend-rs/pkg";
import { app, connectOnce, resultText } from "./bridge";

interface CachedAudio {
  dataUri: string;
  voiceActor?: VoiceActorInfo;
}

const cache = new Map<string, CachedAudio>();
const inflight = new Map<string, Promise<CachedAudio>>();

function cacheKey(request: AudioRequest): string {
  return JSON.stringify(request);
}

async function fetchAudio(request: AudioRequest): Promise<CachedAudio> {
  const key = cacheKey(request);
  const hit = cache.get(key);
  if (hit) return hit;
  const pending = inflight.get(key);
  if (pending) return pending;

  const task = (async () => {
    await connectOnce();
    const result = await app.callServerTool({
      name: "get_audio",
      arguments: { request: request.request, provider: request.provider },
    });
    const audio = result.structuredContent as {
      audio_base64?: string;
      mime_type?: string;
      voice_actor?: VoiceActorInfo;
    } | null;
    if (result.isError || !audio?.audio_base64) {
      throw new Error(resultText(result) || "audio unavailable");
    }
    const entry: CachedAudio = {
      dataUri: `data:${audio.mime_type ?? "audio/mpeg"};base64,${audio.audio_base64}`,
      voiceActor: audio.voice_actor,
    };
    cache.set(key, entry);
    return entry;
  })();
  inflight.set(key, task);
  try {
    return await task;
  } finally {
    inflight.delete(key);
  }
}

/** Warm the cache so playback is instant when the card is revealed/advanced. */
export function prefetchAudio(request: AudioRequest): void {
  void fetchAudio(request).catch(() => {});
}

let currentAudio: HTMLAudioElement | null = null;

function abortError(): DOMException {
  return new DOMException("Aborted", "AbortError");
}

async function playFetched(
  audioRequest: AudioRequest,
  signal: AbortSignal | undefined,
  onVoiceActor: ((info: VoiceActorInfo) => void) | undefined,
  onAudioElement?: (audio: HTMLAudioElement) => void | Promise<void>,
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

  const { dataUri, voiceActor } = await fetchAudio(audioRequest);
  if (signal?.aborted) throw abortError();
  if (voiceActor) {
    onVoiceActor?.(voiceActor);
  }

  const audio = new Audio(dataUri);
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
    if (currentAudio === audio) currentAudio = null;
    throw abortError();
  }

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
      if (currentAudio === audio) currentAudio = null;
      reject(abortError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });

    audio.onended = () => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", onAbort);
      if (currentAudio === audio) currentAudio = null;
      resolve();
    };

    audio.onerror = () => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", onAbort);
      // A bad clip shouldn't stay cached.
      cache.delete(cacheKey(audioRequest));
      if (currentAudio === audio) currentAudio = null;
      reject(new Error("Audio playback failed"));
    };

    audio.play().catch((error: unknown) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", onAbort);
      if (currentAudio === audio) currentAudio = null;
      reject(error instanceof Error ? error : new Error(String(error)));
    });
  });
}

export async function playAudio(
  audioRequest: AudioRequest,
  _accessToken: string | undefined,
  _needsAuth: () => void,
  onAudioElement?: (audio: HTMLAudioElement) => void | Promise<void>,
  signal?: AbortSignal,
  onVoiceActor?: (info: VoiceActorInfo) => void,
): Promise<void> {
  return playFetched(audioRequest, signal, onVoiceActor, onAudioElement);
}

export async function playTempAudio(
  audioRequest: AudioRequest,
  _accessToken: string | undefined,
  _needsAuth: () => void,
  signal?: AbortSignal,
  onVoiceActor?: (info: VoiceActorInfo) => void,
): Promise<void> {
  return playFetched(audioRequest, signal, onVoiceActor);
}
