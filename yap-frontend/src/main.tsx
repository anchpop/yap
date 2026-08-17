import "./instrument"; // Sentry must init before anything else

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { captureMessage, reactErrorHandler } from "@sentry/react";
import { get_ai_server_url } from "../../yap-frontend-rs/pkg";
import "@fontsource-variable/nunito";
import "@fontsource-variable/nunito-sans";
import "./index.css";
import App from "./App.tsx";

// A WASM built with the `local-backend` feature points every AI-backend call
// at localhost — if that build is running on a real domain, autograde/TTS are
// broken for everyone, so make it loud in Sentry. (Sentry is disabled in dev,
// so local use of the feature stays silent.)
{
  const aiServerUrl = get_ai_server_url();
  if (
    aiServerUrl.includes("localhost") &&
    !["localhost", "127.0.0.1"].includes(window.location.hostname)
  ) {
    captureMessage(
      `local-backend WASM deployed to ${window.location.hostname}: AI backend is ${aiServerUrl}`,
      "fatal",
    );
  }
}

// Hide the loading screen once React mounts
if (typeof window !== "undefined" && (window as any).hideLoadingScreen) {
  (window as any).hideLoadingScreen();
}

createRoot(document.getElementById("root")!, {
  onUncaughtError: reactErrorHandler(),
  onCaughtError: reactErrorHandler(),
  onRecoverableError: reactErrorHandler(),
}).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
