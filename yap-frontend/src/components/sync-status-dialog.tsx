import { useEffect, useState, useMemo } from "react";
import { useInterval } from "react-use";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import {
  RefreshCw,
  Check,
  X,
  Database,
  Cloud,
  Baby,
  CupSoda,
} from "lucide-react";
import {
  get_app_version,
  type EarliestUnsyncedEvent,
  type SyncState,
} from "../../../yap-frontend-rs/pkg";
import { useWeapon, useSyncActions } from "@/weapon";
import { useNetworkState } from "react-use";
import { ErrorMessage } from "@/components/ui/error-message";
import {
  useImpersonationActivation,
  ImpersonateUser,
} from "@/components/impersonate-user";

export function SyncStatusDialog() {
  const weapon = useWeapon();
  const { syncNow } = useSyncActions();
  const { online: isOnline } = useNetworkState();
  const { activated: impersonationActivated, handleActivationClick } =
    useImpersonationActivation();

  // Poll sync state and earliest unsynced timestamp every second
  const [lastSyncFinishedAt, setLastSyncFinishedAt] = useState<number | null>(
    null,
  );
  const [lastSyncError, setLastSyncError] = useState<string | null>(null);
  const [earliestUnsyncedAt, setEarliestUnsyncedAt] = useState<number | null>(
    null,
  );
  const [syncInProgress, setSyncInProgress] = useState<boolean>(false);
  const [currentTimestamp, setCurrentTimestamp] = useState(() => Date.now());

  // Update timestamp periodically for responsive UI
  useInterval(
    () => {
      setCurrentTimestamp(Date.now());
    },
    1000, // Update every second
  );

  useEffect(() => {
    const update = () => {
      try {
        const s: SyncState<string, string> = weapon.get_sync_state("supabase");
        const started = s.lastSyncStarted;
        const finished = s.lastSyncFinished;
        setLastSyncFinishedAt(finished);
        setLastSyncError(s.lastSyncError ?? null);
        setSyncInProgress(!!started && (!finished || started > finished));

        const earliest: EarliestUnsyncedEvent | undefined =
          weapon.get_timestamp_of_earliest_unsynced_event("supabase");
        if (earliest && earliest.timestamp) {
          // Handle timestamp - it comes as an ISO string from WASM
          const timestampMs = new Date(earliest.timestamp).getTime();
          setEarliestUnsyncedAt(timestampMs);
        } else {
          setEarliestUnsyncedAt(null);
        }
      } catch (e) {
        // ignore polling errors
        console.error("Error polling sync status:", e);
      }
    };
    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, [weapon]);

  const handleManualSync = async () => {
    setSyncInProgress(true);
    await syncNow();
  };

  const handleOpenChange = (open: boolean) => {
    if (open && isOnline) {
      // Trigger sync when dialog opens and online
      syncNow();
    }
  };

  // Get local and remote event counts
  const localEventCount = weapon.num_events;
  const remoteEventCount =
    weapon.num_events_on_remote_as_of_last_sync("supabase");

  // Determine sync status
  const unsyncedStale = useMemo(() => {
    return (
      earliestUnsyncedAt != null && currentTimestamp - earliestUnsyncedAt > 5000
    );
  }, [earliestUnsyncedAt, currentTimestamp]);

  let statusIcon;
  let statusText;
  let statusColor;

  if (!isOnline) {
    statusIcon = <Cloud className="w-2 h-2" />;
    statusText = "Offline";
    statusColor = "text-gray-600 dark:text-gray-400";
  } else if (lastSyncError) {
    statusIcon = <X className="w-2 h-2" />;
    statusText = "Sync error";
    statusColor = "text-red-600 dark:text-red-400";
  } else if (isOnline) {
    if (unsyncedStale) {
      statusIcon = <RefreshCw className="w-2 h-2" />;
      statusText = "Unsynced";
      statusColor = "text-yellow-700 dark:text-yellow-400";
    } else {
      statusIcon = <Check className="w-2 h-2" />;
      statusText = "Synced";
      statusColor = "text-muted-foreground";
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <button className="flex items-center gap-1.5 hover:opacity-80 transition-opacity">
          <span
            className={`hidden sm:inline text-sm ${statusColor} transition-colors duration-300`}
          >
            {statusText}
          </span>
          <span
            className={`w-2 h-2 rounded-full ${
              !isOnline
                ? "bg-gray-500 dark:bg-gray-400"
                : lastSyncError
                  ? "bg-red-500 dark:bg-red-400"
                  : unsyncedStale
                    ? "bg-yellow-600 dark:bg-yellow-400"
                    : ""
            } transition-colors duration-300`}
          ></span>
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
            <ErrorMessage
              title="Sync Error"
              message={lastSyncError}
              variant="compact"
            />
          )}

          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Database className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  Local Events
                </span>
              </div>
              <span className="text-sm text-muted-foreground">
                {localEventCount}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Database className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  Server Events
                </span>
              </div>
              <span className="text-sm text-muted-foreground">
                {remoteEventCount}
              </span>
            </div>

            {/* Additional remote metrics can be added when exposed by the core */}

            <div
              className="flex items-center justify-between cursor-default select-none"
              onClick={handleActivationClick}
            >
              <div className="flex items-center gap-2">
                <CupSoda className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  User ID
                </span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">
                {weapon.user_id
                  ? weapon.user_id.substring(0, 16)
                  : "Logged out"}
                ...
              </span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <CupSoda className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  Device ID
                </span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">
                {weapon.device_id.substring(0, 16)}...
              </span>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Baby className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  Yap.Town version
                </span>
              </div>
              <span className="text-sm text-muted-foreground font-mono">
                {get_app_version()}
              </span>
            </div>
          </div>

          {impersonationActivated && <ImpersonateUser />}

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
            <RefreshCw
              className={`mr-2 h-4 w-4 ${syncInProgress ? "animate-spin" : ""}`}
            />
            {syncInProgress ? "Syncing..." : "Sync Now"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
