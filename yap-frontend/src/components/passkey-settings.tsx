import { useCallback, useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";
import type { PasskeyListItem } from "@supabase/supabase-js";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { KeyRound, Trash2 } from "lucide-react";

export function passkeysSupported(): boolean {
  return typeof window !== "undefined" && "PublicKeyCredential" in window;
}

export function PasskeySettings() {
  const [open, setOpen] = useState(false);
  // Supabase only allows passkey registration for confirmed, non-anonymous
  // users, so hide the dialog entirely until the email is confirmed.
  const [emailConfirmed, setEmailConfirmed] = useState(false);
  const [passkeys, setPasskeys] = useState<PasskeyListItem[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const { data, error } = await supabase.auth.passkey.list();
    if (error) {
      setError(error.message);
      setPasskeys([]);
    } else {
      setPasskeys(data);
    }
  }, []);

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen);
    if (nextOpen) {
      setError(null);
      setPasskeys(null);
      refresh();
    }
  };

  const handleRegister = async () => {
    setError(null);
    setBusy(true);
    const { error } = await supabase.auth.registerPasskey();
    if (error) {
      setError(error.message);
    } else {
      await refresh();
    }
    setBusy(false);
  };

  const handleDelete = async (passkeyId: string) => {
    setError(null);
    setBusy(true);
    const { error } = await supabase.auth.passkey.delete({ passkeyId });
    if (error) {
      setError(error.message);
    } else {
      await refresh();
    }
    setBusy(false);
  };

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setEmailConfirmed(Boolean(session?.user.email_confirmed_at));
    });
  }, []);

  if (!passkeysSupported() || !emailConfirmed) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <DropdownMenuItem onSelect={(e) => e.preventDefault()}>
          <KeyRound className="mr-2 h-4 w-4" />
          Passkeys
        </DropdownMenuItem>
      </DialogTrigger>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Passkeys</DialogTitle>
          <DialogDescription>
            Sign in with your fingerprint, face, or security key instead of a
            password.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-4">
          {passkeys === null ? (
            <p className="text-muted-foreground text-center">Loading...</p>
          ) : passkeys.length === 0 ? (
            <p className="text-muted-foreground text-center text-sm">
              No passkeys registered yet.
            </p>
          ) : (
            <div className="space-y-2">
              {passkeys.map((pk) => (
                <div
                  key={pk.id}
                  className="flex items-center gap-3 rounded-md border p-3"
                >
                  <KeyRound className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate">
                      {pk.friendly_name || "Passkey"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Added {new Date(pk.created_at).toLocaleDateString()}
                      {pk.last_used_at &&
                        ` · Last used ${new Date(
                          pk.last_used_at,
                        ).toLocaleDateString()}`}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => handleDelete(pk.id)}
                    disabled={busy}
                    title="Remove passkey"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button onClick={handleRegister} disabled={busy} className="w-full">
            {busy ? "Working..." : "Add a Passkey"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
