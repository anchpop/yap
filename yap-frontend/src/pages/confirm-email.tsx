import { useState, useEffect } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
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
import { KeyRound } from "lucide-react";
import {
  canCreatePasskey,
  isCancelled,
  registerPasskeyCeremony,
} from "@/lib/passkey";

export function ConfirmEmail() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  // Confirming the email is the first moment passkey registration is
  // allowed, and for a brand-new user it's the only prompt they'll see —
  // the post-password-sign-in offer requires a confirmed email.
  const [offerPasskey, setOfferPasskey] = useState(false);
  const [passkeyBusy, setPasskeyBusy] = useState(false);
  const [passkeyError, setPasskeyError] = useState<string | null>(null);

  const token_hash = searchParams.get("token_hash");
  const type = searchParams.get("type");

  useEffect(() => {
    const confirmEmail = async () => {
      if (token_hash && type) {
        const { error } = await supabase.auth.verifyOtp({
          token_hash,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          type: type as any,
        });

        if (error) {
          setError(error.message);
        } else {
          setSuccess(true);
          // verifyOtp established a session, so registration is legal
          // here. Only offer when this device can actually hold one.
          const { data } = await supabase.auth.getSession();
          if (data.session && (await canCreatePasskey())) {
            setOfferPasskey(true);
          }
        }
      } else {
        setError("Invalid confirmation link");
      }
      setLoading(false);
    };

    confirmEmail();
  }, [token_hash, type]);

  // Auto-redirect only when we're not asking the user anything.
  useEffect(() => {
    if (!success || offerPasskey) return;
    const timeout = setTimeout(() => navigate("/"), 3000);
    return () => clearTimeout(timeout);
  }, [success, offerPasskey, navigate]);

  const handleReturnHome = () => {
    navigate("/");
  };

  const handleCreatePasskey = async () => {
    setPasskeyError(null);
    setPasskeyBusy(true);
    try {
      await registerPasskeyCeremony();
      navigate("/");
    } catch (err) {
      if (isCancelled(err)) {
        navigate("/");
      } else {
        setPasskeyError("Couldn't set up a passkey on this device.");
      }
    }
    setPasskeyBusy(false);
  };

  if (loading) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold">
                Confirming Email...
              </CardTitle>
              <CardDescription>
                Please wait while we confirm your email address
              </CardDescription>
            </CardHeader>
            <CardContent className="text-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto"></div>
            </CardContent>
          </Card>
        </div>
      </ThemeProvider>
    );
  }

  if (success) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold text-green-500">
                Email Confirmed!
              </CardTitle>
              <CardDescription>
                {offerPasskey
                  ? "You're signed in. One more thing — want to skip the password from now on?"
                  : "Your email has been confirmed and you're signed in."}
              </CardDescription>
            </CardHeader>
            <CardContent className="text-center">
              {offerPasskey ? (
                <div className="space-y-3">
                  <p className="text-sm text-muted-foreground">
                    Add a passkey to sign in with your fingerprint, face, or
                    device PIN.
                  </p>
                  {passkeyError && (
                    <p className="text-sm text-destructive">{passkeyError}</p>
                  )}
                  <Button
                    className="w-full"
                    disabled={passkeyBusy}
                    onClick={handleCreatePasskey}
                  >
                    <KeyRound className="mr-2 h-4 w-4" />
                    {passkeyBusy ? "Working..." : "Set up a passkey"}
                  </Button>
                  <Button
                    variant="ghost"
                    className="w-full"
                    disabled={passkeyBusy}
                    onClick={handleReturnHome}
                  >
                    Maybe later
                  </Button>
                </div>
              ) : (
                <>
                  <p className="text-sm text-muted-foreground mb-4">
                    Redirecting to home page...
                  </p>
                  <Button onClick={handleReturnHome} variant="outline">
                    Go to Home
                  </Button>
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </ThemeProvider>
    );
  }

  return (
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <div className="min-h-screen bg-background flex items-center justify-center p-4">
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <CardTitle className="text-2xl font-bold text-destructive">
              Confirmation Failed
            </CardTitle>
            <CardDescription>
              There was an issue confirming your email address
            </CardDescription>
          </CardHeader>
          <CardContent className="text-center">
            {error && (
              <div className="mb-4 p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                {error}
              </div>
            )}
            <Button onClick={handleReturnHome} variant="outline">
              Return to Home
            </Button>
          </CardContent>
        </Card>
      </div>
    </ThemeProvider>
  );
}
