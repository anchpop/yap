import "./instrument"; // Sentry must init before anything else

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { reactErrorHandler } from "@sentry/react";
import "@fontsource-variable/nunito";
import "@fontsource-variable/nunito-sans";
import "./index.css";

// Hide the loading screen once React mounts
if (typeof window !== "undefined" && (window as any).hideLoadingScreen) {
  (window as any).hideLoadingScreen();
}

const root = createRoot(document.getElementById("root")!, {
  onUncaughtError: reactErrorHandler(),
  onCaughtError: reactErrorHandler(),
  onRecoverableError: reactErrorHandler(),
});

// App.tsx (and its dependency graph) statically imports the WASM module, which
// references the global `WebAssembly` object as soon as it's evaluated. On
// browsers/environments where WebAssembly is unavailable (e.g. hardened
// privacy browsers), that import throws a ReferenceError before React ever
// mounts. Check for WebAssembly first and dynamically import App only when
// it's present, so unsupported browsers get the normal "not supported" screen
// instead of a blank crash.
if (typeof WebAssembly === "undefined") {
  import("./components/browser-not-supported").then(
    ({ BrowserNotSupported }) => {
      root.render(
        <StrictMode>
          <BrowserNotSupported />
        </StrictMode>,
      );
    },
  );
} else {
  import("./App.tsx").then(({ default: App }) => {
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  });
}
