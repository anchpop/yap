import { useCallback, useEffect, useState } from "react";
import { definePluginApp, useRpc } from "@get-bb/plugin-sdk/app";
import type { Metrics, rpcContract } from "./server";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

function MetricCard({ label, value, detail }: { label: string; value: number; detail: string }) {
  return (
    <Card>
      <CardHeader className="gap-2 pb-3">
        <CardDescription>{label}</CardDescription>
        <CardTitle className="text-4xl tabular-nums">{value.toLocaleString()}</CardTitle>
      </CardHeader>
      <CardContent className="text-sm text-muted-foreground">{detail}</CardContent>
    </Card>
  );
}

function formatWindow(start: string): string {
  return `Since ${new Date(start).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  })}`;
}

function MetricsPanel() {
  const rpc = useRpc<typeof rpcContract>();
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      setMetrics(await rpc.call("getMetrics"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not load Yap metrics");
    } finally {
      setIsLoading(false);
    }
  }, [rpc]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="h-full overflow-y-auto p-4 md:p-5">
      <div className="mx-auto w-full max-w-3xl space-y-4">
        <div className="flex items-center justify-between gap-3">
          <p className="text-sm text-muted-foreground">
            Rolling product metrics from Supabase
          </p>
          <Button size="sm" variant="outline" disabled={isLoading} onClick={() => void load()}>
            {isLoading ? "Refreshing…" : "Refresh"}
          </Button>
        </div>

        {error ? (
          <Card className="border-destructive/50">
            <CardHeader>
              <CardTitle className="text-base">Metrics unavailable</CardTitle>
              <CardDescription>{error}</CardDescription>
            </CardHeader>
          </Card>
        ) : metrics ? (
          <>
            <div className="grid gap-4 sm:grid-cols-2">
              <MetricCard
                label="Weekly active users"
                value={metrics.weeklyActiveUsers}
                detail={formatWindow(metrics.activityWindowStart)}
              />
              <MetricCard
                label="Sign-ups in the past month"
                value={metrics.signupsPastMonth}
                detail={formatWindow(metrics.signupWindowStart)}
              />
            </div>
            <p className="text-xs text-muted-foreground">
              Updated {new Date(metrics.generatedAt).toLocaleString()}. Active users are unique
              accounts with at least one synced event.
            </p>
          </>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2" aria-label="Loading metrics">
            <div className="h-36 animate-pulse rounded-lg border border-border bg-card" />
            <div className="h-36 animate-pulse rounded-lg border border-border bg-card" />
          </div>
        )}
      </div>
    </div>
  );
}

export default definePluginApp((app) => {
  app.slots.navPanel({
    id: "yap-metrics",
    title: "Yap Metrics",
    icon: "ChartColumn",
    path: "metrics",
    component: MetricsPanel,
  });
});
