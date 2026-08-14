import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

/**
 * Whether the current session's email is confirmed. `undefined` means
 * "not known yet" — render nothing rather than flashing.
 *
 * Live, not one-shot: subscribes to onAuthStateChange, and GoTrue
 * broadcasts auth events across tabs, so confirming the email in another
 * tab updates this one without a reload.
 */
export function useEmailConfirmed(): boolean | undefined {
  const [confirmed, setConfirmed] = useState<boolean | undefined>(undefined);

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setConfirmed(Boolean(session?.user.email_confirmed_at));
    });
    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((_event, session) => {
      setConfirmed(Boolean(session?.user.email_confirmed_at));
    });
    return () => subscription.unsubscribe();
  }, []);

  return confirmed;
}
