import { useMemo } from "react";
import { useTheme } from "./theme-provider";
import type { ShaderTheme } from "@/lib/shader-colors";

/** The theme the shader should draw: the app's, with "system" resolved. */
export function useShaderTheme(): ShaderTheme {
  const { theme } = useTheme();
  return useMemo(
    () =>
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme,
    [theme],
  );
}
