import { Button } from "@/components/ui/button";
import TimeAgo from "react-timeago";
import { EngagementPrompts } from "@/components/engagement-prompts";
import type {
  CardSummary,
  CardType,
  ChallengeRequirements,
  DeckEvent,
  Deck,
  Language,
  ManualAddOption,
  MovieMetadataBasic,
} from "../../../yap-frontend-rs/pkg";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Headphones,
  Sparkles,
} from "lucide-react";
import { Card } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";
import type { UserInfo } from "@/App";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { Poster } from "@/components/Poster";
import { goalToGoalSelection, type Goal } from "@/hooks/useGoal";
import { useNavigate } from "react-router-dom";
import { TargetLanguageText } from "./TargetLanguageText";

export interface MovieWithMetadata extends MovieMetadataBasic {
  percent_known: number;
  all_available_learned: boolean;
  cards_to_next_milestone: number | null | undefined;
}

interface NoCardsReadyProps {
  nextDueCard: CardSummary | null;
  showEngagementPrompts: boolean;
  addEvent: (event: DeckEvent) => void;
  targetLanguage: Language;
  deck: Deck;
  bannedChallengeTypes: ChallengeRequirements[];
  userInfo: UserInfo | undefined;
  goal: Goal;
  setGoal: (goal: Goal) => void;
  moviesWithMetadata: MovieWithMetadata[];
  hasPimsleur: boolean;
}

export const NoCardsReady = memo(function NoCardsReady({
  nextDueCard,
  showEngagementPrompts,
  addEvent,
  targetLanguage,
  deck,
  bannedChallengeTypes,
  userInfo,
  goal,
  setGoal,
  moviesWithMetadata,
  hasPimsleur,
}: NoCardsReadyProps) {
  const navigate = useNavigate();
  const [pimsleurAcknowledged, setPimsleurAcknowledged] = useState(
    () => localStorage.getItem("yap-pimsleur-acknowledged") === "true",
  );
  const goalSelection = goalToGoalSelection(goal);

  // One memoized call that does the expensive next_unknown_cards computation
  const info = useMemo(
    () => deck.get_no_cards_ready_info(bannedChallengeTypes, goalSelection),
    [deck, bannedChallengeTypes, goalSelection],
  );

  // Manual add options — computed lazily on dropdown open
  const [manualAddOptions, setManualAddOptions] = useState<ManualAddOption[]>([]);
  const loadManualAddOptions = useCallback(() => {
    if (manualAddOptions.length > 0) return;
    const types: CardType[] = ["TargetLanguage", "Listening", "LetterPronunciation"];
    const options = types
      .map((t) => deck.get_manual_add_option(t, goalSelection))
      .filter((o) => userInfo !== undefined || o.card_type === "TargetLanguage" || o.card_type === "LetterPronunciation")
      .filter((o) => userInfo !== undefined || o.count > 0);
    setManualAddOptions(options);
  }, [deck, goalSelection, userInfo, manualAddOptions.length]);

  const addSmartCards = useCallback(() => {
    if (info.smart_add_event) {
      addEvent(info.smart_add_event);
    }
  }, [info.smart_add_event, addEvent]);

  let nextTargetLanguageWord: string | null = null;
  if (
    nextDueCard &&
    (nextDueCard.card_indicator.type === "WrittenPhrase" ||
      nextDueCard.card_indicator.type === "WrittenGram")
  ) {
    nextTargetLanguageWord = nextDueCard?.card_text;
  }

  // Calculate if workload looks light
  const showLightWorkloadNotification =
    info.cards_added_past_16_hours < 20 &&
    (info.upcoming_total_reviews < info.past_week_challenge_average * 21 ||
      info.upcoming_max_per_day < 10) &&
    info.upcoming_max_per_day <= 50 &&
    info.smart_add_count > 0;

  const targetLanguageSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} → English</span>
  );
  const listeningSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} listening</span>
  );
  const pronunciationSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} pronunciation</span>
  );

  // We're in NoCardsReady so due_count is 0; if there's also no future card,
  // there are zero schedulable cards (deck is empty, all leeches, or all already-known).
  const noSchedulableCards = nextDueCard === null;
  const hasNeverStudied = noSchedulableCards && deck.get_total_reviews() === 0n;

  // Keyboard shortcut to add cards
  useEffect(() => {
    const handleKeyPress = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT"
      ) {
        return;
      }

      if (
        (event.code === "Space" || event.code === "Enter") &&
        info.smart_add_event
      ) {
        event.preventDefault();
        addSmartCards();
      }
    };

    window.addEventListener("keydown", handleKeyPress);

    return () => {
      window.removeEventListener("keydown", handleKeyPress);
    };
  }, [addSmartCards, info.smart_add_event]);

  // Goal navigation — 3 tabs: essential, movie, pimsleur
  // Each tab auto-picks the best specific goal within that category
  type GoalCategory = "essential" | "movie" | "pimsleur";
  const categories: GoalCategory[] = [
    "essential",
    ...(moviesWithMetadata.length > 0 ? ["movie" as const] : []),
    ...(hasPimsleur ? ["pimsleur" as const] : []),
  ];

  const goalCategory = (g: Goal): GoalCategory => g.type;

  const currentCategory = goalCategory(goal);
  const currentCategoryIndex = categories.indexOf(currentCategory);
  // If category not found (e.g. no movies available but goal is movie), fall back to essential
  const effectiveIndex = currentCategoryIndex === -1 ? 0 : currentCategoryIndex;
  const effectiveGoal: Goal =
    currentCategoryIndex === -1 ? { type: "essential" } : goal;

  const canGoLeft = effectiveIndex > 0;
  const canGoRight = effectiveIndex < categories.length - 1;

  const goalForCategory = (category: GoalCategory): Goal => {
    switch (category) {
      case "essential":
        return { type: "essential" };
      case "movie": {
        const best = deck.get_best_movie_goal();
        if (best && best.type === "Movie") {
          return { type: "movie", movieId: best.id };
        }
        // Fallback to first movie
        return { type: "movie", movieId: moviesWithMetadata[0].id };
      }
      case "pimsleur": {
        const best = deck.get_best_pimsleur_goal();
        if (best && best.type === "PimsleurLesson") {
          return { type: "pimsleur", level: best.level, lesson: best.lesson };
        }
        return { type: "essential" };
      }
    }
  };

  const navigateGoal = (direction: "left" | "right") => {
    const nextIndex =
      direction === "left" ? effectiveIndex - 1 : effectiveIndex + 1;
    if (nextIndex >= 0 && nextIndex < categories.length) {
      setGoal(goalForCategory(categories[nextIndex]));
    }
  };

  // Goal progress info
  const tierInfo = info.tier_info;
  const { goalPercentKnown, goalDone } = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return { goalPercentKnown: tierInfo.percent_known, goalDone: false };
      case "movie": {
        const movie = moviesWithMetadata.find(
          (m) => m.id === effectiveGoal.movieId,
        );
        return {
          goalPercentKnown: movie?.percent_known ?? 0,
          goalDone: movie?.all_available_learned ?? false,
        };
      }
      case "pimsleur": {
        const stats = deck.get_pimsleur_stats();
        const lesson = stats.find(
          (l) =>
            l.level === effectiveGoal.level &&
            l.lesson === effectiveGoal.lesson,
        );
        return {
          goalPercentKnown: lesson?.percent_known ?? 0,
          goalDone: lesson?.all_available_learned ?? false,
        };
      }
    }
  })();

  const goalLabel = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return `${tierInfo.name} ${targetLanguage} Level ${tierInfo.level}`;
      case "movie":
        return (
          moviesWithMetadata.find((m) => m.id === effectiveGoal.movieId)
            ?.title ?? "Movie"
        );
      case "pimsleur":
        return `Pimsleur Level ${effectiveGoal.level}, Lesson ${effectiveGoal.lesson}`;
    }
  })();

  // Check if adding cards would cross a 5% threshold
  const currentRounded = Math.floor(goalPercentKnown / 5) * 5;
  const afterRounded = Math.floor(info.percent_known_after / 5) * 5;
  const crossesThreshold = afterRounded > currentRounded;
  const thresholdTarget = crossesThreshold ? afterRounded : null;

  const goalImage = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return { type: "url" as const, url: "/essential-course.webp" };
      case "movie":
        return { type: "movie" as const, movieId: effectiveGoal.movieId };
      case "pimsleur":
        return null;
    }
  })();

  return (
    <div className="space-y-4">
      <div className="text-center py-4">
        <div className="flex flex-col gap-2">
          <p className="text-2xl font-bold">
            {hasNeverStudied
              ? "Ready to start learning?"
              : noSchedulableCards
                ? "Ready for more?"
                : "All caught up!"}
          </p>
          {hasNeverStudied ? (
            <p className="text-muted-foreground">
              We'll start with a couple words you might know.
            </p>
          ) : noSchedulableCards ? (
            <p className="text-muted-foreground">
              Add some cards to keep building your vocabulary.
            </p>
          ) : (
            <p className="text-muted-foreground">
              {nextTargetLanguageWord ? (
                <>
                  You'll review{" "}
                  <span className="font-semibold">
                    <TargetLanguageText language={targetLanguage}>
                      {nextTargetLanguageWord}
                    </TargetLanguageText>
                  </span>{" "}
                  {nextDueCard ? (
                    <TimeAgo date={new Date(nextDueCard.due_timestamp_ms)} />
                  ) : (
                    "soon"
                  )}
                  .
                </>
              ) : (
                <>
                  Your next review is{" "}
                  {nextDueCard ? (
                    <TimeAgo date={new Date(nextDueCard.due_timestamp_ms)} />
                  ) : (
                    "soon"
                  )}
                  .
                </>
              )}
            </p>
          )}
        </div>
      </div>

      {noSchedulableCards ? (
        <div className="flex justify-center">
          <Button
            onClick={addSmartCards}
            variant="default"
            size="lg"
            className="group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg"
          >
            <span className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent translate-x-[-200%] group-hover:translate-x-[200%] transition-transform duration-1000"></span>
            <Sparkles className="h-5 w-5 mr-2 animate-pulse" />
            Start learning
          </Button>
        </div>
      ) : (
        <Card className="overflow-hidden px-2 py-4 gap-2" animate>
          <p className="text-lg font-semibold px-4 sm:px-8 text-center">
            {goalDone ? (
              <>
                You're all done with
                <br />
                <span className="uppercase font-bold">{goalLabel}!</span>
              </>
            ) : showLightWorkloadNotification && thresholdTarget !== null ? (
              <>
                Soon you'll hit {thresholdTarget}% on
                <br />
                <span className="uppercase font-bold">{goalLabel}!</span>
              </>
            ) : showLightWorkloadNotification ? (
              <>
                Keep up the momentum on
                <br />
                <span className="uppercase font-bold">{goalLabel}!</span>
              </>
            ) : (
              <>
                You're doing great on
                <br />
                <span className="uppercase font-bold">{goalLabel}!</span>
              </>
            )}
          </p>
          <div className="flex items-center justify-between gap-0">
            <button
              onClick={() => navigateGoal("left")}
              className={`hidden sm:flex p-2 self-stretch items-center transition-colors ${canGoLeft ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
              disabled={!canGoLeft}
              aria-label="Previous goal"
            >
              <ChevronLeft className="h-6 w-6" />
            </button>

            <div className="flex-1 flex flex-col sm:flex-row items-center gap-4">
              {effectiveGoal.type === "pimsleur" && !pimsleurAcknowledged ? (
                <div className="flex-1 flex flex-col items-center gap-3 py-4 px-2 text-center">
                  <Headphones className="h-8 w-8 text-muted-foreground" />
                  <p className="text-sm text-muted-foreground">
                    Yap has word lists for Pimsleur, but is not affiliated with
                    Pimsleur in any way.
                  </p>
                  <Button
                    variant="default"
                    onClick={() => {
                      localStorage.setItem("yap-pimsleur-acknowledged", "true");
                      setPimsleurAcknowledged(true);
                    }}
                  >
                    I understand
                  </Button>
                </div>
              ) : (
                <>
                  <div
                    onClick={() => navigate("/goals")}
                    className="hidden sm:block sm:order-first w-24 h-36 flex-shrink-0 rounded-lg border border-border/50 overflow-hidden cursor-pointer hover:scale-105 transition-all"
                  >
                    {goalImage?.type === "url" ? (
                      <img
                        src={goalImage.url}
                        alt={goalLabel}
                        className={`w-full h-full object-cover opacity-90 saturate-70 dark:opacity-70 dark:saturate-80 hover:opacity-100 hover:saturate-100 transition-all ${effectiveGoal.type === "essential" ? "dark:invert dark:hue-rotate-180" : ""}`}
                      />
                    ) : goalImage?.type === "movie" ? (
                      <Poster
                        movieId={goalImage.movieId}
                        deck={deck}
                        alt={goalLabel}
                      />
                    ) : (
                      <div className="w-full h-full bg-muted flex items-center justify-center">
                        <Headphones className="h-8 w-8 text-muted-foreground" />
                      </div>
                    )}
                  </div>
                  <div className="order-1 sm:order-last flex-1 flex flex-col items-center sm:items-start gap-3 min-w-0 w-full sm:w-auto">
                    {goalDone ? (
                      (() => {
                        // Show "next lesson" / "next movie" button when goal is complete
                        const nextGoal = (() => {
                          switch (effectiveGoal.type) {
                            case "pimsleur": {
                              const best = deck.get_best_pimsleur_goal();
                              if (best && best.type === "PimsleurLesson") {
                                return {
                                  goal: {
                                    type: "pimsleur" as const,
                                    level: best.level,
                                    lesson: best.lesson,
                                  },
                                  label: "Next lesson",
                                };
                              }
                              return null;
                            }
                            case "movie": {
                              const best = deck.get_best_movie_goal();
                              if (best && best.type === "Movie") {
                                return {
                                  goal: {
                                    type: "movie" as const,
                                    movieId: best.id,
                                  },
                                  label: "Next movie",
                                };
                              }
                              return null;
                            }
                            case "essential":
                              return null;
                          }
                        })();

                        return nextGoal ? (
                          <Button
                            onClick={() => setGoal(nextGoal.goal)}
                            variant="default"
                            size="lg"
                            className="group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg"
                          >
                            <ChevronRight className="h-5 w-5 mr-2" />
                            {nextGoal.label}
                          </Button>
                        ) : (
                          <p className="text-sm">
                            You've learned all available words!
                          </p>
                        );
                      })()
                    ) : info.smart_add_count > 0 ? (
                      <div className="flex">
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              onClick={addSmartCards}
                              variant="default"
                              size="lg"
                              className="group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg whitespace-normal h-auto min-h-10 rounded-r-none"
                            >
                              <span className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent translate-x-[-200%] group-hover:translate-x-[200%] transition-transform duration-1000"></span>
                              <Sparkles className="h-5 w-5 mr-2 animate-pulse" />
                              Learn {info.smart_add_count} new{" "}
                              {info.smart_add_count === 1 ? "card" : "cards"}
                              {thresholdTarget !== null &&
                                !showLightWorkloadNotification && (
                                  <> to hit {thresholdTarget}%</>
                                )}
                            </Button>
                          </TooltipTrigger>
                          {info.preview.length > 0 && (
                            <TooltipContent>
                              {info.preview.join(", ")}
                            </TooltipContent>
                          )}
                        </Tooltip>
                        <DropdownMenu onOpenChange={(open) => { if (open) loadManualAddOptions(); }}>
                          <DropdownMenuTrigger asChild>
                            <Button
                              variant="default"
                              size="lg"
                              className="rounded-l-none border-l border-l-primary-foreground/20 px-2"
                            >
                              <ChevronDown className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {manualAddOptions.filter(o => o.count > 0).map((option) => (
                              <DropdownMenuItem
                                key={option.card_type}
                                onClick={() => option.event && addEvent(option.event)}
                                className="cursor-pointer"
                              >
                                <Sparkles className="h-4 w-4 mr-2" />
                                Learn {option.count}{" "}
                                {option.card_type === "TargetLanguage"
                                  ? targetLanguageSpan
                                  : option.card_type === "Listening"
                                    ? listeningSpan
                                    : option.card_type === "LetterPronunciation"
                                      ? pronunciationSpan
                                      : ""}{" "}
                                {option.count === 1 ? "card" : "cards"}
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                    ) : (
                      <p className="text-sm">
                        You've learned all available words!
                      </p>
                    )}

                    <Progress
                      value={goalPercentKnown}
                      projectedValue={
                        goalDone
                          ? undefined
                          : info.percent_known_after
                      }
                      showPercentage
                      label={goalDone ? "Done!" : undefined}
                      className="h-6"
                    />

                    {effectiveGoal.type === "essential" && (
                      <p className="text-xs text-muted-foreground text-center sm:text-left">
                        When you complete this level, you'll understand{" "}
                        {tierInfo.percent_of_usage.toFixed(1)}% of everyday{" "}
                        {targetLanguage}.
                      </p>
                    )}

                    <button
                      onClick={() => navigate("/goals")}
                      className="text-xs text-foreground/60 hover:text-foreground underline underline-offset-2 transition-colors text-left"
                    >
                      change goal
                    </button>

                    {categories.length > 1 && (
                      <div className="flex sm:hidden items-center justify-between w-full">
                        <button
                          onClick={() => navigateGoal("left")}
                          className={`p-2 transition-colors ${canGoLeft ? "text-foreground/60 hover:text-foreground" : "text-transparent cursor-default"}`}
                          disabled={!canGoLeft}
                          aria-label="Previous goal"
                        >
                          <ChevronLeft className="h-6 w-6" />
                        </button>
                        <button
                          onClick={() => navigateGoal("right")}
                          className={`p-2 transition-colors ${canGoRight ? "text-foreground/60 hover:text-foreground" : "text-transparent cursor-default"}`}
                          disabled={!canGoRight}
                          aria-label="Next goal"
                        >
                          <ChevronRight className="h-6 w-6" />
                        </button>
                      </div>
                    )}
                  </div>
                </>
              )}
            </div>

            <button
              onClick={() => navigateGoal("right")}
              className={`hidden sm:flex p-2 self-stretch items-center transition-colors ${canGoRight ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
              disabled={!canGoRight}
              aria-label="Next goal"
            >
              <ChevronRight className="h-6 w-6" />
            </button>
          </div>
        </Card>
      )}

      {!hasNeverStudied && <WeekProgressStrip deck={deck} />}

      {showEngagementPrompts && <EngagementPrompts language={targetLanguage} />}
    </div>
  );
});

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

function WeekProgressStrip({ deck }: { deck: Deck }) {
  const day = useMidnightTick();
  const week = useMemo(() => deck.get_current_week_progress(), [deck, day]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="flex w-full items-stretch">
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
