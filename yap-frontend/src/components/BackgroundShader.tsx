import { useRef, useMemo, memo, useCallback, type ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { useTheme } from "./theme-provider";
import { getShaderBackgroundCss } from "@/lib/shader-colors";
import { shaderAvailable, type ShaderHandle } from "@/lib/shader-background";
import { ShaderCanvas, ShaderTexture } from "./shader-canvas";
import { useShaderTheme } from "./use-shader-theme";
import { BackgroundContext } from "./background-context";

interface BackgroundShaderProps {
  children: ReactNode;
}

function BackgroundShaderComponent({ children }: BackgroundShaderProps) {
  const handle = useRef<ShaderHandle | null>(null);
  const { animatedBackground } = useTheme();
  const theme = useShaderTheme();
  const location = useLocation();
  const blurBackground = location.pathname === "/select-language";
  // The landing page paints its own full-bleed scene, so the shader is
  // neither visible nor wanted there (it would only peek out on overscroll).
  const onLanding = location.pathname === "/";

  const shouldRender = useMemo(
    () => !onLanding && shaderAvailable(animatedBackground),
    [animatedBackground, onLanding],
  );

  const bumpBackground = useCallback((multiplier?: number) => {
    handle.current?.bump(multiplier);
  }, []);
  const onMount = useCallback((h: ShaderHandle | null) => {
    handle.current = h;
  }, []);

  return (
    <BackgroundContext.Provider value={{ bumpBackground }}>
      {!onLanding && (
        <div
          className="fixed -inset-10 -z-10 transition-[filter] duration-700 ease-in-out"
          style={{
            filter: blurBackground ? "blur(20px)" : "blur(0px)",
            pointerEvents: "none",
            backgroundColor: getShaderBackgroundCss(theme),
          }}
        >
          {shouldRender && (
            <>
              <ShaderCanvas
                className="fixed inset-0 h-full w-full"
                onMount={onMount}
              />
              <ShaderTexture className="fixed inset-0 h-full w-full" />
            </>
          )}
        </div>
      )}
      {children}
    </BackgroundContext.Provider>
  );
}

export const BackgroundShader = memo(BackgroundShaderComponent);
