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
    const value = event.exception?.values?.[0];
    const message = value?.value ?? "";
    const type = value?.type ?? "";

    // Filter out browser extension errors (not our code)
    if (message.includes("runtime.sendMessage()")) {
      return null;
    }

    // Filter WASM module init failures: transient network errors while
    // fetching/instantiating the .wasm binary (e.g. "Load failed", the
    // iOS Safari/WKWebView message for a failed fetch), and browsers that
    // lack a WebAssembly global entirely. These come from Vite's wasm-loader
    // helper, whose stack frames are only resolved to a recognizable
    // filename after Sentry applies sourcemaps server-side — beforeSend only
    // ever sees the raw bundled chunk URL, so match on message text instead.
    if (message === "Load failed" || message === "WebAssembly is not defined") {
      return null;
    }

    // Service worker update checks interrupted by a dropped connection —
    // transient and not actionable, same class as the TypeError/
    // InvalidStateError already excluded around the sw.update() call.
    if (
      type === "AbortError" &&
      (message.includes("The connection was closed") ||
        message.includes("Failed to update a ServiceWorker"))
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
