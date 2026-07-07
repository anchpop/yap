import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button.tsx";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ChartContainer } from "@/components/ui/chart";
import type { ChartConfig } from "@/components/ui/chart";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts";
import type {
  Deck,
  Language,
  SimulationSample,
} from "../../../yap-frontend-rs/pkg";
import { getMovieMetadata } from "@/lib/movie-cache";
import { TargetLanguageText } from "@/components/TargetLanguageText";
import { BetaFeatureBanner } from "@/components/BetaFeatureBanner";

const CHUNK_DAYS = 1;
const MAX_DAYS = 365;
const GOAL_REACHED_PERCENT = 95;
const DAY_MS = 24 * 60 * 60 * 1000;
const DEFAULT_NEW_CARDS_PER_DAY = 10;
const MAX_NEW_CARDS_PER_DAY = 100;

const chartConfig = {
  goal: {
    label: "Goal",
    color: "var(--chart-1)",
  },
  overall: {
    label: "Overall",
    color: "var(--chart-2)",
  },
} satisfies ChartConfig;

function formatDate(timestampMs: number): string {
  return new Date(timestampMs).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

function SimStat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="text-sm font-medium tabular-nums">{value}</div>
    </div>
  );
}

interface SimulateProps {
  deck: Deck;
  targetLanguage: Language;
}

export function Simulate({ deck, targetLanguage }: SimulateProps) {
  const [samples, setSamples] = useState<SimulationSample[]>([]);
  const [status, setStatus] = useState<"idle" | "running" | "done">("idle");
  const [startTime, setStartTime] = useState(() => Date.now());
  const [newCardsInput, setNewCardsInput] = useState(
    String(DEFAULT_NEW_CARDS_PER_DAY),
  );
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const runIdRef = useRef(0);

  const newCardsPerDay = Math.min(
    MAX_NEW_CARDS_PER_DAY,
    Math.max(1, parseInt(newCardsInput, 10) || DEFAULT_NEW_CARDS_PER_DAY),
  );

  // Invalidates any in-flight simulation loop (it checks its run id between chunks)
  const cancelActiveRun = useCallback(() => {
    runIdRef.current++;
  }, []);

  // Cancel any in-flight simulation when this component unmounts
  useEffect(() => cancelActiveRun, [cancelActiveRun]);

  const goal = useMemo(() => {
    const selection = deck.get_sentence_list();
    if (!selection) return null;
    if (selection.type === "Movie") {
      const [meta] = getMovieMetadata(deck, [selection.id]);
      return {
        label: meta?.title ?? "your movie",
        isMovie: true,
      };
    }
    return {
      label: `Pimsleur Level ${selection.level}, Lesson ${selection.lesson}`,
      isMovie: false,
    };
  }, [deck]);

  const run = async () => {
    const runId = ++runIdRef.current;
    const start = Date.now();
    setStartTime(start);
    setSamples([]);
    setSelectedIndex(null);
    setStatus("running");

    const simulation = deck.start_simulation(start, newCardsPerDay);
    try {
      let simulatedDays = 0;
      while (simulatedDays < MAX_DAYS) {
        const batch = simulation.simulate_days(
          Math.min(CHUNK_DAYS, MAX_DAYS - simulatedDays),
        );
        if (runIdRef.current !== runId) return;
        simulatedDays += batch.length;
        setSamples((prev) => [...prev, ...batch]);

        const last = batch[batch.length - 1];
        if (!last) break;
        if (
          last.goal_percent != null &&
          last.goal_percent >= GOAL_REACHED_PERCENT
        ) {
          break;
        }

        // Yield to the main thread so the chart renders between chunks
        await new Promise((resolve) => setTimeout(resolve, 0));
        if (runIdRef.current !== runId) return;
      }
      setStatus("done");
    } finally {
      simulation.free();
    }
  };

  const stop = () => {
    cancelActiveRun();
    setStatus("done");
  };

  const chartData = useMemo(
    () =>
      samples.map((sample) => ({
        day: sample.day,
        date: formatDate(startTime + sample.day * DAY_MS),
        goal: sample.goal_percent ?? undefined,
        overall: sample.overall_percent,
      })),
    [samples, startTime],
  );

  const goalReachedSample = useMemo(
    () =>
      goal
        ? samples.find(
            (sample) =>
              sample.goal_percent != null &&
              sample.goal_percent >= GOAL_REACHED_PERCENT,
          )
        : undefined,
    [samples, goal],
  );

  const goalName = goal ? (
    goal.isMovie ? (
      <TargetLanguageText language={targetLanguage}>
        {goal.label}
      </TargetLanguageText>
    ) : (
      goal.label
    )
  ) : null;

  const lastSample = samples[samples.length - 1];
  const selectedSample =
    (selectedIndex != null ? samples[selectedIndex] : undefined) ?? lastSample;

  return (
    <div className="flex flex-col gap-4 max-w-3xl mx-auto w-full p-4">
      <BetaFeatureBanner />
      <Card className="p-4 gap-2">
        <h3 className="text-lg font-semibold">Simulate your future</h3>
        <p className="text-sm text-muted-foreground">
          Fast-forward your deck up to a year: each simulated day you clear
          your reviews and add new cards
          {goal ? <> from {goalName}</> : null}
          , and watch your comprehension grow.
        </p>
        <div className="flex flex-wrap items-center gap-3 mt-2">
          {status === "running" ? (
            <Button variant="outline" onClick={stop}>
              Stop
            </Button>
          ) : (
            <Button onClick={run}>
              {status === "done" ? "Simulate again" : "Simulate"}
            </Button>
          )}
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <Input
              type="number"
              min={1}
              max={MAX_NEW_CARDS_PER_DAY}
              value={newCardsInput}
              onChange={(e) => setNewCardsInput(e.target.value)}
              disabled={status === "running"}
              className="w-20"
            />
            new cards per day
          </label>
          {status === "running" && (
            <span className="text-sm text-muted-foreground">
              Simulating… {samples.length} days
            </span>
          )}
        </div>
      </Card>

      {samples.length > 0 && (
        <Card className="p-4 gap-2">
          <ChartContainer config={chartConfig} className="h-[300px] w-full">
            <LineChart
              data={chartData}
              onMouseMove={(state) => {
                const index = state?.activeTooltipIndex;
                setSelectedIndex(typeof index === "number" ? index : null);
              }}
              onMouseLeave={() => setSelectedIndex(null)}
            >
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis dataKey="date" minTickGap={48} className="text-xs" />
              <YAxis
                domain={[0, 100]}
                ticks={[0, 25, 50, 75, 100]}
                width={36}
                className="text-xs"
              />
              {goal && (
                <ReferenceLine
                  y={GOAL_REACHED_PERCENT}
                  strokeDasharray="4 4"
                  className="stroke-muted-foreground"
                />
              )}
              {goal && (
                <Line
                  type="monotone"
                  dataKey="goal"
                  stroke="var(--color-goal)"
                  strokeWidth={2}
                  dot={false}
                  isAnimationActive={false}
                />
              )}
              <Line
                type="monotone"
                dataKey="overall"
                stroke="var(--color-overall)"
                strokeWidth={2}
                dot={false}
                isAnimationActive={false}
              />
            </LineChart>
          </ChartContainer>

          {selectedSample && (
            <div className="grid grid-cols-3 sm:grid-cols-7 gap-x-4 gap-y-2 mt-2">
              <SimStat
                label="Date"
                value={formatDate(startTime + selectedSample.day * DAY_MS)}
              />
              {goal && selectedSample.goal_percent != null && (
                <SimStat
                  label="Goal"
                  value={`${selectedSample.goal_percent.toFixed(1)}%`}
                />
              )}
              <SimStat
                label="Overall"
                value={`${selectedSample.overall_percent.toFixed(1)}%`}
              />
              <SimStat
                label="Reviews"
                value={selectedSample.reviews.toLocaleString()}
              />
              <SimStat
                label="Total reviews"
                value={selectedSample.total_reviews.toLocaleString()}
              />
              <SimStat
                label="New cards"
                value={selectedSample.new_cards.toLocaleString()}
              />
              <SimStat
                label="Total cards"
                value={selectedSample.total_cards.toLocaleString()}
              />
            </div>
          )}

          {status === "done" && lastSample && (
            <p className="text-sm mt-1">
              {goalReachedSample ? (
                <>
                  {goal?.isMovie ? "🍿 " : "🎉 "}
                  {goalName} reaches {GOAL_REACHED_PERCENT}% around{" "}
                  <span className="font-semibold">
                    {formatDate(startTime + goalReachedSample.day * DAY_MS)}
                  </span>
                  .
                </>
              ) : goal && lastSample.day >= MAX_DAYS ? (
                <>
                  {goalName} doesn't reach {GOAL_REACHED_PERCENT}% within a
                  year at this pace — keep at it!
                </>
              ) : (
                <>
                  By{" "}
                  <span className="font-semibold">
                    {formatDate(startTime + lastSample.day * DAY_MS)}
                  </span>{" "}
                  you'd know{" "}
                  <span className="font-semibold">
                    {lastSample.overall_percent.toFixed(1)}%
                  </span>{" "}
                  of words
                  {goal && lastSample.goal_percent != null ? (
                    <>
                      {" "}
                      overall and {lastSample.goal_percent.toFixed(1)}% of{" "}
                      {goalName}
                    </>
                  ) : null}
                  , up from {samples[0].overall_percent.toFixed(1)}% today.
                </>
              )}
            </p>
          )}

        </Card>
      )}
    </div>
  );
}
