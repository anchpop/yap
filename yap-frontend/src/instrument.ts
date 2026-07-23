import * as Sentry from "@sentry/react";

Sentry.init({
  dsn: "https://46ad67fa41ae7cafe1048ba1c4c41994@o4511102905090048.ingest.us.sentry.io/4511102907056128",
  enabled: import.meta.env.PROD,
  environment: import.meta.env.MODE,

  sendDefaultPii: true,
  tunnel: "https://yap-ai-backend.fly.dev/sentry-tunnel",

  integrations: [
    Sentry.browserTracingIntegration(),
    Sentry.replayIntegration({
      maskAllText: false,
      blockAllMedia: false,
    }),
  ],

  beforeSend(event) {
    const message = event.exception?.values?.[0]?.value ?? "";

    // Filter out browser extension errors (not our code)
    if (message.includes("runtime.sendMessage()")) {
      return null;
    }

    // Filter WASM module load failures. These are all transient/environmental
    // conditions during .wasm module initialization, not application bugs:
    // "Load failed" (Safari's fetch failure message), "Failed to fetch"
    // (Chrome/Firefox's), and "WebAssembly is not defined" (extensions or
    // embedded webviews that strip the WebAssembly global). Identified by the
    // vite-generated WASM loader appearing in the stack — these messages are
    // generic enough (e.g. "Load failed" also covers other fetches) that we
    // only drop them when the WASM loader is actually on the stack.
    if (
      message === "Load failed" ||
      message === "Failed to fetch" ||
      message === "WebAssembly is not defined"
    ) {
      const frames =
        event.exception?.values?.[0]?.stacktrace?.frames ?? [];
      if (
        frames.some(
          (f) =>
            f.filename?.includes("wasm-helper") ||
            f.filename?.includes("_bg.wasm"),
        )
      ) {
        return null;
      }
    }

    // Chrome's own message when `WebAssembly.instantiateStreaming()` gets cut
    // off mid-download — unambiguously our single WASM module load (that's
    // the only streaming-instantiate call site in the app), so no frame check
    // needed here.
    if (message.startsWith("WebAssembly compilation aborted")) {
      return null;
    }

    return event;
  },

  // Tracing
  tracesSampleRate: 0.2,

  // Session Replay
  replaysSessionSampleRate: 0.1,
  replaysOnErrorSampleRate: 1.0,
});
