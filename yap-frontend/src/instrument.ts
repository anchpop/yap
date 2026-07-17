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

    // Filter third-party analytics beacon errors (not our code).
    const frames = event.exception?.values?.[0]?.stacktrace?.frames ?? [];
    if (frames.some((f) => f.filename?.includes("beacon.min.js"))) {
      return null;
    }

    // Filter WASM fetch/compile failures caused by flaky networks or browsers
    // that predate WebAssembly / reference types. Nothing we can do about
    // these beyond the existing browser-support check, which only runs once
    // the module has already loaded.
    if (
      message.includes("WebAssembly is not defined") ||
      message.includes("WebAssembly compilation aborted") ||
      message.includes("WebAssembly.instantiateStreaming")
    ) {
      return null;
    }

    // Filter WASM load failures. Identifiable by the generic network-error
    // text browsers use for a failed fetch ("Load failed" on iOS/WKWebView,
    // "Failed to fetch" on Chromium, "NetworkError" on Firefox) together with
    // a WASM file in the stack.
    if (
      (message.includes("Load failed") ||
        message.includes("Failed to fetch") ||
        message.includes("NetworkError")) &&
      frames.some(
        (f) =>
          f.filename?.includes("wasm-helper") ||
          f.filename?.includes("_bg.wasm"),
      )
    ) {
      return null;
    }

    // Filter IndexedDB backing-store errors. This is a known browser/OS-level
    // storage engine fault (not triggered by any of our own indexedDB calls)
    // seen on constrained or aging devices — there's no app-level recovery.
    if (message.includes("Internal error opening backing store")) {
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
