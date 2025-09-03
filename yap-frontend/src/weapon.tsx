import { useState, useCallback, useEffect, useRef, createContext, useContext, type PropsWithChildren } from 'react';
import { useNetworkState } from 'react-use'
import { supabase } from '@/lib/supabase'
import { test_opfs, Weapon } from '../../yap-frontend-rs/pkg/yap_frontend_rs';
import type { IUseNetworkState } from 'react-use/lib/useNetworkState';

export type WeaponToken = {
    browserSupported: true
}

type WeaponState =
    | { type: 'loading' }
    | { type: 'error', message: string }
    | { type: 'ready', weapon: Weapon }

// React context for Weapon state
const WeaponContext = createContext<WeaponState | undefined>(undefined)

// simple actions context to trigger syncs
const SyncActionsContext = createContext<undefined | { syncNow: () => Promise<void> }>(undefined)

export function WeaponProvider({ userId, accessToken, children }: PropsWithChildren<{ userId: string | undefined, accessToken: string | undefined }>) {
    const [state, setState] = useState<WeaponState>({ type: 'loading' })
    const stateRef = useRef<WeaponState>(null)
    const accessTokenRef = useRef<string | undefined>(accessToken)
    const networkState = useNetworkState();
    const networkStateRef = useRef<IUseNetworkState>(networkState);
    stateRef.current = state
    accessTokenRef.current = accessToken
    networkStateRef.current = networkState

    const network = useNetworkState()
    const realtimeChannelRef = useRef<any>(null)
    const broadcastChannelRef = useRef<BroadcastChannel | null>(null)

    const sync = useCallback(async function sync(listenerId: any, stream_id: string) {
        if (stateRef.current) {
            if (stateRef.current.type !== 'ready') return
            try {
                await stateRef.current.weapon.sync(
                    stream_id,
                    accessTokenRef.current,
                    networkState.online ? true : false,
                    listenerId ?? undefined,
                )
            } catch (e: any) {
                console.warn('sync failed after store change', e)
            }
        }
    }, [])

    useEffect(() => {
        const abortController = new AbortController()

        async function loadWeapon() {
            setState({ type: 'loading' })

            try {
                const weapon = await new Weapon(userId, sync)
                if (!abortController.signal.aborted) {
                    setState({ type: 'ready', weapon })
                }
            } catch (err: any) {
                if (err.name !== 'AbortError') {
                    setState({ type: 'error', message: err.message })
                }
            }
        }

        loadWeapon()

        return () => abortController.abort()
    }, [userId, sync])

    const syncWithSupabase = useCallback(async () => {
        if (stateRef.current === null) return;
        if (stateRef.current.type !== 'ready') return;
        if (accessTokenRef.current === undefined) return;
        try {
            if (networkState.online) {
                await stateRef.current.weapon.sync_with_supabase(accessTokenRef.current, undefined);
            }
        } catch (e: any) {
            console.warn('sync_with_supabase failed', e)
        }
    }, [])

    useEffect(() => {
        // rerun supabase sync every 30 seconds
        const interval = setInterval(() => {
            void syncWithSupabase()
        }, 30_000) // 30 seconds

        return () => {
            clearInterval(interval);
        }
    }, [syncWithSupabase])

    // Set up BroadcastChannel for inter-tab sync
    useEffect(() => {
        if (!window.BroadcastChannel) {
            console.log('BroadcastChannel not supported');
            return;
        }

        // Clean up existing channel if any
        if (broadcastChannelRef.current) {
            broadcastChannelRef.current.close();
            broadcastChannelRef.current = null;
        }

        const channel = new BroadcastChannel('weapon-opfs-sync');
        broadcastChannelRef.current = channel;

        channel.onmessage = (event) => {
            if (event.data?.type === 'opfs-written' && event.data?.stream_id) {
                const streamId = event.data.stream_id;
                console.log(`Another tab wrote to OPFS for stream ${streamId}, reloading...`);

                // Load from local storage for the affected stream
                const currentState = stateRef.current;
                if (currentState && currentState.type === 'ready') {
                    currentState.weapon.load_from_local_storage(streamId)
                        .then(() => {
                            console.log(`Successfully reloaded stream ${streamId} from OPFS`);
                        })
                        .catch((e: any) => {
                            console.warn(`Failed to reload stream ${streamId} from OPFS:`, e);
                        });
                }
            }
        };

        return () => {
            if (broadcastChannelRef.current) {
                broadcastChannelRef.current.close();
                broadcastChannelRef.current = null;
            }
        };
    }, []);


    // Realtime subscription to remote events via Supabase
    useEffect(() => {
        // tear down existing channel if any
        if (realtimeChannelRef.current) {
            supabase.removeChannel(realtimeChannelRef.current)
            realtimeChannelRef.current = null
        }

        if (!userId) return
        if (!network.online) return

        const channel = supabase
            .channel(`events:${userId}`)
            .on(
                'postgres_changes',
                {
                    event: 'INSERT',
                    schema: 'public',
                    table: 'events',
                    filter: `user_id=eq.${userId}`,
                },
                (payload: any) => {
                    try {
                        const row = payload?.new
                        if (!row) return
                        const device_id: string = row.device_id
                        const stream_id: string = row.stream_id
                        const event_json: string = row.event
                        const stringified_event_json = typeof event_json !== 'string' ? JSON.stringify(event_json) : event_json

                        const current = stateRef.current
                        if (current && current.type === 'ready') {
                            console.log(`Adding remote ${stream_id} event from ${device_id}`, stringified_event_json)
                            current.weapon.add_remote_event(device_id, stream_id, stringified_event_json)
                        }
                    } catch (e) {
                        console.error('Error handling realtime event', e)
                    }
                }
            )
            .subscribe()

        realtimeChannelRef.current = channel

        return () => {
            if (realtimeChannelRef.current) {
                supabase.removeChannel(realtimeChannelRef.current)
                realtimeChannelRef.current = null
            }
        }
    }, [userId, network.online])

    const actions = { syncNow: async () => { await syncWithSupabase() } }

    return (
        <WeaponContext.Provider value={state}>
            <SyncActionsContext.Provider value={actions}>
                {children}
            </SyncActionsContext.Provider>
        </WeaponContext.Provider>
    )
}

// Hook to grab the weapon instance from context (ready-only)
export function useWeapon(): Weapon {
    const ctx = useContext(WeaponContext)
    if (!ctx) throw new Error('useWeapon must be used within a WeaponProvider')
    if (ctx.type !== 'ready') throw new Error('Weapon not ready')
    return ctx.weapon
}

// Optional hook to read full state (loading/error/ready)
export function useWeaponState(): WeaponState {
    const ctx = useContext(WeaponContext)
    if (!ctx) throw new Error('useWeaponState must be used within a WeaponProvider')
    return ctx
}

export function useSyncActions(): { syncNow: () => Promise<void> } {
    const ctx = useContext(SyncActionsContext)
    if (!ctx) throw new Error('useSyncActions must be used within a WeaponProvider')
    return ctx
}

async function checkBrowserSupport(setBrowserSupported: (browserSupported: boolean) => void) {
    try {
        // Check if OPFS test has already passed
        const opfsTestPassed = localStorage.getItem('opfs-test-passed')

        if (opfsTestPassed === 'true') {
            setBrowserSupported(true)
        } else {
            // Create a timeout promise that rejects after 3 seconds
            const timeoutPromise = new Promise<boolean>((resolve,) => {
                setTimeout(() => { console.log('OPFS test timed out after 3 seconds'); resolve(false) }, 3000)
            })

            // Race between the OPFS test and the timeout
            const isSupported = await Promise.race([
                test_opfs(),
                timeoutPromise
            ])

            setBrowserSupported(isSupported)

            if (isSupported) {
                // Store successful test result
                localStorage.setItem('opfs-test-passed', 'true')
            }
        }
    } catch (error) {
        console.error('Browser support check failed:', error)
        // If test_opfs throws an error or times out, the browser is not supported
        setBrowserSupported(false)
    }
}

export function useWeaponSupport(): { browserSupported: true | false | null } {
    const [browserSupported, setBrowserSupported] = useState<boolean | null>(null)
    useEffect(() => {
        checkBrowserSupport(setBrowserSupported)
    }, [setBrowserSupported])

    return { browserSupported }
}

export function useAsyncMemo<T>(factory: () => Promise<T> | undefined | null, deps: React.DependencyList, initial?: T) {
    const [val, setVal] = useState<T | undefined>(initial)
    useEffect(() => {
        let cancel = false
        const promise = factory()
        if (promise === undefined || promise === null) return
        promise.then((val) => {
            if (!cancel) {
                setVal(val)
            }
        })
        return () => {
            cancel = true
        }
    }, deps)
    return val
}
