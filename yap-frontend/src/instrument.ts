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
    // Browser extension errors
    if (message.includes("runtime.sendMessage()")) {
      return null;
    }
    // WASM compile errors from browsers that don't support reference types (e.g. Chrome <79)
    if (message.includes("WebAssembly.instantiateStreaming")) {
      return null;
    }
    // IndexedDB backing store errors from old Android/Chrome versions
    if (message.includes("Internal error opening backing store for indexedDB")) {
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
