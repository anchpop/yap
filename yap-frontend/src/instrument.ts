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
    // (the iOS Safari message for a failed fetch, sometimes re-wrapped by
    // wasm-bindgen as "TypeError: Load failed") with a WASM file in the stack.
    // The module's top-level await means the rejection often carries no
    // captured call stack at all, so an empty stacktrace also counts as a match.
    if (message === "Load failed" || message === "TypeError: Load failed") {
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

    // Chrome's equivalent of the above: the .wasm fetch's response stream
    // aborts mid-flight, so WebAssembly.compile/instantiate reject with this.
    if (message.startsWith("WebAssembly compilation aborted") ||
        message.startsWith("WebAssembly instantiation aborted")) {
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
