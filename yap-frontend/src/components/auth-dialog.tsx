import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { supabase } from "@/lib/supabase";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  armConditional,
  browserSupportsWebAuthn,
  canAutofillPasskey,
  canCreatePasskey,
  disarm,
  isCancelled,
  registerPasskeyCeremony,
  signInWithPasskeyModal,
} from "@/lib/passkey";
import { KeyRound } from "lucide-react";
import type { Session } from "@supabase/supabase-js";

interface AuthDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  view: "signin" | "signup";
  onViewChange: (view: "signin" | "signup") => void;
}

const PASSKEY_OFFER_DISMISSED_KEY = "yap-passkey-offer-dismissed";

/** Fixed strings only — raw WebAuthn error messages can name authenticator
 * hardware, and Sentry replay records rendered text verbatim. */
const PASSKEY_SIGNIN_FAILED =
  "Couldn't sign in with a passkey. Try your password instead.";
const PASSKEY_SETUP_FAILED = "Couldn't set up a passkey on this device.";

/**
 * After a password sign-in, decide whether to show the "skip the password
 * next time?" offer. Gates ordered cheapest-first so the common case adds
 * nothing to the sign-in path; passkey.list() is raced against a timer so
 * a slow network can't hold the dialog hostage.
 */
async function shouldOfferPasskey(session: Session): Promise<boolean> {
  if (localStorage.getItem(PASSKEY_OFFER_DISMISSED_KEY) === "true")
    return false;
  if (!session.user.email_confirmed_at) return false;
  if (!(await canCreatePasskey())) return false;
  try {
    const existing = await Promise.race([
      supabase.auth.passkey.list().then(({ data }) => data),
      new Promise<null>((resolve) => setTimeout(() => resolve(null), 800)),
    ]);
    // null = errored or too slow: skip the offer rather than stall.
    return existing !== null && existing.length === 0;
  } catch {
    return false;
  }
}

export function AuthDialog({
  open,
  onOpenChange,
  view,
  onViewChange,
}: AuthDialogProps) {
  const [loading, setLoading] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [passkeyOffer, setPasskeyOffer] = useState(false);
  const signinEmailRef = useRef<HTMLInputElement>(null);

  // The dialog outlives sign-ins now (it's mounted in a provider, not in
  // Header's signed-out branch), so clear everything on close — the
  // password must not sit in React state indefinitely. Every close path,
  // including Escape, funnels through Radix's onOpenChange.
  const handleOpenChange = useCallback(
    (next: boolean) => {
      if (!next) {
        setEmail("");
        setPassword("");
        setError(null);
        setPasskeyOffer(false);
        setLoading(false);
      }
      onOpenChange(next);
    },
    [onOpenChange],
  );

  // Arm the conditional (autofill) passkey request while the sign-in form
  // is showing. The browser surfaces the passkey in the email field's
  // autofill dropdown / QuickType bar; picking it resolves with a session.
  useEffect(() => {
    if (!open || view !== "signin") return;
    let cancelled = false;
    (async () => {
      if (!(await canAutofillPasskey()) || cancelled) return;
      const session = await armConditional();
      if (session && !cancelled) {
        handleOpenChange(false);
      }
    })().catch(() => {
      // Conditional flow is invisible until it succeeds — failures stay
      // silent (the user can always sign in normally).
    });
    return () => {
      cancelled = true;
      disarm();
    };
  }, [open, view, handleOpenChange]);

  const finishSignIn = async (session: Session | null) => {
    if (session && (await shouldOfferPasskey(session))) {
      setPasskeyOffer(true);
    } else {
      handleOpenChange(false);
    }
  };

  const handleSignUp = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    const { data, error } = await supabase.auth.signUp({
      email,
      password,
    });

    if (error) {
      setError(error.message);
    } else if (data.user) {
      const { error: signInError } = await supabase.auth.signInWithPassword({
        email,
        password,
      });

      if (signInError) {
        setError(signInError.message);
      } else {
        // No passkey offer here: the email is unconfirmed, so
        // registration would be rejected. The confirm-email page owns
        // the new-user offer.
        handleOpenChange(false);
      }
    }
    setLoading(false);
  };

  const handlePasskeySignIn = async () => {
    setError(null);
    setLoading(true);
    try {
      const session = await signInWithPasskeyModal();
      if (session) {
        handleOpenChange(false);
      }
    } catch (err) {
      if (!isCancelled(err)) {
        setError(PASSKEY_SIGNIN_FAILED);
      }
    }
    setLoading(false);
  };

  const handleSignIn = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    const { data, error } = await supabase.auth.signInWithPassword({
      email,
      password,
    });

    if (error) {
      setError(error.message);
    } else {
      await finishSignIn(data.session);
    }
    setLoading(false);
  };

  const handleCreatePasskey = async () => {
    setError(null);
    setLoading(true);
    try {
      await registerPasskeyCeremony();
      handleOpenChange(false);
    } catch (err) {
      if (isCancelled(err)) {
        // They changed their mind at the system sheet; treat as "not now"
        // without burning a dismissal.
        handleOpenChange(false);
      } else {
        setError(PASSKEY_SETUP_FAILED);
      }
    }
    setLoading(false);
  };

  const handleDeclinePasskey = () => {
    localStorage.setItem(PASSKEY_OFFER_DISMISSED_KEY, "true");
    handleOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="sm:max-w-md translate-y-[-80%] sm:translate-y-[-50%]"
        onPointerDownOutside={(e) => e.preventDefault()}
        onInteractOutside={(e) => e.preventDefault()}
        onOpenAutoFocus={(e) => {
          // Radix focuses the dialog content itself; the autofill dropdown
          // only appears once the email input has focus, so put it there.
          if (view === "signin" && !passkeyOffer) {
            e.preventDefault();
            signinEmailRef.current?.focus();
          }
        }}
      >
        {passkeyOffer ? (
          <>
            <DialogHeader>
              <DialogTitle>Skip the password next time?</DialogTitle>
              <DialogDescription>
                Set up a passkey to sign in with your fingerprint, face, or
                device PIN instead of typing your password.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4">
              {error && <p className="text-sm text-red-500">{error}</p>}
              <Button
                className="w-full"
                disabled={loading}
                onClick={handleCreatePasskey}
              >
                <KeyRound className="mr-2 h-4 w-4" />
                {loading ? "Working..." : "Set up a passkey"}
              </Button>
              <Button
                variant="ghost"
                className="w-full"
                disabled={loading}
                onClick={handleDeclinePasskey}
              >
                Not now
              </Button>
            </div>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Welcome to Yap.Town</DialogTitle>
              <DialogDescription>
                Sign in or create an account to sync your progress across
                devices
              </DialogDescription>
            </DialogHeader>
            <Tabs
              value={view}
              onValueChange={(v) => onViewChange(v as "signin" | "signup")}
              className="w-full"
            >
              <TabsList className="grid w-full grid-cols-2">
                <TabsTrigger value="signin">Sign In</TabsTrigger>
                <TabsTrigger value="signup">Sign Up</TabsTrigger>
              </TabsList>
              <TabsContent value="signin">
                <form onSubmit={handleSignIn} className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="signin-email">Email</Label>
                    <Input
                      id="signin-email"
                      type="email"
                      ref={signinEmailRef}
                      autoComplete="username webauthn"
                      value={email}
                      onChange={(e) => setEmail(e.target.value)}
                      required
                      disabled={loading}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="signin-password">Password</Label>
                    <Input
                      id="signin-password"
                      type="password"
                      autoComplete="current-password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      required
                      disabled={loading}
                    />
                  </div>
                  {error && <p className="text-sm text-red-500">{error}</p>}
                  <Button type="submit" className="w-full" disabled={loading}>
                    {loading ? "Signing in..." : "Sign In"}
                  </Button>
                  {browserSupportsWebAuthn() && (
                    <>
                      <div className="flex items-center gap-3">
                        <div className="h-px flex-1 bg-border" />
                        <span className="text-xs text-muted-foreground">
                          or
                        </span>
                        <div className="h-px flex-1 bg-border" />
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        className="w-full"
                        disabled={loading}
                        onClick={handlePasskeySignIn}
                      >
                        <KeyRound className="mr-2 h-4 w-4" />
                        Sign in with a passkey
                      </Button>
                    </>
                  )}
                  <div className="text-center">
                    <Link
                      to="/forgot-password"
                      onClick={() => handleOpenChange(false)}
                      className="text-sm text-muted-foreground hover:text-foreground underline"
                    >
                      Forgot your password?
                    </Link>
                  </div>
                </form>
              </TabsContent>
              <TabsContent value="signup">
                <form onSubmit={handleSignUp} className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="signup-email">Email</Label>
                    <Input
                      id="signup-email"
                      type="email"
                      autoComplete="email"
                      value={email}
                      onChange={(e) => setEmail(e.target.value)}
                      required
                      disabled={loading}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="signup-password">Password</Label>
                    <Input
                      id="signup-password"
                      type="password"
                      autoComplete="new-password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      required
                      disabled={loading}
                    />
                  </div>
                  {error && <p className="text-sm text-red-500">{error}</p>}
                  <Button type="submit" className="w-full" disabled={loading}>
                    {loading ? "Creating account..." : "Create Account"}
                  </Button>
                </form>
              </TabsContent>
            </Tabs>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
