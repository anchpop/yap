import { useEffect, useMemo, useState } from "react";
import type { Deck } from "../../../yap-frontend-rs/pkg";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

const DAY_LABELS = ["M", "T", "W", "T", "F", "S", "S"];

function useMidnightTick() {
  const [day, setDay] = useState(() => new Date().toDateString());
  useEffect(() => {
    const now = new Date();
    const midnight = new Date(now);
    midnight.setHours(24, 0, 0, 0);
    const ms = midnight.getTime() - now.getTime();
    const timer = setTimeout(() => setDay(new Date().toDateString()), ms);
    return () => clearTimeout(timer);
  }, [day]);
  return day;
}

export function WeekProgressStrip({
  deck,
  className = "",
}: {
  deck: Deck;
  className?: string;
}) {
  const day = useMidnightTick();
  const week = useMemo(() => deck.get_current_week_progress(), [deck, day]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className={`flex w-full items-stretch ${className}`}>
      {week.map((day, i) => (
        <DayCell key={i} day={day} index={i} />
      ))}
    </div>
  );
}

function DayCell({ day, index }: { day: { seconds: number; target_seconds: number; reviews: number; new_cards: number; learned_cards: number; locked_in_cards: number; met_goal: boolean; is_today: boolean; is_future: boolean }; index: number }) {
  const fillPercent = day.is_future || day.target_seconds === 0
    ? 0
    : Math.min(100, (day.seconds / day.target_seconds) * 100);

  const borderL = "";
  const rounded =
    index === 0
      ? "rounded-l-lg"
      : index === 6
        ? "rounded-r-lg"
        : "";
  const todayClasses = day.is_today
    ? "scale-110 z-10 rounded-lg shadow-lg border border-border"
    : "";

  const content = (
    <>
      <span className="text-lg font-semibold leading-none tabular-nums">
        {day.is_future ? "·" : `+${day.new_cards + day.learned_cards + day.locked_in_cards}`}
      </span>
      <span className="text-[10px] uppercase tracking-wide opacity-70">
        {DAY_LABELS[index]}
      </span>
    </>
  );

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className={`flex-1 relative overflow-hidden ${borderL} ${rounded} ${todayClasses} ${day.is_future ? "bg-muted/30" : `backdrop-brightness-105 backdrop-saturate-120 dark:backdrop-brightness-100 backdrop-blur-sm ${day.seconds > 0 ? "bg-foreground/10" : ""}`}`}
        >
          {/* Green fill bar */}
          {!day.is_future && fillPercent > 0 && (
            <div
              className={`absolute inset-0 ${fillPercent >= 100 ? "bg-foreground" : "bg-foreground/90"}`}
              style={{ clipPath: `inset(${100 - fillPercent}% 0 0 0)` }}
            />
          )}

          {/* Base text layer (normal foreground color) */}
          <div className={`relative flex flex-col items-center justify-center py-3 gap-0.5 ${day.is_future ? "text-muted-foreground/60" : "text-foreground"}`}>
            {content}
          </div>

          {/* Clipped text layer (white, revealed by fill) */}
          {!day.is_future && fillPercent > 0 && (
            <div
              className="absolute inset-0 flex flex-col items-center justify-center py-3 gap-0.5 text-background"
              style={{ clipPath: `inset(${100 - fillPercent}% 0 0 0)` }}
            >
              {content}
            </div>
          )}
        </div>
      </TooltipTrigger>
      <TooltipContent>
        {day.is_future
          ? "Upcoming"
          : day.reviews === 0
            ? "No activity"
            : `${day.new_cards} added · ${day.learned_cards + day.locked_in_cards} back on track`}
      </TooltipContent>
    </Tooltip>
  );
}
