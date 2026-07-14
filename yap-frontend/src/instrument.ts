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

    // Filter WASM fetch failures on iOS/WKWebView. These are transient network
    // errors during .wasm module initialization — identifiable by "Load failed"
    // (the iOS Safari message for a failed fetch) with a WASM file in the stack.
    if (message === "Load failed") {
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

    // Filter other known-unactionable WASM bootstrap failures. The WASM
    // module uses top-level await, so a rejection here escapes every
    // try/catch in our code and lands directly in the global handlers —
    // there's no app-level place to catch it short of turning every static
    // WASM import into a dynamic one.
    if (
      message.includes("WebAssembly compilation aborted") || // network interrupted the streaming compile
      message.includes("WebAssembly is not defined") // environment strips/lacks the global entirely
    ) {
      return null;
    }

    // Filter ServiceWorker install/update failures caused by the browser
    // being unable to fetch sw.js (flaky network, offline, blocked by an
    // extension). Browsers disagree on the exception name for this — Chrome,
    // Firefox, and Safari each surface a different `.name` (or none at all)
    // for the same underlying condition — so match on the browser-generated
    // message text instead, which is consistent across them.
    if (
      message.includes("yap.town") &&
      (message.includes("encountered an error during installation") ||
        message.startsWith("Failed to update a ServiceWorker for scope") ||
        /^Script .* load failed$/.test(message))
    ) {
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
