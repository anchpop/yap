import { useEffect, useRef } from "react";
import { useShaderTheme } from "./use-shader-theme";
import { cn } from "@/lib/utils";
import { mountDotGrid, type DotGridHandle } from "@/lib/dot-grid";

/** The swishable dot grid, filling a positioned box. */
export function DotGridCanvas({ className }: { className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const own = useRef<DotGridHandle | null>(null);
  const theme = useShaderTheme();

  // The mount reads the theme through a ref so it survives its changes.
  const themeRef = useRef(theme);
  useEffect(() => {
    themeRef.current = theme;
    own.current?.setTheme(theme);
  }, [theme]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const handle = mountDotGrid(el, themeRef.current);
    own.current = handle;
    return () => {
      handle.stop();
      own.current = null;
    };
  }, []);

  return (
    <div
      ref={ref}
      aria-hidden
      className={cn("pointer-events-none", className)}
    />
  );
}
