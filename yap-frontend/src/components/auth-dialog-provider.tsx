import { createContext, useCallback, useContext, useMemo, useState } from "react";
import { AuthDialog } from "@/components/auth-dialog";

interface AuthDialogContextType {
  openSignIn: () => void;
  openSignUp: () => void;
}

const AuthDialogContext = createContext<AuthDialogContextType | null>(null);

/**
 * Owns the single app-wide AuthDialog. Mounted around the main app's
 * Outlet (not AppShell — the standalone routes like /connect and
 * /privacy don't want it, and /connect keeps its own dialog because it
 * renders inside its own nested ThemeProvider).
 *
 * The dialog living here, outside Header's `!userInfo` branch, is what
 * lets it survive a successful sign-in — required for the in-dialog
 * "set up a passkey?" offer.
 */
export function AuthDialogProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<"signin" | "signup">("signin");

  const openSignIn = useCallback(() => {
    setView("signin");
    setOpen(true);
  }, []);
  const openSignUp = useCallback(() => {
    setView("signup");
    setOpen(true);
  }, []);
  const value = useMemo(
    () => ({ openSignIn, openSignUp }),
    [openSignIn, openSignUp],
  );

  return (
    <AuthDialogContext.Provider value={value}>
      {children}
      <AuthDialog
        open={open}
        onOpenChange={setOpen}
        view={view}
        onViewChange={setView}
      />
    </AuthDialogContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuthDialog(): AuthDialogContextType {
  const ctx = useContext(AuthDialogContext);
  if (!ctx) {
    throw new Error("useAuthDialog must be used within AuthDialogProvider");
  }
  return ctx;
}
