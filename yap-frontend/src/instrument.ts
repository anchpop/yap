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
    // Filter out browser extension errors (not our code)
    const message = event.exception?.values?.[0]?.value ?? "";
    if (message.includes("runtime.sendMessage()")) {
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
