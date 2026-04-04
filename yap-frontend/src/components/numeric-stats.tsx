import React from "react";
import { Card } from "@/components/ui/card";

interface NumericStatsProps {
  xp: number;
  totalCards: number;
  cardsReady: number;
  percentKnown: number;
  dailyStreak: number;
  totalReviews: number | bigint;
  tierName?: string;
  tierPercent?: number;
  todayReviews: number;
  todayTimeSpent: number;
  dailyTargetSeconds: number;
}

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins === 0) return `${secs}s`;
  return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`;
}

export const NumericStats: React.FC<NumericStatsProps> = ({
  xp,
  totalCards,
  cardsReady,
  percentKnown,
  dailyStreak,
  totalReviews,
  tierName,
  tierPercent,
  todayReviews,
  todayTimeSpent,
  dailyTargetSeconds,
}) => {
  const goalReached = todayTimeSpent >= dailyTargetSeconds;
  const targetMins = Math.round(dailyTargetSeconds / 60);
  return (
    <div className="mb-4">
      <h2 className="text-2xl font-semibold animate-fade-in-delay-2">Stats</h2>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-3">
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">XP</p>
          <p className="text-2xl font-bold">{xp}</p>
          <p className="text-sm text-muted-foreground mt-1">
            You get more XP for words you didn't remember.
          </p>
        </Card>
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">Daily Goal</p>
          <p className="text-2xl font-bold">
            {formatTime(todayTimeSpent)} / {targetMins}m
          </p>
          <p className="text-sm text-muted-foreground mt-1">
            {goalReached ? "Goal reached!" : `${todayReviews} reviews today`}
          </p>
        </Card>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">Total Cards</p>
          <p className="text-2xl font-bold">{totalCards}</p>
          <p className="text-sm text-muted-foreground mt-1">{cardsReady} ready now</p>
        </Card>
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">{tierName ?? "Words Known"}</p>
          <p className="text-2xl font-bold">{(tierPercent ?? percentKnown).toFixed(1)}%</p>
          <p className="text-sm text-muted-foreground mt-1">{percentKnown.toFixed(1)}% overall</p>
        </Card>
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">Daily Streak</p>
          <p className="text-2xl font-bold">{dailyStreak}</p>
          <p className="text-sm text-muted-foreground mt-1">days</p>
        </Card>
        <Card className="p-4 gap-0" animate>
          <p className="text-sm text-muted-foreground mb-1">Total Reviews</p>
          <p className="text-2xl font-bold">{totalReviews.toString()}</p>
          <p className="text-sm text-muted-foreground mt-1">all time</p>
        </Card>
      </div>
    </div>
  );
};
