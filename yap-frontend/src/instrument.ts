import * as Sentry from "@sentry/react";

// Autograde failures degrade gracefully (the app falls back to manual
// grading), so without an explicit report a broken backend is invisible to
// us. Called from the challenge components where `autograding_error` is
// handled. Offline failures are expected in an offline-first app; skip them.
export function reportAutogradeFailure(
  kind: "translation" | "transcription",
  error: string,
) {
  if (!navigator.onLine) return;
  Sentry.captureMessage(`Autograde failed (${kind})`, {
    level: "error",
    extra: { error },
  });
}

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
    // Backend failures (autograde, TTS, language-stats) are handled gracefully
    // in the app — a console.error and a fallback — so without this they never
    // reach Sentry and an unreachable backend is invisible to us.
    Sentry.captureConsoleIntegration({ levels: ["error"] }),
  ],

  beforeSend(event) {
    // Console errors from an offline device are expected (offline-first app,
    // fetches fail constantly); only report ones that happen while online.
    if (event.logger === "console" && !navigator.onLine) {
      return null;
    }

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

    // Filter cancelled/blocked WebAuthn ceremonies (user closed the passkey
    // sheet, tab lost focus, etc.). Match on exception type, not message —
    // WebAuthn error messages vary by browser and locale.
    {
      const first = event.exception?.values?.[0];
      if (first?.type === "NotAllowedError" || first?.type === "AbortError") {
        const frames = first.stacktrace?.frames ?? [];
        if (frames.some((f) => f.filename?.includes("passkey"))) {
          return null;
        }
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
