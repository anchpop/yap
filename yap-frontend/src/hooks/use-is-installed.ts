import { useState, useEffect } from "react";

export function useIsInstalled() {
  const [isInstalled, setIsInstalled] = useState(false);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const checkInstalled = () => {
      const isStandalone =
        window.matchMedia("(display-mode: standalone)").matches ||
        ("standalone" in window.navigator &&
          window.navigator.standalone === true) ||
        document.referrer.includes("android-app://") ||
        window.matchMedia("(display-mode: fullscreen)").matches ||
        window.matchMedia("(display-mode: minimal-ui)").matches;

      setIsInstalled(isStandalone);
      setIsLoading(false);
    };

    checkInstalled();

    const mediaQuery = window.matchMedia("(display-mode: standalone)");

    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener("change", checkInstalled);
    } else {
      mediaQuery.addListener(checkInstalled);
    }

    return () => {
      if (mediaQuery.removeEventListener) {
        mediaQuery.removeEventListener("change", checkInstalled);
      } else {
        mediaQuery.removeListener(checkInstalled);
      }
    };
  }, []);

  return { isInstalled, isLoading };
}
