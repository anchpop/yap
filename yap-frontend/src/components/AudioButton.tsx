import { useState, useEffect, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Volume2 } from "lucide-react";
import { playAudio, playTempAudio } from "@/lib/utils";
import { type AudioRequest } from "../../../yap-frontend-rs/pkg";
import { isSoundEffectPlaying } from "@/lib/sound-effects";
import { toast } from "sonner";

interface AudioButtonProps {
  audioRequest: AudioRequest;
  accessToken: string | undefined;
  autoPlay?: boolean;
  className?: string;
  size?: "default" | "sm" | "lg" | "icon";
  variant?:
    | "default"
    | "destructive"
    | "outline"
    | "secondary"
    | "ghost"
    | "link";
  autoplayed?: boolean;
  setAutoplayed?: () => void;
  playPreAudio?: boolean;
  onError?: () => void;
  onSuccess?: () => void;
  visualizer?: boolean | "radial";
  temp?: boolean;
}

const WAVE_BARS = 36;
const WAVE_MAX_HEIGHT = 40;
const BLOB_POINTS = 14;
const BLOB_BASE = 34;
const BLOB_AMPLITUDE = 1;

const BLOB_PRIMARY: BlobParams = {
  base: BLOB_BASE,
  amplitude: BLOB_AMPLITUDE,
  phase: 0,
  speed: 1,
};
const BLOB_SECONDARY: BlobParams = {
  base: BLOB_BASE + 3,
  amplitude: BLOB_AMPLITUDE * 0.85,
  phase: 1.2,
  speed: 1.35,
};

interface BlobParams {
  base: number;
  amplitude: number;
  phase: number;
  speed: number;
}

function buildBlobPath(
  t: number,
  intensity: number,
  params: BlobParams,
): string {
  const cx = 60;
  const cy = 60;
  const pts: [number, number][] = [];
  for (let i = 0; i < BLOB_POINTS; i++) {
    const angle = (i / BLOB_POINTS) * Math.PI * 2 + params.phase;
    const s = params.speed;
    const wobble =
      Math.sin(t * 0.006 * s + i * 1.7 + params.phase) * 0.55 +
      Math.sin(-t * 0.011 * s + i * 2.9 + params.phase) * 0.35 +
      Math.sin(t * 0.017 * s + i * 3.7 + params.phase) * 0.2 +
      Math.sin(-t * 0.028 * s + i * 5.3 + params.phase) * 0.1;
    const r = params.base + wobble * params.amplitude * intensity;
    pts.push([cx + Math.cos(angle) * r, cy + Math.sin(angle) * r]);
  }
  // Closed Catmull-Rom-ish smoothing via quadratic midpoints
  let d = "";
  for (let i = 0; i < pts.length; i++) {
    const [x1, y1] = pts[i];
    const [x2, y2] = pts[(i + 1) % pts.length];
    const mx = (x1 + x2) / 2;
    const my = (y1 + y2) / 2;
    if (i === 0) d += `M ${mx} ${my} `;
    else d += `Q ${x1} ${y1} ${mx} ${my} `;
  }
  const [x1, y1] = pts[0];
  const [x2, y2] = pts[1];
  d += `Q ${x1} ${y1} ${(x1 + x2) / 2} ${(y1 + y2) / 2} Z`;
  return d;
}

export function AudioButton({
  audioRequest,
  accessToken,
  autoPlay = false,
  className = "h-10 w-10 shrink-0",
  size = "icon",
  variant = "ghost",
  autoplayed,
  setAutoplayed,
  playPreAudio = false,
  onError,
  onSuccess,
  visualizer = false,
  temp = false,
}: AudioButtonProps) {
  "use memo";
  const [isPlaying, setIsPlaying] = useState(false);
  const [needsAuth, setNeedsAuth] = useState(false);
  const isPlayingRef = useRef(isPlaying);
  const clickedRef = useRef(false);
  const blobPathRef = useRef<SVGPathElement | null>(null);
  const blobPathRef2 = useRef<SVGPathElement | null>(null);
  const waveContainerRef = useRef<HTMLDivElement | null>(null);
  const waveHistoryRef = useRef<number[]>(new Array(WAVE_BARS).fill(0));
  const analyserRef = useRef<AnalyserNode | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const audioElRef = useRef<HTMLAudioElement | null>(null);
  const hoverRef = useRef(false);
  const rafRunningRef = useRef(false);
  const kickRafRef = useRef<() => void>(() => {});
  const abortControllersRef = useRef<Set<AbortController>>(new Set());

  // Abort any in-flight audio fetch/playback when the component unmounts.
  useEffect(() => {
    const controllers = abortControllersRef.current;
    return () => {
      for (const c of controllers) c.abort();
      controllers.clear();
    };
  }, []);

  const attachAnalyser = useCallback(async (audio: HTMLAudioElement) => {
    try {
      if (!audioCtxRef.current) {
        const Ctx =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext })
            .webkitAudioContext;
        audioCtxRef.current = new Ctx();
      }
      const ctx = audioCtxRef.current;
      if (ctx.state === "suspended") {
        try {
          await ctx.resume();
        } catch (err) {
          console.warn("AudioContext resume failed:", err);
        }
      }
      const source = ctx.createMediaElementSource(audio);
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      analyser.smoothingTimeConstant = 0.1;
      source.connect(analyser);
      analyser.connect(ctx.destination);
      analyserRef.current = analyser;
      audioElRef.current = audio;
      audio.addEventListener("ended", () => {
        analyserRef.current = null;
        audioElRef.current = null;
      });
    } catch (err) {
      console.warn("Failed to attach analyser:", err);
    }
  }, []);

  // Animate blob via rAF using real audio amplitude
  useEffect(() => {
    if (!visualizer) return;
    let raf = 0;
    let intensity = 0;
    let lastPlayheadIdx = -1;
    let idleFramesAfterStop = 0;
    const buf = new Uint8Array(128);
    const tick = (t: number) => {
      let target = hoverRef.current ? 0.35 : 0;
      const analyser = analyserRef.current;
      if (isPlayingRef.current && analyser) {
        analyser.getByteTimeDomainData(buf);
        let sum = 0;
        for (let i = 0; i < buf.length; i++) {
          const v = (buf[i] - 128) / 128;
          sum += v * v;
        }
        const rms = Math.sqrt(sum / buf.length);
        target = Math.min(1.4, rms * 9);
      }
      intensity = target;
      if (blobPathRef.current) {
        blobPathRef.current.setAttribute(
          "d",
          buildBlobPath(t, intensity, BLOB_PRIMARY),
        );
      }
      if (blobPathRef2.current) {
        blobPathRef2.current.setAttribute(
          "d",
          buildBlobPath(t, intensity, BLOB_SECONDARY),
        );
      }
      // Stationary waveform: bar i represents the amplitude at i/N of playback.
      const audioEl = audioElRef.current;
      let playheadIdx = -1;
      if (audioEl && audioEl.duration > 0 && isPlayingRef.current) {
        const progress = Math.min(
          0.9999,
          audioEl.currentTime / audioEl.duration,
        );
        playheadIdx = Math.floor(progress * WAVE_BARS);
        if (playheadIdx !== lastPlayheadIdx) {
          // First frame in this bar — overwrite any prior value from a past play
          waveHistoryRef.current[playheadIdx] = intensity;
          lastPlayheadIdx = playheadIdx;
        } else if (intensity > waveHistoryRef.current[playheadIdx]) {
          // Same bar — keep the peak across this bar's window
          waveHistoryRef.current[playheadIdx] = intensity;
        }
      } else {
        lastPlayheadIdx = -1;
      }
      const waveContainer = waveContainerRef.current;
      if (waveContainer) {
        for (let p = 0; p < WAVE_BARS; p++) {
          const amp = waveHistoryRef.current[p];
          const bar = waveContainer.children[p] as
            | HTMLDivElement
            | undefined;
          if (bar) {
            const hoverWobble = hoverRef.current
              ? (Math.sin(t * 0.008 + p * 0.7) +
                  Math.sin(t * 0.008 - p * 0.7) +
                  2) *
                0.75
              : 0;
            bar.style.height = `${Math.max(3, amp * WAVE_MAX_HEIGHT) + hoverWobble}px`;
            // Bars at or before the playhead are bright; ahead are dim
            const reached = playheadIdx < 0 ? false : p <= playheadIdx;
            bar.style.opacity = reached ? "1" : "0.2";
          }
        }
      }
      const active = isPlayingRef.current || hoverRef.current;
      if (active) {
        idleFramesAfterStop = 0;
        raf = requestAnimationFrame(tick);
      } else if (idleFramesAfterStop < 1) {
        // Run one more frame after stopping so the final state (flat circle,
        // dim bars) is rendered before we let the loop sleep.
        idleFramesAfterStop++;
        raf = requestAnimationFrame(tick);
      } else {
        rafRunningRef.current = false;
      }
    };
    const kick = () => {
      if (rafRunningRef.current) return;
      rafRunningRef.current = true;
      idleFramesAfterStop = 0;
      raf = requestAnimationFrame(tick);
    };
    kickRafRef.current = kick;
    // Render one initial frame so the blob is drawn even when idle.
    kick();
    return () => {
      cancelAnimationFrame(raf);
      rafRunningRef.current = false;
    };
  }, [visualizer]);

  // Keep ref in sync with state
  useEffect(() => {
    isPlayingRef.current = isPlaying;
    if (isPlaying) kickRafRef.current();
  }, [isPlaying]);

  // Show toast when authentication is needed
  useEffect(() => {
    if (needsAuth && clickedRef.current) {
      toast.error("Please log in to play audio", {
        description:
          "Audio playback requires an account to access the text-to-speech service.",
        duration: 5000,
      });
      setNeedsAuth(false); // Reset the state
    }
  }, [needsAuth]);

  const handlePlayAudio = useCallback(
    async (e?: React.MouseEvent) => {
      e?.stopPropagation();
      if (isPlayingRef.current) return;

      const controller = new AbortController();
      abortControllersRef.current.add(controller);
      const { signal } = controller;

      const isAbort = (error: unknown) =>
        error instanceof DOMException && error.name === "AbortError";

      setIsPlaying(true);
      try {
        // Wait for any currently playing sound effects to finish
        while (isSoundEffectPlaying()) {
          if (signal.aborted) throw new DOMException("Aborted", "AbortError");
          await new Promise((resolve) => setTimeout(resolve, 50));
        }

        // Play pre-audio if enabled (to wake up Bluetooth headphones in low power mode)
        if (playPreAudio) {
          try {
            const preAudio = new Audio("/pre-audio.mp3");
            const onAbort = () => {
              try {
                preAudio.pause();
                preAudio.src = "";
              } catch {
                // ignore
              }
            };
            signal.addEventListener("abort", onAbort, { once: true });
            try {
              await new Promise<void>((resolve, reject) => {
                preAudio.onended = () => resolve();
                preAudio.onerror = () =>
                  reject(new Error("Failed to play pre-audio"));
                signal.addEventListener(
                  "abort",
                  () => reject(new DOMException("Aborted", "AbortError")),
                  { once: true },
                );
                preAudio.play().catch(reject);
              });
            } finally {
              signal.removeEventListener("abort", onAbort);
            }
          } catch (error) {
            if (isAbort(error)) throw error;
            console.error("Failed to play pre-audio:", error);
          }
        }

        if (signal.aborted) throw new DOMException("Aborted", "AbortError");

        const authCallback = () => {
          if (clickedRef.current) {
            setNeedsAuth(true);
          }
        };
        if (temp) {
          await playTempAudio(audioRequest, accessToken, authCallback, signal);
        } else {
          await playAudio(
            audioRequest,
            accessToken,
            authCallback,
            visualizer ? attachAnalyser : undefined,
            signal,
          );
        }
        if (!signal.aborted) onSuccessRef.current?.();
      } catch (error) {
        if (isAbort(error) || signal.aborted) return;
        console.error("Failed to play audio:", error);
        onErrorRef.current?.();
      } finally {
        abortControllersRef.current.delete(controller);
        if (!signal.aborted) setIsPlaying(false);
      }
    },
    [audioRequest, accessToken, playPreAudio, visualizer, attachAnalyser, temp],
  );

  // Read latest onSuccess/onError via refs so handlePlayAudio stays stable —
  // otherwise the autoplay effect re-runs every parent render and cancels
  // its own in-flight playback.
  const onSuccessRef = useRef(onSuccess);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    onSuccessRef.current = onSuccess;
    onErrorRef.current = onError;
  });

  // Auto-play audio when text changes (if autoPlay is enabled)
  useEffect(() => {
    if (!autoPlay) return;
    if (autoplayed) {
      return;
    }

    let cancelled = false;

    const playWithDelay = async () => {
      // Only autoplay if user has interacted with the page and it's focused
      if (!navigator.userActivation?.hasBeenActive || !document.hasFocus()) {
        return;
      }

      // Wait for any currently playing sound effects to finish
      while (isSoundEffectPlaying() && !cancelled) {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }

      // Check if we should still play and we're not already playing
      if (!cancelled && !isPlayingRef.current) {
        if (setAutoplayed) {
          setAutoplayed();
        }
        handlePlayAudio();
      }
    };

    playWithDelay();

    // Cleanup function to prevent race conditions
    return () => {
      cancelled = true;
    };
  }, [
    audioRequest.request.text,
    audioRequest.request.language,
    audioRequest.provider,
    autoPlay,
    handlePlayAudio,
    autoplayed,
    setAutoplayed,
  ]);

  const button = (
    <Button
      variant={variant}
      size={size}
      className={className}
      onClick={(e) => {
        e.stopPropagation();
        clickedRef.current = true;
        handlePlayAudio();
      }}
      disabled={isPlaying}
      title="Play pronunciation"
    >
      <Volume2
        className={`h-6 w-6 ${isPlaying ? "animate-pulse" : ""} size--xl`}
      />
    </Button>
  );

  if (!visualizer) return button;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => {
        clickedRef.current = true;
        handlePlayAudio();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          clickedRef.current = true;
          handlePlayAudio();
        }
      }}
      onMouseEnter={() => {
        hoverRef.current = true;
        kickRafRef.current();
      }}
      onMouseLeave={() => {
        hoverRef.current = false;
      }}
      className="inline-flex cursor-pointer items-center gap-3"
      title="Play pronunciation"
    >
      <div className="relative flex h-32 w-32 items-center justify-center">
        <svg
          viewBox="0 0 120 120"
          className="pointer-events-none absolute inset-0 h-full w-full"
        >
          <path
            ref={blobPathRef}
            d={buildBlobPath(0, 0, BLOB_PRIMARY)}
            fill="none"
            className="stroke-primary/70"
            strokeWidth="2.5"
            strokeLinejoin="round"
          />
          <path
            ref={blobPathRef2}
            d={buildBlobPath(0, 0, BLOB_SECONDARY)}
            fill="none"
            className="stroke-primary/35"
            strokeWidth="2"
            strokeLinejoin="round"
          />
        </svg>
        {button}
      </div>
      {visualizer !== "radial" && (
        <div
          ref={waveContainerRef}
          className="pointer-events-none flex h-12 items-center gap-0.5"
          aria-hidden
        >
          {Array.from({ length: WAVE_BARS }).map((_, i) => (
            <div
              key={i}
              className="w-1 rounded-full bg-primary transition-opacity duration-500 ease-out"
              style={{ height: "3px", opacity: 0.2 }}
            />
          ))}
        </div>
      )}
    </div>
  );
}
