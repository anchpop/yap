import "./instrument"; // Sentry must init before anything else

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { reactErrorHandler } from "@sentry/react";
import "@fontsource-variable/nunito";
import "@fontsource-variable/nunito-sans";
import "./index.css";

function hideLoadingScreen() {
  if (typeof window !== "undefined" && (window as any).hideLoadingScreen) {
    (window as any).hideLoadingScreen();
  }
}

const root = createRoot(document.getElementById("root")!, {
  onUncaughtError: reactErrorHandler(),
  onCaughtError: reactErrorHandler(),
  onRecoverableError: reactErrorHandler(),
});

// App.tsx imports the yap-frontend-rs WASM module at the top level, which
// runs the wasm-loading code as soon as that module is evaluated — before
// any in-app "browser not supported" screen gets a chance to render. On
// browsers without a WebAssembly global, that throws an uncaught
// ReferenceError and the page is left blank. Check for WebAssembly here,
// ahead of the import, so those browsers get the fallback screen instead.
if (typeof WebAssembly === "undefined") {
  import("./components/browser-not-supported").then(
    ({ BrowserNotSupported }) => {
      hideLoadingScreen();
      root.render(
        <StrictMode>
          <BrowserNotSupported />
        </StrictMode>,
      );
    },
  );
} else {
  import("./App.tsx").then(({ default: App }) => {
    hideLoadingScreen();
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  });
}
