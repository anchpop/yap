import { get_daily_goal_options } from "../../../yap-frontend-rs/pkg";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import confetti from "canvas-confetti";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type {
  Deck,
  DailyReviewTarget,
  Language,
} from "../../../yap-frontend-rs/pkg";
import { Trophy, ChevronDown, PartyPopper } from "lucide-react";
import { cn } from "@/lib/utils";
import { TargetLanguageText } from "./TargetLanguageText";



function formatMinutes(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  return `${mins} min`;
}

function dayMessage(day: string): string {
  switch (day) {
    case "Monday":
      return "Monday can't stop you!";
    case "Tuesday":
      return "Solid Tuesday session!";
    case "Wednesday":
      return "Midweek momentum!";
    case "Thursday":
      return "Thursday well spent!";
    case "Friday":
      return "Happy Friday!";
    case "Saturday":
      return "Weekend warrior!";
    case "Sunday":
      return "So much for the day of rest!";
    default:
      return "Nice!";
  }
}

function streakMessage(streak: number): string {
  if (streak <= 1) return "you're just getting started";
  if (streak <= 3) return "keep it up!";
  if (streak <= 7) return "you're on a roll!";
  if (streak <= 14) return "impressive dedication!";
  if (streak <= 30) return "unstoppable!";
  if (streak <= 60) return "what a habit!";
  if (streak <= 100) return "legendary consistency!";
  return "absolutely incredible!";
}

interface AccomplishmentScreenProps {
  deck: Deck;
  targetLanguage: Language;
  dailyReviewTarget: DailyReviewTarget;
  onChangeDailyReviewTarget: (target: DailyReviewTarget) => void;
  onDismiss: () => void;
}

export function AccomplishmentScreen({
  deck,
  targetLanguage,
  dailyReviewTarget,
  onChangeDailyReviewTarget,
  onDismiss,
}: AccomplishmentScreenProps) {
  const [goalOpen, setGoalOpen] = useState(false);
  const [pendingTarget, setPendingTarget] =
    useState<DailyReviewTarget>(dailyReviewTarget);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const confettiRef = useRef<confetti.CreateTypes | null>(null);

  const summary = useMemo(() => deck.get_today_summary(), [deck]);
  const streak = deck.get_daily_streak();
  const wordsKnown = useMemo(() => {
    const pct = deck.get_percent_of_words_known();
    return Math.round(pct * deck.num_cards_added());
  }, [deck]);

  // Create confetti instance bound to our canvas
  useEffect(() => {
    if (!canvasRef.current) return;
    const myConfetti = confetti.create(canvasRef.current, { resize: true });
    confettiRef.current = myConfetti;
    return () => {
      myConfetti.reset();
    };
  }, []);

  const fireConfetti = useCallback(() => {
    const fire = confettiRef.current;
    if (!fire) return;
    const end = Date.now() + 800;
    const frame = () => {
      fire({
        particleCount: 3,
        angle: 60,
        spread: 55,
        origin: { x: 0, y: 0.6 },
      });
      fire({
        particleCount: 3,
        angle: 120,
        spread: 55,
        origin: { x: 1, y: 0.6 },
      });
      if (Date.now() < end) requestAnimationFrame(frame);
    };
    frame();
  }, []);

  // Fire on mount
  useEffect(() => {
    const timer = setTimeout(fireConfetti, 100);
    return () => clearTimeout(timer);
  }, [fireConfetti]);

  // Enter/Space to dismiss
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLButtonElement) return;
      if (e.code === "Enter" || e.code === "Space") {
        e.preventDefault();
        onDismiss();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onDismiss]);

  return (
    <div className="flex flex-col items-center gap-6">
      <canvas
        ref={canvasRef}
        className="fixed inset-0 w-full h-full pointer-events-none"
      />
      {/* Header */}
      <div className="flex flex-col items-center text-center gap-2 pt-4">
        <div className="rounded-xl bg-yellow-100 dark:bg-yellow-900/30 p-4">
          <Trophy className="h-10 w-10 text-yellow-500" />
        </div>
        <h2 className="text-2xl font-bold">
          Goal Reached! {dayMessage(summary.day_of_week)}
        </h2>
        <p className="text-muted-foreground">
          You studied {formatMinutes(summary.time_spent_seconds)}! (
          {summary.reviews} challenges)
        </p>
        {streak > 0 && (
          <Badge
            variant="outline"
            className="border-red-400/50 text-muted-foreground font-normal text-sm"
          >
            🔥 {streak} day streak — {streakMessage(streak)}
          </Badge>
        )}
        {/* Change goal collapsible */}
        <Collapsible
          open={goalOpen}
          onOpenChange={setGoalOpen}
          className="flex flex-col items-center"
        >
          <CollapsibleTrigger className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition-colors">
            Change daily goal
            <ChevronDown
              className={cn(
                "h-4 w-4 transition-transform",
                goalOpen && "rotate-180",
              )}
            />
          </CollapsibleTrigger>
          <CollapsibleContent className="flex flex-col items-center gap-3 mt-3">
            <div className="flex rounded-lg border overflow-hidden">
              {get_daily_goal_options().map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setPendingTarget(opt.value)}
                  className={cn(
                    "flex-1 px-3 py-2 text-sm font-medium transition-colors",
                    "border-r last:border-r-0",
                    opt.value === pendingTarget
                      ? "bg-primary text-primary-foreground"
                      : "hover:bg-muted",
                  )}
                >
                  <div>{opt.value}</div>
                  <div className="text-xs opacity-70">{opt.minutes}m</div>
                </button>
              ))}
            </div>
            <Button
              size="sm"
              disabled={pendingTarget === dailyReviewTarget}
              onClick={() => {
                onChangeDailyReviewTarget(pendingTarget);
                setGoalOpen(false);
              }}
            >
              Set goal
            </Button>
          </CollapsibleContent>
        </Collapsible>
      </div>

      {/* New today */}
      {summary.new_cards.length > 0 && (
        <div className="w-full">
          <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
            New today
          </p>
          <div className="grid grid-cols-2 gap-2">
            {summary.new_cards.map((card, i) => (
              <Card key={i} className="p-3 gap-0">
                <div className="flex justify-between items-start">
                  <p className="font-semibold">
                    <TargetLanguageText language={targetLanguage}>
                      {card.word}
                    </TargetLanguageText>
                  </p>
                  <span className="text-[10px] uppercase tracking-wider text-muted-foreground/60">
                    {card.card_type}
                  </span>
                </div>
                <p className="text-sm text-muted-foreground">
                  {card.translation}
                </p>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* Learned (graduated to Review state today) */}
      {summary.learned_cards.length > 0 && (
        <div className="w-full">
          <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
            Learned
          </p>
          <div className="grid grid-cols-2 gap-2">
            {summary.learned_cards.map((card, i) => (
              <Card key={i} className="p-3 gap-0">
                <div className="flex justify-between items-start">
                  <p className="font-semibold">
                    <TargetLanguageText language={targetLanguage}>
                      {card.word}
                    </TargetLanguageText>
                  </p>
                  <span className="text-[10px] uppercase tracking-wider text-muted-foreground/60">
                    {card.card_type}
                  </span>
                </div>
                <p className="text-sm text-muted-foreground">
                  {card.translation}
                </p>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* Back on track (relearning → review) */}
      {summary.locked_in_cards.length > 0 && (
        <div className="w-full">
          <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
            Back on track
          </p>
          <div className="grid grid-cols-2 gap-2">
            {summary.locked_in_cards.map((card, i) => (
              <Card key={i} className="p-3 gap-0">
                <div className="flex justify-between items-start">
                  <p className="font-semibold">
                    <TargetLanguageText language={targetLanguage}>
                      {card.word}
                    </TargetLanguageText>
                  </p>
                  <span className="text-[10px] uppercase tracking-wider text-muted-foreground/60">
                    {card.card_type}
                  </span>
                </div>
                <p className="text-sm text-muted-foreground">
                  {card.translation}
                </p>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* Reviewed */}
      {summary.reviewed_words.length > 0 && (
        <div className="w-full">
          <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
            Reviewed ({summary.reviewed_words.length})
          </p>
          <Card variant="light" className="p-3 gap-0" animate>
            <div className="flex flex-wrap gap-2">
              {summary.reviewed_words.map((word, i) => (
                <Badge key={i} variant="outline">
                  <TargetLanguageText language={targetLanguage}>
                    {word}
                  </TargetLanguageText>
                </Badge>
              ))}
            </div>
          </Card>
        </div>
      )}

      {/* Stats row */}
      <div className="grid grid-cols-2 gap-2 w-full">
        {summary.recall_percent != null && (
          <Card variant="light" className="p-3 gap-0 text-center">
            <p className="text-2xl font-bold">{summary.recall_percent}%</p>
            <p className="text-xs text-muted-foreground">recall</p>
          </Card>
        )}
        <Card variant="light" className="p-3 gap-0 text-center">
          <p className="text-2xl font-bold">{wordsKnown}</p>
          <p className="text-xs text-muted-foreground">words known</p>
        </Card>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2 w-full sticky bottom-0 py-3">
        <Button
          variant="outline"
          size="icon"
          onClick={fireConfetti}
          className="shrink-0"
        >
          <PartyPopper className="h-5 w-5" />
        </Button>
        <Button onClick={onDismiss} size="lg" className="flex-1">
          Continue
        </Button>
      </div>
    </div>
  );
}
