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
    const frames = event.exception?.values?.[0]?.stacktrace?.frames ?? [];

    // Filter out browser extension errors (not our code)
    if (message.includes("runtime.sendMessage()")) {
      return null;
    }

    // Filter errors thrown from Cloudflare Web Analytics' beacon.min.js — a
    // third-party script we don't control, injected outside our source tree.
    // Old browsers (e.g. Chrome <92 lacking Array.prototype.at) crash inside
    // it with nothing for us to fix.
    if (frames.some((f) => f.filename?.includes("beacon.min.js"))) {
      return null;
    }

    // Filter WASM fetch/compile failures caused by a dropped network
    // connection during .wasm module initialization — identifiable by
    // "Load failed" (the iOS Safari message for a failed fetch) with a WASM
    // file in the stack, or by Chrome's "WebAssembly compilation aborted"
    // message when streaming instantiation is interrupted mid-transfer.
    if (message === "Load failed") {
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
