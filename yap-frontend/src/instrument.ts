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
    if (
      message.includes("runtime.sendMessage()") ||
      message.includes("__firefox__.reader") ||
      message.includes("Can't find variable: DarkReader")
    ) {
      return null;
    }

    // Filter WASM init failures caused by the environment, not our code:
    // transient network errors fetching/compiling the .wasm module (iOS
    // Safari's "Load failed", Chrome's "WebAssembly compilation aborted:
    // Network error"), and embedded webviews (e.g. Twitter's in-app browser)
    // that don't expose a WebAssembly global at all. These already surface a
    // friendly reload prompt via the page-level listener in index.html.
    if (message.includes("Can't find variable: WebAssembly")) {
      return null;
    }
    if (
      message.includes("Load failed") ||
      message.includes("WebAssembly compilation aborted")
    ) {
      const frames = event.exception?.values?.[0]?.stacktrace?.frames ?? [];
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

    return event;
  },

  // Tracing
  tracesSampleRate: 0.2,

  // Session Replay
  replaysSessionSampleRate: 0.1,
  replaysOnErrorSampleRate: 1.0,
});
