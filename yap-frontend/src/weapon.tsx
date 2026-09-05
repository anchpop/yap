import {
  useState,
  useCallback,
  useEffect,
  useRef,
  createContext,
  useContext,
  type PropsWithChildren,
} from "react";
import { useNetworkState } from "react-use";
import { supabase } from "@/lib/supabase";
import {
  test_opfs,
  Weapon,
  type ListenerKey,
} from "../../yap-frontend-rs/pkg/yap_frontend_rs";

export type WeaponToken = {
  browserSupported: true;
};

type WeaponState =
  | { type: "loading" }
  | { type: "error"; message: string }
  | { type: "ready"; weapon: Weapon };

const WeaponContext = createContext<WeaponState | undefined>(undefined);

const ORIGINAL_SESSION_KEY = "yap-impersonation-original-session";

function isImpersonating() {
  return !!localStorage.getItem(ORIGINAL_SESSION_KEY);
}

const SyncActionsContext = createContext<
  undefined | { syncNow: () => Promise<void>; forcePush: () => Promise<void> }
>(undefined);

export function WeaponProvider({
  userId,
  accessToken,
  children,
}: PropsWithChildren<{
  userId: string | undefined;
  accessToken: string | undefined;
}>) {
  const [state, setState] = useState<WeaponState>({ type: "loading" });
  const stateRef = useRef(state);
  const accessTokenRef = useRef<string | undefined>(accessToken);
  const networkState = useNetworkState();
  const networkStateRef = useRef(networkState);
  stateRef.current = state;
  accessTokenRef.current = accessToken;
  networkStateRef.current = networkState;

  const sync = useCallback(
    async (listenerId: ListenerKey | undefined, streamId: string) => {
      const current = stateRef.current;
      if (current.type !== "ready") return;
      try {
        await current.weapon.sync(
          streamId,
          accessTokenRef.current,
          !!networkStateRef.current.online,
          listenerId,
          !isImpersonating(),
        );
      } catch (error) {
        console.warn("sync failed after store change", error);
      }
    },
    [],
  );

  useEffect(() => {
    const abortController = new AbortController();

    async function loadWeapon() {
      setState({ type: "loading" });

      try {
        const weapon = await Weapon.create(userId, sync);
        if (!abortController.signal.aborted) {
          setState({ type: "ready", weapon });
        }
      } catch (err: any) {
        if (err.name !== "AbortError") {
          setState({ type: "error", message: err.message });
        }
      }
    }

    loadWeapon();

    return () => abortController.abort();
  }, [userId, sync]);

  // Dev-only: expose the ready Weapon instance so E2E/screenshot tooling
  // (Playwright) can seed a deck deterministically without clicking through
  // onboarding. Never compiled into production builds.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    if (state.type !== "ready") return;
    (window as unknown as { __weapon?: Weapon }).__weapon = state.weapon;
    return () => {
      delete (window as unknown as { __weapon?: Weapon }).__weapon;
    };
  }, [state]);

  const syncWithSupabase = useCallback(
    async (forceUpload?: boolean) => {
      if (stateRef.current.type !== "ready") return;
      if (accessTokenRef.current === undefined) return;
      try {
        if (networkStateRef.current.online) {
          const upload = forceUpload ?? !isImpersonating();
          await stateRef.current.weapon.sync_with_supabase(
            accessTokenRef.current,
            undefined,
            upload,
          );
        }
      } catch (e) {
        console.warn("sync_with_supabase failed", e);
      }
    },
    [],
  );

  useEffect(() => {
    const interval = setInterval(() => {
      void syncWithSupabase();
    }, 30_000);

    return () => {
      clearInterval(interval);
    };
  }, [syncWithSupabase]);

  // Initial stream sync may run before the auth token is available.
  useEffect(() => {
    if (!accessToken) return;
    if (state.type !== "ready") return;
    void syncWithSupabase();
  }, [accessToken, state, syncWithSupabase]);

  useEffect(() => {
    if (!window.BroadcastChannel) {
      console.log("BroadcastChannel not supported");
      return;
    }

    const channel = new BroadcastChannel("weapon-opfs-sync");

    channel.onmessage = (event) => {
      if (event.data?.type === "opfs-written" && event.data?.stream_id) {
        const streamId = event.data.stream_id;
        console.log(
          `Another tab wrote to OPFS for stream ${streamId}, reloading...`,
        );

        const currentState = stateRef.current;
        if (currentState.type === "ready") {
          currentState.weapon
            .load_from_local_storage(streamId)
            .then(() => {
              console.log(`Successfully reloaded stream ${streamId} from OPFS`);
            })
            .catch((e) => {
              console.warn(`Failed to reload stream ${streamId} from OPFS:`, e);
            });
        }
      }
    };

    return () => channel.close();
  }, []);

  useEffect(() => {
    if (!userId) return;
    if (!networkState.online) return;
    if (isImpersonating()) return;

    const channel = supabase
      .channel(`events:${userId}`)
      .on(
        "postgres_changes",
        {
          event: "INSERT",
          schema: "public",
          table: "events",
          filter: `user_id=eq.${userId}`,
        },
        (payload) => {
          try {
            const row = payload?.new;
            if (!row) return;
            const device_id: string = row.device_id;
            const stream_id: string = row.stream_id;
            const event_json: string = row.event;
            const stringified_event_json =
              typeof event_json !== "string"
                ? JSON.stringify(event_json)
                : event_json;

            const current = stateRef.current;
            if (
              current.type === "ready" &&
              device_id !== current.weapon.device_id
            ) {
              console.log(
                `Adding remote ${stream_id} event from ${device_id}`,
                stringified_event_json,
              );
              current.weapon.add_remote_event(
                device_id,
                stream_id,
                stringified_event_json,
              );
            }
          } catch (e) {
            console.error("Error handling realtime event", e);
          }
        },
      )
      .subscribe();

    return () => {
      void supabase.removeChannel(channel);
    };
  }, [userId, networkState.online]);

  const actions = {
    syncNow: () => syncWithSupabase(),
    forcePush: () => syncWithSupabase(true),
  };

  return (
    <WeaponContext.Provider value={state}>
      <SyncActionsContext.Provider value={actions}>
        {children}
      </SyncActionsContext.Provider>
    </WeaponContext.Provider>
  );
}

export function useWeapon(): Weapon {
  const ctx = useContext(WeaponContext);
  if (!ctx) throw new Error("useWeapon must be used within a WeaponProvider");
  if (ctx.type !== "ready") throw new Error("Weapon not ready");
  return ctx.weapon;
}

export function useWeaponState(): WeaponState {
  const ctx = useContext(WeaponContext);
  if (!ctx)
    throw new Error("useWeaponState must be used within a WeaponProvider");
  return ctx;
}

export function useSyncActions(): {
  syncNow: () => Promise<void>;
  forcePush: () => Promise<void>;
} {
  const ctx = useContext(SyncActionsContext);
  if (!ctx)
    throw new Error("useSyncActions must be used within a WeaponProvider");
  return ctx;
}

// Minimal WASM module using externref (reference types). Returns false on
// browsers that predate Chrome 86 / Firefox 79 where externref was behind a
// flag — those browsers can't compile our WASM module at all.
function supportsWasmExternref(): boolean {
  try {
    return WebAssembly.validate(
      new Uint8Array([
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x6f, // type section: () -> externref
        0x03, 0x02, 0x01, 0x00, // function section
        0x0a, 0x06, 0x01, 0x04, 0x00, 0xd0, 0x6f, 0x0b, // code section
      ]),
    );
  } catch {
    return false;
  }
}

async function checkBrowserSupport(
  setBrowserSupported: (browserSupported: boolean) => void,
) {
  try {
    const opfsTestPassed = localStorage.getItem("opfs-test-passed");

    // Reject browsers that don't support WebAssembly reference types
    // (externref). Our WASM module uses externref, so it can't compile on
    // Chrome < 86 / Firefox < 79 even if OPFS were somehow present.
    if (!supportsWasmExternref()) {
      setBrowserSupported(false);
      return;
    }

    // Quick JS-level check before calling into WASM (avoids noisy TypeErrors
    // on browsers that lack OPFS entirely or don't support createWritable)
    if (
      !navigator.storage ||
      typeof navigator.storage.getDirectory !== "function" ||
      typeof FileSystemFileHandle?.prototype?.createWritable !== "function"
    ) {
      setBrowserSupported(false);
      return;
    }

    if (opfsTestPassed === "true") {
      setBrowserSupported(true);
    } else {
      let timeoutId: number | undefined;
      const timeoutPromise = new Promise<boolean>((resolve) => {
        timeoutId = window.setTimeout(() => {
          console.log("OPFS test timed out after 3 seconds");
          resolve(false);
        }, 3000);
      });

      // Race between the OPFS test and the timeout
      const isSupported = await Promise.race([test_opfs(), timeoutPromise]);
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }

      setBrowserSupported(isSupported);

      if (isSupported) {
        // Store successful test result
        localStorage.setItem("opfs-test-passed", "true");
      }
    }
  } catch (error) {
    console.error("Browser support check failed:", error);
    // If test_opfs throws an error or times out, the browser is not supported
    setBrowserSupported(false);
  }
}

export function useWeaponSupport(): { browserSupported: true | false | null } {
  const [browserSupported, setBrowserSupported] = useState<boolean | null>(
    null,
  );
  useEffect(() => {
    checkBrowserSupport(setBrowserSupported);
  }, [setBrowserSupported]);

  return { browserSupported };
}

export function useAsyncMemo<T>(
  factory: () => Promise<T> | undefined | null,
  deps: React.DependencyList,
  initial?: T,
) {
  const [val, setVal] = useState<T | undefined>(initial);
  useEffect(() => {
    let cancel = false;
    const promise = factory();
    if (promise === undefined || promise === null) return;
    promise.then((val) => {
      if (!cancel) {
        setVal(val);
      }
    }).catch((error) => {
      if (!cancel) {
        console.error("useAsyncMemo rejected:", error);
      }
    });
    return () => {
      cancel = true;
    };
  }, deps);
  return val;
}
