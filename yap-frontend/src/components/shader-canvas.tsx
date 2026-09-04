import { useEffect, useRef } from "react";
import { useTheme } from "./theme-provider";
import { useShaderTheme } from "./use-shader-theme";
import { cn } from "@/lib/utils";
import { mountShader, type ShaderHandle } from "@/lib/shader-background";

/** The shader, filling a positioned box. `onMount` hands over the handle
 *  to drive it, and null again when it goes. */
export function ShaderCanvas({
  className,
  onMount,
}: {
  className?: string;
  onMount?: (handle: ShaderHandle | null) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const own = useRef<ShaderHandle | null>(null);
  const theme = useShaderTheme();
  const { mouseFollow } = useTheme();

  // The mount reads these through refs so it survives their changes.
  const themeRef = useRef(theme);
  const mouseFollowRef = useRef(mouseFollow);
  useEffect(() => {
    themeRef.current = theme;
    own.current?.setTheme(theme);
  }, [theme]);
  useEffect(() => {
    mouseFollowRef.current = mouseFollow;
    // Toggled off: ease the sun back to its anchor.
    if (!mouseFollow) own.current?.setMouse(0.5, 0.4);
  }, [mouseFollow]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const handle = mountShader(
      el,
      themeRef.current,
      () => mouseFollowRef.current,
    );
    own.current = handle;
    onMount?.(handle);
    return () => {
      handle.stop();
      own.current = null;
      onMount?.(null);
    };
  }, [onMount]);

  return <div ref={ref} className={cn("pointer-events-none", className)} />;
}

/** The film grain the shader wears: a fog texture by night and a paper one
 *  by day, plus a finer noise on both. Fills a positioned box. */
export function ShaderTexture({ className }: { className?: string }) {
  const theme = useShaderTheme();
  const dark = theme === "dark" || theme === "oled";
  const layer = (image: string, opacity: number) => (
    <div
      className={cn(
        "pointer-events-none bg-cover bg-center bg-no-repeat",
        className,
      )}
      style={{
        opacity,
        backgroundImage: `url(${image})`,
        mixBlendMode: dark ? "multiply" : "screen",
        filter: dark ? "invert(1)" : "none",
      }}
    />
  );
  return (
    <>
      {dark ? layer("/fog.webp", 0.7) : layer("/noise2.webp", 0.3)}
      {layer("/noise.webp", 0.2)}
    </>
  );
}
