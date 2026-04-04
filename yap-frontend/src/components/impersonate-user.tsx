import { useState, useRef } from "react";
import { createClient } from "@supabase/supabase-js";
import { supabase } from "@/lib/supabase";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { UserRoundCog, X, Upload } from "lucide-react";
import { useSyncActions } from "@/weapon";

const SUPABASE_URL = "https://eearwzqotpfoderpfrqx.supabase.co";
const ORIGINAL_SESSION_KEY = "yap-impersonation-original-session";
const SERVICE_ROLE_KEY_STORAGE = "yap-impersonation-service-role-key";

/**
 * Hook to gate the impersonation UI behind a secret gesture:
 * rapidly click the User ID row 5 times within 2 seconds.
 */
export function useImpersonationActivation() {
  const [activated, setActivated] = useState(
    () => !!localStorage.getItem(ORIGINAL_SESSION_KEY),
  );
  const clickCountRef = useRef(0);
  const lastClickRef = useRef(0);

  const handleActivationClick = () => {
    const now = Date.now();
    // Reset counter if more than 2s since last click
    if (now - lastClickRef.current > 2000) {
      clickCountRef.current = 0;
    }
    lastClickRef.current = now;
    clickCountRef.current += 1;

    if (clickCountRef.current >= 5) {
      setActivated(true);
      clickCountRef.current = 0;
    }
  };

  return { activated, handleActivationClick };
}

export function ImpersonateUser() {
  const { forcePush } = useSyncActions();
  const [serviceRoleKey, setServiceRoleKey] = useState(
    () => sessionStorage.getItem(SERVICE_ROLE_KEY_STORAGE) ?? "",
  );
  const [targetEmail, setTargetEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [isImpersonating, setIsImpersonating] = useState(
    () => !!localStorage.getItem(ORIGINAL_SESSION_KEY),
  );

  const handleForcePush = async () => {
    setPushing(true);
    try {
      await forcePush();
    } catch (e: unknown) {
      console.warn("Force push failed", e);
    } finally {
      setPushing(false);
    }
  };

  const handleImpersonate = async () => {
    setError(null);
    setLoading(true);

    try {
      // Must be logged in to have a session to save
      const {
        data: { session: currentSession },
      } = await supabase.auth.getSession();
      if (!currentSession) {
        throw new Error("You must be logged in to impersonate");
      }

      // Persist original session so we can come back
      localStorage.setItem(
        ORIGINAL_SESSION_KEY,
        JSON.stringify({
          access_token: currentSession.access_token,
          refresh_token: currentSession.refresh_token,
        }),
      );

      // Remember service role key for the tab lifetime
      sessionStorage.setItem(SERVICE_ROLE_KEY_STORAGE, serviceRoleKey);

      // Build a throwaway admin client with the service role key
      const adminClient = createClient(SUPABASE_URL, serviceRoleKey, {
        auth: { autoRefreshToken: false, persistSession: false },
      });

      // Generate a magic-link for the target user (server-side only, never sent)
      const { data: linkData, error: linkError } =
        await adminClient.auth.admin.generateLink({
          type: "magiclink",
          email: targetEmail,
        });

      if (linkError) throw linkError;
      if (!linkData?.properties?.hashed_token) {
        throw new Error("Failed to generate impersonation token");
      }

      // Exchange the hashed token for a real session on our normal client
      const { error: otpError } = await supabase.auth.verifyOtp({
        token_hash: linkData.properties.hashed_token,
        type: "magiclink",
      });

      if (otpError) throw otpError;

      setIsImpersonating(true);
      setTargetEmail("");
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : "Impersonation failed";
      setError(message);
      // Roll back saved session on failure
      localStorage.removeItem(ORIGINAL_SESSION_KEY);
    } finally {
      setLoading(false);
    }
  };

  const handleStopImpersonating = async () => {
    setError(null);
    setLoading(true);

    try {
      const raw = localStorage.getItem(ORIGINAL_SESSION_KEY);
      if (!raw) throw new Error("No original session found");

      const { access_token, refresh_token } = JSON.parse(raw) as {
        access_token: string;
        refresh_token: string;
      };

      // Drop the impersonated session
      await supabase.auth.signOut();

      // Restore the real admin's session
      const { error: restoreError } = await supabase.auth.setSession({
        access_token,
        refresh_token,
      });
      if (restoreError) throw restoreError;

      localStorage.removeItem(ORIGINAL_SESSION_KEY);
      sessionStorage.removeItem(SERVICE_ROLE_KEY_STORAGE);
      setIsImpersonating(false);
    } catch (e: unknown) {
      const message =
        e instanceof Error ? e.message : "Failed to restore session";
      setError(message);
    } finally {
      setLoading(false);
    }
  };

  // ── Currently impersonating ──────────────────────────────
  if (isImpersonating) {
    return (
      <div className="p-3 bg-orange-50 dark:bg-orange-950/30 border border-orange-300 dark:border-orange-800 rounded-lg space-y-2">
        <div className="flex items-center gap-2 text-orange-700 dark:text-orange-400">
          <UserRoundCog className="w-4 h-4" />
          <span className="text-sm font-medium">
            Currently impersonating a user
          </span>
        </div>
        <p className="text-xs text-orange-600 dark:text-orange-400/80">
          Sync uploads are paused. Changes will not be pushed to the server
          unless you use the button below.
        </p>
        {error && (
          <p className="text-xs text-red-600 dark:text-red-400">{error}</p>
        )}
        <Button
          size="sm"
          variant="outline"
          onClick={handleForcePush}
          disabled={pushing}
          className="w-full border-orange-300 dark:border-orange-700 text-orange-700 dark:text-orange-400"
        >
          <Upload className="w-3 h-3 mr-1" />
          {pushing ? "Pushing..." : "Force Push to Server"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={handleStopImpersonating}
          disabled={loading}
          className="w-full border-orange-300 dark:border-orange-700 text-orange-700 dark:text-orange-400"
        >
          <X className="w-3 h-3 mr-1" />
          {loading ? "Restoring..." : "Stop Impersonating"}
        </Button>
      </div>
    );
  }

  // ── Impersonation form ───────────────────────────────────
  return (
    <div className="p-3 bg-muted/50 border border-dashed border-muted-foreground/30 rounded-lg space-y-3">
      <div className="flex items-center gap-2">
        <UserRoundCog className="w-4 h-4 text-muted-foreground" />
        <span className="text-sm font-medium text-muted-foreground">
          Impersonate User
        </span>
      </div>
      <p className="text-xs text-muted-foreground">
        Requires the Supabase <span className="font-mono">service_role</span>{" "}
        key. The key is kept in session storage (cleared when the tab closes).
      </p>
      <div className="space-y-2">
        <Input
          type="password"
          placeholder="Supabase service role key"
          value={serviceRoleKey}
          onChange={(e) => setServiceRoleKey(e.target.value)}
          className="text-xs font-mono"
        />
        <Input
          type="email"
          placeholder="Target user email"
          value={targetEmail}
          onChange={(e) => setTargetEmail(e.target.value)}
          className="text-xs"
        />
      </div>
      {error && (
        <p className="text-xs text-red-600 dark:text-red-400">{error}</p>
      )}
      <Button
        size="sm"
        onClick={handleImpersonate}
        disabled={loading || !serviceRoleKey || !targetEmail}
        className="w-full"
      >
        {loading ? "Impersonating..." : "Impersonate"}
      </Button>
    </div>
  );
}
