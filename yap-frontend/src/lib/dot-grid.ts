// The landing's swishable dot grid: simulated and drawn by a worker (see
// dotGrid.worker.ts), fed the pointer's position and velocity from here.
import type { ShaderTheme } from "./shader-colors";
import { mountWorkerCanvas } from "./worker-canvas";

export interface DotGridHandle {
  setTheme(theme: ShaderTheme): void;
  stop(): void;
}

/** Fill `container` (which must be positioned) with the dot grid. The
 *  pointer stirs it while the container is on screen. */
export function mountDotGrid(
  container: HTMLElement,
  theme: ShaderTheme,
): DotGridHandle {
  const worker = new Worker(
    new URL("../workers/dotGrid.worker.ts", import.meta.url),
    { type: "module" },
  );
  const mounted = mountWorkerCanvas(container, worker, { theme });

  // Velocity comes from consecutive samples; a fresh start after the
  // pointer has been away, so a jump does not read as a flick.
  let last: { x: number; y: number; t: number } | null = null;
  const onPointerMove = (e: PointerEvent) => {
    if (!mounted.onScreen()) return;
    const r = container.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    const t = e.timeStamp;
    if (last && t > last.t && t - last.t < 100) {
      const dt = Math.max(4, t - last.t) / 1000;
      worker.postMessage({
        type: "pointer",
        x,
        y,
        vx: (x - last.x) / dt,
        vy: (y - last.y) / dt,
      });
    }
    last = { x, y, t };
  };
  const onPointerLeave = () => {
    last = null;
  };
  window.addEventListener("pointermove", onPointerMove);
  document.documentElement.addEventListener("pointerleave", onPointerLeave);

  return {
    setTheme: (t) => worker.postMessage({ type: "theme", theme: t }),
    stop() {
      window.removeEventListener("pointermove", onPointerMove);
      document.documentElement.removeEventListener(
        "pointerleave",
        onPointerLeave,
      );
      mounted.stop();
    },
  };
}
