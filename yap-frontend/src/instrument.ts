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

    // Filter out errors from crypto-wallet / reader-mode / dark-mode browser
    // extensions injecting globals into the page (window.ethereum, DarkReader,
    // Firefox's __firefox__ reader helper). These fire from injected scripts,
    // not our code, and there is nothing we can do about them.
    if (
      message.includes("window.ethereum") ||
      message.includes("DarkReader") ||
      message.includes("__firefox__")
    ) {
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

    // Filter WASM streaming-compile failures caused by the connection
    // dropping mid-download — same class of transient network issue as the
    // "Load failed" case above, just surfaced with a different message.
    if (message.startsWith("WebAssembly compilation aborted: Network error")) {
      return null;
    }

    // Browsers/in-app webviews that don't implement WebAssembly at all can't
    // run this app no matter what. The WASM module is imported at the top of
    // our entry chunk (statically, across the whole codebase), so this fails
    // before our browserSupported check in weapon.tsx ever gets a chance to
    // run and show the "browser not supported" screen — there's no code fix
    // short of restructuring every import in the app to be lazy.
    if (
      message === "Can't find variable: WebAssembly" ||
      message === "WebAssembly is not defined"
    ) {
      return null;
    }

    // Environmental audio-hardware failure surfaced by Howler's internal
    // AudioContext setup (e.g. the OS refuses to open an audio session).
    // Not something our code triggers or can recover from.
    if (message === "Failed to start the audio device") {
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
