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

    // Filter connection-closed AbortErrors. These fire (as an unhandled
    // rejection with no stacktrace) when the browser tears down an in-flight
    // request during page unload/navigation — most visibly alongside the
    // service worker's own update check being aborted for the same reason.
    // Not actionable: there's no app code on the other end of this rejection.
    if (message === "AbortError: The connection was closed.") {
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

    return event;
  },

  // Tracing
  tracesSampleRate: 0.2,

  // Session Replay
  replaysSessionSampleRate: 0.1,
  replaysOnErrorSampleRate: 1.0,
});
