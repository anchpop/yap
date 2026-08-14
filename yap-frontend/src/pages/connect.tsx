import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import type { Session } from "@supabase/supabase-js";
import { supabase } from "@/lib/supabase";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ThemeProvider } from "@/components/theme-provider";
import { AuthDialog } from "@/components/auth-dialog";

// The only MCP servers we will ever hand the user's session proof to. A
// malicious link can't redirect approval anywhere else.
const ALLOWED_MCP_ORIGINS = [
  "https://mcp.yap.town",
  "https://yap-mcp.fly.dev",
  ...(import.meta.env.DEV ? ["http://localhost:8080", "http://localhost:8199"] : []),
];

/**
 * OAuth approval page for MCP connectors (e.g. adding yap to claude.ai).
 * The MCP server parks the client's authorize request and redirects here;
 * the user signs in with their normal yap auth and approves, and we post
 * their proof back to the server's /oauth/approve endpoint.
 */
export function Connect() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [session, setSession] = useState<Session | null>(null);
  const [sessionLoaded, setSessionLoaded] = useState(false);
  const [authOpen, setAuthOpen] = useState(false);
  const [authView, setAuthView] = useState<"signin" | "signup">("signin");
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const requestId = searchParams.get("request");
  const mcpOrigin = searchParams.get("mcp") ?? ALLOWED_MCP_ORIGINS[0];
  const mcpOriginAllowed = ALLOWED_MCP_ORIGINS.includes(mcpOrigin);

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setSession(session);
      setSessionLoaded(true);
    });
    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((_event, session) => {
      setSession(session);
    });
    return () => subscription.unsubscribe();
  }, []);

  const handleConnect = async () => {
    if (!session || !requestId) return;
    setConnecting(true);
    setError(null);
    try {
      const response = await fetch(`${mcpOrigin}/oauth/approve`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          request_id: requestId,
          access_token: session.access_token,
        }),
      });
      const body = await response.json();
      if (!response.ok) {
        setError(
          body.error_description ?? "Something went wrong — try connecting again.",
        );
        setConnecting(false);
        return;
      }
      window.location.href = body.redirect;
    } catch {
      setError("Could not reach the connector server — try again.");
      setConnecting(false);
    }
  };

  const invalidReason = !requestId
    ? "This connect link is missing its request. Start over from the app you're connecting."
    : !mcpOriginAllowed
      ? "This connect link points at a server yap.town doesn't recognize."
      : null;

  return (
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <div className="min-h-screen bg-background flex items-center justify-center p-4">
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <CardTitle className="text-2xl font-bold">Connect to yap.town</CardTitle>
            <CardDescription>
              An app wants to use your yap.town account through its connector.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {invalidReason ? (
              <>
                <div className="p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                  {invalidReason}
                </div>
                <Button onClick={() => navigate("/")} variant="outline" className="w-full">
                  Go Home
                </Button>
              </>
            ) : !sessionLoaded ? (
              <p className="text-sm text-muted-foreground text-center">Loading…</p>
            ) : !session ? (
              <>
                <p className="text-sm text-muted-foreground">
                  Sign in to your yap.town account to continue.
                </p>
                <Button onClick={() => setAuthOpen(true)} className="w-full">
                  Sign In
                </Button>
              </>
            ) : (
              <>
                <div className="p-3 bg-muted rounded-md text-sm text-center">
                  <p className="font-medium">{session.user.email}</p>
                </div>
                <p className="text-sm text-muted-foreground">
                  This lets it see your deck and stats, quiz you on due cards, log
                  your reviews, and add new words — everything syncs with the app.
                </p>
                {error && (
                  <div className="p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                    {error}
                  </div>
                )}
                <div className="flex gap-2">
                  <Button
                    onClick={() => navigate("/")}
                    variant="outline"
                    className="flex-1"
                    disabled={connecting}
                  >
                    Cancel
                  </Button>
                  <Button onClick={handleConnect} className="flex-1" disabled={connecting}>
                    {connecting ? "Connecting…" : "Connect"}
                  </Button>
                </div>
              </>
            )}
          </CardContent>
        </Card>
        <AuthDialog
          open={authOpen}
          onOpenChange={setAuthOpen}
          view={authView}
          onViewChange={setAuthView}
        />
      </div>
    </ThemeProvider>
  );
}
