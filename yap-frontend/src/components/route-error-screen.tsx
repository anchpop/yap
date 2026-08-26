import { useEffect } from "react";
import * as Sentry from "@sentry/react";
import { useRouteError } from "react-router-dom";
import { Button } from "@/components/ui/button";

export function RouteErrorScreen() {
  const error = useRouteError();

  // With an errorElement in place, React Router catches render errors before
  // they reach React's onUncaughtError → Sentry handler, so report explicitly.
  useEffect(() => {
    Sentry.captureException(error);
  }, [error]);

  const message = error instanceof Error ? error.message : String(error);

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] p-4 gap-4 text-center">
      <h1 className="text-3xl font-bold">Something went wrong</h1>
      <p className="text-muted-foreground max-w-md break-words">{message}</p>
      <Button onClick={() => window.location.reload()}>Reload</Button>
      {/* Plain anchor: full-page navigation recovers even if router state is broken */}
      <a
        href="/"
        className="text-primary underline underline-offset-4 hover:text-primary/80"
      >
        Go home
      </a>
    </div>
  );
}
