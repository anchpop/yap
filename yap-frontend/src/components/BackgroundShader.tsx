import {
  useEffect,
  useRef,
  useMemo,
  memo,
  createContext,
  useContext,
  useCallback,
  type ReactNode,
} from "react";
import { useLocation } from "react-router-dom";
import { useTheme } from "./theme-provider";
import { getShaderBackgroundCss } from "@/lib/shader-colors";

interface BackgroundContextType {
  bumpBackground: (multiplier?: number) => void;
}

const BackgroundContext = createContext<BackgroundContextType | null>(null);

export function useBackground() {
  const context = useContext(BackgroundContext);
  if (!context) {
    throw new Error("useBackground must be used within a BackgroundShader");
  }
  return context;
}

interface BackgroundShaderProps {
  children: ReactNode;
}

function BackgroundShaderComponent({ children }: BackgroundShaderProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const workerRef = useRef<Worker | null>(null);
  const { theme, animatedBackground, mouseFollow } = useTheme();
  const location = useLocation();
  const blurBackground = location.pathname === "/select-language";

  // Superhot mode: on the landing page, mouse motion advances shader time.
  // Tracked via a ref so the pointermove listener can see the latest value
  // without the worker-setup effect being re-run on every navigation.
  const superhotRef = useRef(false);
  useEffect(() => {
    superhotRef.current = location.pathname === "/";
  }, [location.pathname]);

  // Mouse-follow toggle (off by default on dark themes). Tracked via a ref so
  // the pointermove listener sees the latest value without rebuilding the
  // worker. When toggled off we also ease the sun back to its default anchor.
  const mouseFollowRef = useRef(mouseFollow);
  useEffect(() => {
    mouseFollowRef.current = mouseFollow;
    if (!mouseFollow && workerRef.current) {
      workerRef.current.postMessage({ type: "mouse", x: 0.5, y: 0.4 });
    }
  }, [mouseFollow]);

  // Determine actual theme (resolve "system") - memoized to prevent recalculation
  const actualTheme = useMemo(
    () =>
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme,
    [theme],
  );

  // Check accessibility preferences and hardware capabilities
  const shouldRender = useMemo(() => {
    // User preference
    if (!animatedBackground) {
      console.log("animatedBackground is false");
      return false;
    }

    // Disable on low-end devices
    if (navigator.hardwareConcurrency && navigator.hardwareConcurrency < 4) {
      console.log(
        `hardwareConcurrency is less than 4: ${navigator.hardwareConcurrency}`,
      );
      return false;
    }

    // Respect reduced motion preference
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      console.log("prefers-reduced-motion is reduce");
      return false;
    }

    // Respect high contrast preference
    if (window.matchMedia("(prefers-contrast: more)").matches) {
      console.log("prefers-contrast is more");
      return false;
    }

    // Respect reduced transparency preference
    if (window.matchMedia("(prefers-reduced-transparency: reduce)").matches) {
      console.log("prefers-reduced-transparency is reduce");
      return false;
    }

    // Check OffscreenCanvas support (needed for worker-based rendering)
    if (
      typeof HTMLCanvasElement.prototype.transferControlToOffscreen !==
      "function"
    ) {
      return false;
    }

    return true;
  }, [animatedBackground]);

  // Expose bump function to children
  const bumpBackground = useCallback((multiplier?: number) => {
    if (workerRef.current) {
      workerRef.current.postMessage({ type: "bump", multiplier });
    }
  }, []);

  // Set up worker and transfer canvas control
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !shouldRender) return;

    // Create a fresh canvas element for this worker
    const canvas = document.createElement("canvas");
    canvas.className = "fixed inset-0 w-full h-full";
    canvas.style.pointerEvents = "none";
    canvas.style.willChange = "contents";
    canvas.style.transform = "translateZ(0)";
    container.appendChild(canvas);

    // Create worker
    const worker = new Worker(
      new URL("../workers/backgroundShader.worker.ts", import.meta.url),
      { type: "module" },
    );
    workerRef.current = worker;

    // Transfer canvas control to worker
    const offscreenCanvas = canvas.transferControlToOffscreen();

    // Set initial size
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    const isMobile = window.innerWidth < 768;
    const scale = isMobile ? 0.35 : 0.75;
    offscreenCanvas.width = window.innerWidth * dpr * scale;
    offscreenCanvas.height = window.innerHeight * dpr * scale;

    // Initialize worker with canvas and theme
    worker.postMessage(
      {
        type: "init",
        canvas: offscreenCanvas,
        theme: actualTheme,
      },
      [offscreenCanvas],
    );

    // Handle resize events
    const handleResize = () => {
      worker.postMessage({
        type: "resize",
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio || 1,
      });
    };

    // Drive the "sun" anchor from the cursor; y is flipped to match the
    // shader's bottom-up UV. No-mouse / touch devices simply never fire this
    // and the shader stays at its default anchor. In superhot mode (landing
    // page), each move also nudges shader time forward via a bump.
    const handlePointerMove = (e: PointerEvent) => {
      if (e.pointerType !== "mouse") return;
      if (mouseFollowRef.current) {
        worker.postMessage({
          type: "mouse",
          x: e.clientX / window.innerWidth,
          y: 1 - e.clientY / window.innerHeight,
        });
      }
      if (superhotRef.current) {
        // multiplier ≈ 33 makes iTime advance at roughly real-time during
        // motion, which is the scale the wave drift constants assume.
        worker.postMessage({ type: "bump", multiplier: 35.0 });
      }
    };

    // Mouse left the window — ease back to the default anchor.
    const handlePointerLeave = () => {
      worker.postMessage({ type: "mouse", x: 0.5, y: 0.4 });
    };

    window.addEventListener("resize", handleResize);
    window.addEventListener("pointermove", handlePointerMove);
    document.documentElement.addEventListener("pointerleave", handlePointerLeave);

    // Cleanup
    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("pointermove", handlePointerMove);
      document.documentElement.removeEventListener(
        "pointerleave",
        handlePointerLeave,
      );
      worker.postMessage({ type: "stop" });
      worker.terminate();
      workerRef.current = null;
      if (container.contains(canvas)) {
        container.removeChild(canvas);
      }
    };
  }, [shouldRender]); // Only re-run when shouldRender changes

  // Handle theme changes separately without recreating worker
  useEffect(() => {
    if (workerRef.current && shouldRender) {
      workerRef.current.postMessage({ type: "theme", theme: actualTheme });
    }
  }, [actualTheme, shouldRender]);

  const shaderBgColor = useMemo(
    () => getShaderBackgroundCss(actualTheme),
    [actualTheme],
  );

  return (
    <BackgroundContext.Provider value={{ bumpBackground }}>
      <div
        className="fixed -inset-10 -z-10 transition-[filter] duration-700 ease-in-out"
        style={{
          filter: blurBackground ? "blur(20px)" : "blur(0px)",
          pointerEvents: "none",
          backgroundColor: shaderBgColor,
        }}
      >
        {shouldRender && (
          <>
            <div ref={containerRef} className="contents" />
            <div
              className="fixed inset-0 w-full h-full opacity-[0.7]"
              style={{
                pointerEvents: "none",
                backgroundImage:
                  actualTheme === "dark" || actualTheme === "oled"
                    ? "url(/fog.webp)"
                    : "url(/noise2.webp)",
                backgroundRepeat: "no-repeat",
                backgroundSize: "cover",
                backgroundPosition: "center",
                mixBlendMode:
                  actualTheme === "dark" || actualTheme === "oled"
                    ? "multiply"
                    : "screen",
                filter:
                  actualTheme === "dark" || actualTheme === "oled"
                    ? "invert(1)"
                    : "none",
              }}
            />
            <div
              className="fixed inset-0 w-full h-full opacity-[0.20]"
              style={{
                pointerEvents: "none",
                backgroundImage: "url(/noise.webp)",
                backgroundRepeat: "no-repeat",
                backgroundSize: "cover",
                backgroundPosition: "center",
                mixBlendMode:
                  actualTheme === "dark" || actualTheme === "oled"
                    ? "multiply"
                    : "screen",
                filter:
                  actualTheme === "dark" || actualTheme === "oled"
                    ? "invert(1)"
                    : "none",
              }}
            />
          </>
        )}
      </div>
      {children}
    </BackgroundContext.Provider>
  );
}

export const BackgroundShader = memo(BackgroundShaderComponent);
