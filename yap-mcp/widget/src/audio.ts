// Web Audio avoids the host iframe's media-src restrictions on blob URLs.
// There is no HTMLAudioElement, so the app's visualizer is unavailable here.
import type { AudioRequest, VoiceActorInfo } from "../../../yap-frontend-rs/pkg";
import { app, connectOnce, resultText } from "./bridge";
import type { PlaybackOptions } from "../../../yap-frontend/src/lib/pure";

interface CachedAudio {
  buffer: AudioBuffer;
  voiceActor?: VoiceActorInfo;
}

const cache = new Map<string, CachedAudio>();
const inflight = new Map<string, Promise<CachedAudio>>();

let sharedContext: AudioContext | null = null;

function audioContext(): AudioContext {
  sharedContext ??= new AudioContext();
  return sharedContext;
}

function decodeBase64(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

async function fetchAudio(request: AudioRequest): Promise<CachedAudio> {
  const key = JSON.stringify(request);
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
      voice_actor?: VoiceActorInfo;
    } | null;
    if (result.isError || !audio?.audio_base64) {
      throw new Error(resultText(result) || "audio unavailable");
    }
    // Decoding needs no user gesture even while the context is suspended,
    // so prefetch can warm the cache with ready-to-play buffers.
    const buffer = await audioContext().decodeAudioData(
      decodeBase64(audio.audio_base64),
    );
    const entry: CachedAudio = { buffer, voiceActor: audio.voice_actor };
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

let stopCurrent: (() => void) | null = null;

function abortError(): DOMException {
  return new DOMException("Aborted", "AbortError");
}

/// Autoplay policy for Web Audio: a context created before any user gesture
/// starts suspended, and resume() just stays pending until a gesture lands.
/// Mirror what HTMLAudioElement.play() would do — fail fast so the caller's
/// error path (the audio button's banner) shows instead of hanging a spinner.
async function ensureRunning(ctx: AudioContext): Promise<void> {
  if (ctx.state === "running") return;
  await Promise.race([
    ctx.resume(),
    new Promise((resolve) => setTimeout(resolve, 300)),
  ]);
  if ((ctx.state as string) !== "running") {
    throw new Error("Audio requires a user interaction first");
  }
}

export async function playAudio(
  audioRequest: AudioRequest,
  _accessToken: string | undefined,
  _needsAuth: () => void,
  { signal, onVoiceActor }: PlaybackOptions = {},
): Promise<void> {
  if (signal?.aborted) throw abortError();

  // If something else is already playing, stop it so the new request wins.
  stopCurrent?.();

  const { buffer, voiceActor } = await fetchAudio(audioRequest);
  if (signal?.aborted) throw abortError();
  if (voiceActor) {
    onVoiceActor?.(voiceActor);
  }

  const ctx = audioContext();
  await ensureRunning(ctx);
  if (signal?.aborted) throw abortError();

  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(ctx.destination);

  return new Promise((resolve, reject) => {
    let settled = false;
    const settle = (outcome: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", stop);
      if (stopCurrent === stop) stopCurrent = null;
      outcome();
    };

    const stop = () => {
      try {
        source.stop();
      } catch {
        // never started / already stopped
      }
      settle(() => reject(abortError()));
    };

    signal?.addEventListener("abort", stop, { once: true });
    stopCurrent = stop;

    // Fires on natural end and after stop(); settle() makes the first
    // outcome win, so a stopped source still rejects with AbortError.
    source.onended = () => settle(resolve);

    try {
      source.start();
    } catch (error) {
      settle(() =>
        reject(error instanceof Error ? error : new Error(String(error))),
      );
    }
  });
}
