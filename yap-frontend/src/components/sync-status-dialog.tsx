import { useEffect, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { RefreshCw, Check, X, Database, Cloud, Baby, CupSoda, Copy } from 'lucide-react'
import { get_app_version, type EarliestUnsyncedEvent, type SyncState } from '../../../yap-frontend-rs/pkg'
import { useWeapon, useSyncActions } from '@/weapon'
import { useNetworkState } from 'react-use'

export function SyncStatusDialog() {
  const weapon = useWeapon()
  const { syncNow } = useSyncActions()
  const { online: isOnline } = useNetworkState()

  // Poll sync state and earliest unsynced timestamp every second
  const [lastSyncFinishedAt, setLastSyncFinishedAt] = useState<number | null>(null)
  const [lastSyncError, setLastSyncError] = useState<string | null>(null)
  const [earliestUnsyncedAt, setEarliestUnsyncedAt] = useState<number | null>(null)
  const [syncInProgress, setSyncInProgress] = useState<boolean>(false)

  useEffect(() => {
    const update = () => {
      try {
        const s: SyncState<string, string> = weapon.get_sync_state("supabase")
        const started = s.lastSyncStarted
        const finished = s.lastSyncFinished
        setLastSyncFinishedAt(finished)
        setLastSyncError(s.lastSyncError ?? null)
        setSyncInProgress(!!started && (!finished || started > finished))

        const earliest: EarliestUnsyncedEvent | undefined = weapon.get_timestamp_of_earliest_unsynced_event("supabase")
        if (earliest && earliest.timestamp) {
          setEarliestUnsyncedAt(earliest.timestamp.getTime())
        } else {
          setEarliestUnsyncedAt(null)
        }
      } catch (e) {
        // ignore polling errors
      }
    }
    update()
    const id = setInterval(update, 1000)
    return () => clearInterval(id)
  }, [weapon])

  const handleManualSync = async () => {
    setSyncInProgress(true)
    await syncNow()
  }

  // Get local and remote event counts
  const localEventCount = weapon.num_events
  const remoteEventCount = weapon.num_events_on_remote_as_of_last_sync("supabase")


  // Determine sync status
  let statusIcon
  let statusText
  let statusColor

  const now = Date.now()
  const unsyncedStale = earliestUnsyncedAt != null && now - earliestUnsyncedAt > 5000

  if (!isOnline) {
    statusIcon = <Cloud className="w-2 h-2" />
    statusText = "Offline"
    statusColor = "text-gray-500"
  } else if (lastSyncError) {
    statusIcon = <X className="w-2 h-2" />
    statusText = "Sync error"
    statusColor = "text-red-500"
  } else if (isOnline) {
    if (unsyncedStale) {
      statusIcon = <RefreshCw className="w-2 h-2" />
      statusText = "Unsynced"
      statusColor = "text-yellow-500"
    } else {
      statusIcon = <Check className="w-2 h-2" />
      statusText = "Synced"
      statusColor = "text-green-500"
    }
  }

  return (
    <Dialog>
      <DialogTrigger asChild>
        <button className="flex items-center gap-1.5 hover:opacity-80 transition-opacity">
          <span className={`w-2 h-2 rounded-full ${(!isOnline) ? 'bg-gray-500' : lastSyncError ? 'bg-red-500' : unsyncedStale ? 'bg-yellow-500' : 'bg-green-500'}`}></span>
          <span className={`hidden sm:inline text-sm ${statusColor}`}>{statusText}</span>
        </button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Sync Status</DialogTitle>
          <DialogDescription>
            Yap.town keeps your data synchronized across your devices.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="flex items-center justify-between p-3 bg-muted/50 rounded-lg">
            <div className="flex items-center gap-2">
              {statusIcon}
              <span className={`font-medium ${statusColor}`}>{statusText}</span>
            </div>
            {lastSyncFinishedAt && (
              <span className="text-sm text-muted-foreground">
                Last sync: {new Date(lastSyncFinishedAt).toLocaleTimeString()}
              </span>
            )}
          </div>

          {lastSyncError && (
            <div className="p-3 bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg">
              <div className="flex items-start justify-between gap-2">
                <p className="text-sm text-red-600 dark:text-red-400 flex-1">
                  Error: {lastSyncError.length > 200
                    ? `${lastSyncError.substring(0, 200)}... (${lastSyncError.length} chars total)`
                    : lastSyncError}
                </p>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 w-6 p-0"
                  onClick={() => {
                    navigator.clipboard.writeText(lastSyncError)
                      .then(() => {
                        // You could add a toast notification here if you have a toast system
                        console.log('Error copied to clipboard')
                      })
                      .catch((err) => {
                        console.error('Failed to copy error:', err)
                      })
                  }}
                  title="Copy full error message"
                >
                  <Copy className="h-3 w-3" />
                </Button>
              </div>
            </div>
          )}

          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Database className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">Local Events</span>
              </div>
              <span className="text-sm text-muted-foreground">{localEventCount}</span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Database className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">Server Events</span>
              </div>
              <span className="text-sm text-muted-foreground">{remoteEventCount}</span>
            </div>

            {/* Additional remote metrics can be added when exposed by the core */}

            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <CupSoda className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">User ID</span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">{weapon.user_id ? weapon.user_id.substring(0, 16) : 'Logged out'}...</span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <CupSoda className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">Device ID</span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">{weapon.device_id.substring(0, 16)}...</span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Baby className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">Yap.Town version</span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">{get_app_version()}</span>
            </div>
          </div>

          {!isOnline && (
            <div className="p-3 bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-lg">
              <p className="text-sm text-yellow-600 dark:text-yellow-400">
                You're currently offline. Changes will sync when you reconnect.
              </p>
            </div>
          )}
        </div>
        <DialogFooter>
          <Button
            onClick={handleManualSync}
            disabled={!isOnline || syncInProgress || syncInProgress}
            className="w-full"
          >
            <RefreshCw className={`mr-2 h-4 w-4 ${syncInProgress ? 'animate-spin' : ''}`} />
            {syncInProgress ? 'Syncing...' : 'Sync Now'}
          </Button>
        </DialogFooter>
      </DialogContent >
    </Dialog >
  )
}
