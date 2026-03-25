import * as Sentry from "@sentry/react";

Sentry.init({
  dsn: "https://46ad67fa41ae7cafe1048ba1c4c41994@o4511102905090048.ingest.us.sentry.io/4511102907056128",
  environment: import.meta.env.MODE,

  sendDefaultPii: true,
  debug: import.meta.env.DEV,
  tunnel: "https://yap-ai-backend.fly.dev/sentry-tunnel",

  integrations: [
    Sentry.browserTracingIntegration(),
    Sentry.replayIntegration({
      maskAllText: false,
      blockAllMedia: false,
    }),
  ],

  // Tracing
  tracesSampleRate: import.meta.env.PROD ? 0.2 : 1.0,

  // Session Replay
  replaysSessionSampleRate: 0.1,
  replaysOnErrorSampleRate: 1.0,
});
