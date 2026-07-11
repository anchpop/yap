import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import { app, connectOnce } from "./bridge";
import { prefetchAudio } from "./audio";
import type { Challenge } from "./types";
import { ReviewCard } from "./ReviewCard";
import { TranslationCard } from "./TranslationCard";

type Phase =
  | { kind: "loading" }
  | { kind: "challenge"; challenge: Challenge }
  | { kind: "error"; message: string };

function applyHostTheme(theme: string | undefined) {
  document.documentElement.classList.toggle("dark", theme === "dark");
}

export function App() {
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });

  useEffect(() => {
    app.ontoolresult = (params) => {
      const challenge = (params.structuredContent as { challenge?: Challenge } | undefined)
        ?.challenge;
      if (!challenge) {
        setPhase({ kind: "error", message: "card data missing — ask for the card again" });
        return;
      }
      prefetchAudio(challenge.audio);
      setPhase({ kind: "challenge", challenge });
    };
    app.onhostcontextchanged = (params) => {
      const theme = (params as { hostContext?: { theme?: string } }).hostContext?.theme;
      if (theme) applyHostTheme(theme);
    };
    connectOnce()
      .then(() => applyHostTheme(app.getHostContext()?.theme))
      .catch((e: unknown) => {
        setPhase({ kind: "error", message: `could not reach the host: ${String(e)}` });
      });
  }, []);

  return (
    <div className="p-2">
      {phase.kind === "loading" && (
        <p className="text-sm text-muted-foreground text-center py-6">loading card…</p>
      )}
      {phase.kind === "error" && (
        <p className="text-sm text-muted-foreground text-center py-6">{phase.message}</p>
      )}
      {phase.kind === "challenge" &&
        (phase.challenge.type === "translation" ? (
          <TranslationCard key={phase.challenge.nonce} challenge={phase.challenge} />
        ) : (
          <ReviewCard key={phase.challenge.nonce} challenge={phase.challenge} />
        ))}
      <Toaster />
    </div>
  );
}
