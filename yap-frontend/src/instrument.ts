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

    // Filter transient network errors during .wasm module initialization. The
    // WASM module is pulled in via a top-level await in generated wasm-bindgen
    // glue, so a dropped connection during the fetch/streaming-compile surfaces
    // as an uncatchable global error rather than a promise our code could
    // .catch(). These frequently have no stacktrace, since they happen during
    // module evaluation before any of our code runs.
    if (message.includes("WebAssembly compilation aborted: Network error")) {
      return null;
    }
    if (message.includes("Load failed")) {
      const frames =
        event.exception?.values?.[0]?.stacktrace?.frames ?? [];
      if (
        frames.length === 0 ||
        frames.some(
          (f) =>
            f.filename?.includes("wasm-helper") ||
            f.filename?.includes("_bg.wasm"),
        )
      ) {
        return null;
      }
    }

    return event;
  },

  // Tracing
  tracesSampleRate: 0.2,

  // Session Replay
  replaysSessionSampleRate: 0.1,
  replaysOnErrorSampleRate: 1.0,
});
