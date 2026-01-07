import { useState, useRef, useEffect } from 'react';

type UseZenoOptions = {
  /** Decay rate - higher = faster approach. Default 8 */
  lambda?: number;
  /** Threshold to snap and stop the loop. Default 0.1 */
  epsilon?: number;
};

export function useZeno(
  target: number,
  { lambda = 8, epsilon = 0.1 }: UseZenoOptions = {}
): number {
  const [display, setDisplay] = useState(target);
  const current = useRef(target);
  const lastTime = useRef<number | null>(null);
  const rafId = useRef<number | undefined>(undefined);

  useEffect(() => {
    const tick = (now: number) => {
      if (lastTime.current === null) {
        lastTime.current = now;
      }

      const dt = (now - lastTime.current) / 1000; // seconds
      lastTime.current = now;

      // Exponential decay: current = target + (current - target) * e^(-λt)
      const delta = current.current - target;
      const decay = Math.exp(-lambda * dt);
      const next = target + delta * decay;

      // Close enough? Snap and stop.
      if (Math.abs(next - target) < epsilon) {
        current.current = target;
        setDisplay(target);
        lastTime.current = null;
        rafId.current = undefined;
        return;
      }

      current.current = next;
      setDisplay(next);
      rafId.current = requestAnimationFrame(tick);
    };

    // Only start loop if we're not already at target
    if (Math.abs(current.current - target) >= epsilon) {
      lastTime.current = null; // reset timing on new target
      rafId.current = requestAnimationFrame(tick);
    }

    return () => {
      if (rafId.current) {
        cancelAnimationFrame(rafId.current);
      }
    };
  }, [target, lambda, epsilon]);

  return display;
}
