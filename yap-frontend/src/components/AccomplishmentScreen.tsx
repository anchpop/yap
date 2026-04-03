import { useEffect, useState } from "react";
import confetti from "canvas-confetti";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type { Accomplishment, DailyReviewTarget } from "../../../yap-frontend-rs/pkg";
import { Trophy, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

const TARGET_OPTIONS: Array<{ value: DailyReviewTarget; label: string; mins: number }> = [
  { value: "Casual", label: "Casual", mins: 5 },
  { value: "Regular", label: "Regular", mins: 10 },
  { value: "Serious", label: "Serious", mins: 15 },
  { value: "Intense", label: "Intense", mins: 20 },
];

interface AccomplishmentScreenProps {
  accomplishment: Accomplishment;
  dailyReviewTarget: DailyReviewTarget;
  onChangeDailyReviewTarget: (target: DailyReviewTarget) => void;
  onDismiss: () => void;
}

export function AccomplishmentScreen({
  accomplishment,
  dailyReviewTarget,
  onChangeDailyReviewTarget,
  onDismiss,
}: AccomplishmentScreenProps) {
  const [open, setOpen] = useState(false);
  const [pendingTarget, setPendingTarget] = useState<DailyReviewTarget>(dailyReviewTarget);
  const currentOption = TARGET_OPTIONS.find((o) => o.value === dailyReviewTarget);

  useEffect(() => {
    const end = Date.now() + 1500;
    const frame = () => {
      confetti({
        particleCount: 3,
        angle: 60,
        spread: 55,
        origin: { x: 0, y: 0.6 },
      });
      confetti({
        particleCount: 3,
        angle: 120,
        spread: 55,
        origin: { x: 1, y: 0.6 },
      });
      if (Date.now() < end) requestAnimationFrame(frame);
    };
    frame();
  }, []);

  return (
    <div className="flex flex-col items-center gap-4">
      <Card className="w-full p-6 gap-0" animate>
        <div className="flex flex-col items-center text-center gap-4 py-4">
          <div className="rounded-full bg-yellow-100 dark:bg-yellow-900/30 p-5">
            <Trophy className="h-10 w-10 text-yellow-500" />
          </div>
          <div className="space-y-1">
            <h2 className="text-2xl font-bold">
              {accomplishment === "DailyGoalReached" && "Daily Goal Reached!"}
            </h2>
            <p className="text-muted-foreground">
              {accomplishment === "DailyGoalReached" &&
                "You've hit your daily study goal. Great work!"}
            </p>
          </div>
          <Button onClick={onDismiss} size="lg" className="mt-2">
            {currentOption && dailyReviewTarget !== "Intense"
              ? "Keep going"
              : "Continue"}
          </Button>
        </div>
      </Card>
      <Collapsible open={open} onOpenChange={setOpen} className="flex flex-col items-center">
        <CollapsibleTrigger className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition-colors">
          Change daily goal
          <ChevronDown className={cn("h-4 w-4 transition-transform", open && "rotate-180")} />
        </CollapsibleTrigger>
        <CollapsibleContent className="flex flex-col items-center gap-3 mt-3">
          <div className="flex rounded-lg border overflow-hidden">
            {TARGET_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                onClick={() => setPendingTarget(opt.value)}
                className={cn(
                  "flex-1 px-3 py-2 text-sm font-medium transition-colors",
                  "border-r last:border-r-0",
                  opt.value === pendingTarget
                    ? "bg-primary text-primary-foreground"
                    : "hover:bg-muted"
                )}
              >
                <div>{opt.label}</div>
                <div className="text-xs opacity-70">{opt.mins}m</div>
              </button>
            ))}
          </div>
          <Button
            size="sm"
            disabled={pendingTarget === dailyReviewTarget}
            onClick={() => {
              onChangeDailyReviewTarget(pendingTarget);
              setOpen(false);
            }}
          >
            Set goal
          </Button>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
