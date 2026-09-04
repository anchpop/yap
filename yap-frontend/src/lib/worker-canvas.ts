// A canvas drawn by a worker. The worker gets an OffscreenCanvas and is told
// the box's size whenever it changes; the main thread never waits on the
// drawing. Shared by the app's background shader and the landing's dot grid.

export interface WorkerCanvas {
  worker: Worker;
  /** Whether the box is in the viewport right now. */
  onScreen(): boolean;
  /** Ends the worker and removes the canvas. */
  stop(): void;
}

export interface CanvasSize {
  width: number;
  height: number;
  devicePixelRatio: number;
}

/** Fill `container` (which must be positioned) with a canvas run by
 *  `worker`. The worker receives `{type: "init", canvas, ...init}` with the
 *  size folded in, then `{type: "resize", width, height, devicePixelRatio}`
 *  as the box changes, and `{type: "stop"}` before it is terminated. */
export function mountWorkerCanvas(
  container: HTMLElement,
  worker: Worker,
  init: Record<string, unknown>,
): WorkerCanvas {
  // A canvas whose control has been transferred can never be drawn to
  // again, so each mount gets a fresh one.
  const canvas = document.createElement("canvas");
  canvas.className = "absolute inset-0 h-full w-full";
  canvas.style.pointerEvents = "none";
  canvas.style.willChange = "contents";
  canvas.style.transform = "translateZ(0)";
  container.appendChild(canvas);

  const size = (): CanvasSize => {
    const r = container.getBoundingClientRect();
    return {
      width: r.width,
      height: r.height,
      devicePixelRatio: window.devicePixelRatio || 1,
    };
  };
  worker.addEventListener("error", (e) =>
    console.error("worker canvas", e.message, e.filename, e.lineno),
  );
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ type: "init", canvas: offscreen, ...size(), ...init }, [
    offscreen,
  ]);

  const resize = new ResizeObserver(() => {
    worker.postMessage({ type: "resize", ...size() });
  });
  resize.observe(container);

  let onScreen = true;
  const visibility = new IntersectionObserver(([entry]) => {
    onScreen = entry.isIntersecting;
  });
  visibility.observe(container);

  return {
    worker,
    onScreen: () => onScreen,
    stop() {
      resize.disconnect();
      visibility.disconnect();
      worker.postMessage({ type: "stop" });
      worker.terminate();
      canvas.remove();
    },
  };
}
