import { Button } from "@/components/ui/button";
import TimeAgo from "react-timeago";
import { EngagementPrompts } from "@/components/engagement-prompts";
import type {
  AddCardOptions,
  CardSummary,
  CardType,
  ChallengeRequirements,
  Deck,
  Language,
} from "../../../yap-frontend-rs/pkg";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ChevronDown, ChevronLeft, ChevronRight, Headphones, Sparkles } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import type { UserInfo } from "@/App";
import { useEffect, useState } from "react";
import { getPosterDataUrl } from "@/lib/poster-utils";
import { goalToGoalSelection, type Goal } from "@/hooks/useGoal";
import { useNavigate } from "react-router-dom";

export interface MovieWithMetadata {
  id: string;
  percent_known: number;
  all_available_learned: boolean;
  cards_to_next_milestone: number | null | undefined;
  title?: string;
  year?: number;
  original_language?: string;
  poster_bytes?: number[];
}

interface NoCardsReadyProps {
  nextDueCard: CardSummary | null;
  showEngagementPrompts: boolean;
  addNextCards: (card_type: CardType | undefined, count: number) => void;
  targetLanguage: Language;
  deck: Deck;
  bannedChallengeTypes: ChallengeRequirements[];
  userInfo: UserInfo | undefined;
  goal: Goal;
  setGoal: (goal: Goal) => void;
  moviesWithMetadata: MovieWithMetadata[];
  hasPimsleur: boolean;
}

export function NoCardsReady({
  nextDueCard,
  showEngagementPrompts,
  addNextCards,
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
    () => localStorage.getItem("yap-pimsleur-acknowledged") === "true"
  );
  const goalSelection = goalToGoalSelection(goal);
  const addCardOptionsRaw = deck.add_card_options(bannedChallengeTypes, goalSelection);
  const addCardOptions: AddCardOptions =
    userInfo === undefined
      ? {
          smart_add: 0,
          manual_add: addCardOptionsRaw.manual_add.map(
            ([count, card_type]) =>
              [
                card_type == "TargetLanguage" ||
                card_type == "LetterPronunciation"
                  ? count
                  : 0,
                card_type,
              ] as [number, CardType]
          ),
          percent_known_after: addCardOptionsRaw.percent_known_after,
          preview: addCardOptionsRaw.preview,
        }
      : addCardOptionsRaw;
  let nextTargetLanguageWord: string | null = null;
  if (nextDueCard && (nextDueCard.card_indicator.type === "WrittenPhrase" || nextDueCard.card_indicator.type === "WrittenGram")) {
    nextTargetLanguageWord = nextDueCard?.card_text;
  }

  const numCanAddTargetLanguage =
    addCardOptions.manual_add.find(
      ([, card_type]) => card_type === "TargetLanguage"
    )?.[0] || 0;
  const numCanAddListening =
    addCardOptions.manual_add.find(
      ([, card_type]) => card_type === "Listening"
    )?.[0] || 0;
  const numCanAddLetterPronunciation =
    addCardOptions.manual_add.find(
      ([, card_type]) => card_type === "LetterPronunciation"
    )?.[0] || 0;
  const numCanSmartAdd = addCardOptions.smart_add;

  // Calculate if workload looks light
  const pastWeekAverage = deck.get_past_week_challenge_average();
  const upcomingStats = deck.get_upcoming_week_review_stats();
  const cardsAddedPast16Hours = deck.get_cards_added_in_past_hours(16);
  const showLightWorkloadNotification =
    cardsAddedPast16Hours < 20 &&
    (upcomingStats.total_reviews < pastWeekAverage * 21 ||
      upcomingStats.max_per_day < 10) &&
    upcomingStats.max_per_day <= 50 &&
    (numCanSmartAdd > 0 ||
      numCanAddTargetLanguage > 0 ||
      numCanAddListening > 0 ||
      numCanAddLetterPronunciation > 0);

  const add_cards: [number, CardType | undefined][] = [];
  if (numCanSmartAdd > 0) {
    add_cards.push([numCanSmartAdd, undefined]);
  }
  if (numCanAddTargetLanguage > 0) {
    add_cards.push([numCanAddTargetLanguage, "TargetLanguage"]);
  }
  if (numCanAddListening > 0) {
    add_cards.push([numCanAddListening, "Listening"]);
  }
  if (numCanAddLetterPronunciation > 0) {
    add_cards.push([numCanAddLetterPronunciation, "LetterPronunciation"]);
  }

  const targetLanguageSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} → English</span>
  );
  const listeningSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} listening</span>
  );
  const pronunciationSpan = (
    <span style={{ fontWeight: "bold" }}>{targetLanguage} pronunciation</span>
  );

  const isEmptyDeck = deck.num_cards() === 0;

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
        add_cards.length > 0
      ) {
        event.preventDefault();
        addNextCards(add_cards[0][1], add_cards[0][0]);
      }
    };

    window.addEventListener("keydown", handleKeyPress);

    return () => {
      window.removeEventListener("keydown", handleKeyPress);
    };
  }, [addNextCards, add_cards]);

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
  const effectiveGoal: Goal = currentCategoryIndex === -1 ? { type: "essential" } : goal;

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
    const nextIndex = direction === "left" ? effectiveIndex - 1 : effectiveIndex + 1;
    if (nextIndex >= 0 && nextIndex < categories.length) {
      setGoal(goalForCategory(categories[nextIndex]));
    }
  };

  // Goal progress info
  const tierInfo = deck.get_current_tier();
  const { goalPercentKnown, goalDone } = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return { goalPercentKnown: tierInfo.percent_known, goalDone: false };
      case "movie": {
        const movie = moviesWithMetadata.find(m => m.id === effectiveGoal.movieId);
        return { goalPercentKnown: movie?.percent_known ?? 0, goalDone: movie?.all_available_learned ?? false };
      }
      case "pimsleur": {
        const stats = deck.get_pimsleur_stats();
        const lesson = stats.find(l => l.level === effectiveGoal.level && l.lesson === effectiveGoal.lesson);
        return { goalPercentKnown: lesson?.percent_known ?? 0, goalDone: lesson?.all_available_learned ?? false };
      }
    }
  })();

  const goalLabel = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return `${tierInfo.name} ${targetLanguage} Level ${tierInfo.level}`;
      case "movie":
        return moviesWithMetadata.find(m => m.id === effectiveGoal.movieId)?.title ?? "Movie";
      case "pimsleur":
        return `Pimsleur Level ${effectiveGoal.level}, Lesson ${effectiveGoal.lesson}`;
    }
  })();

  // Check if adding cards would cross a 5% threshold
  const currentRounded = Math.floor(goalPercentKnown / 5) * 5;
  const afterRounded = Math.floor(addCardOptions.percent_known_after / 5) * 5;
  const crossesThreshold = afterRounded > currentRounded;
  const thresholdTarget = crossesThreshold ? afterRounded : null;

  const goalImageUrl = (() => {
    switch (effectiveGoal.type) {
      case "essential":
        return "/essential-course.webp";
      case "movie": {
        const movie = moviesWithMetadata.find(m => m.id === effectiveGoal.movieId);
        return movie ? getPosterDataUrl(movie.poster_bytes) : null;
      }
      case "pimsleur":
        return null;
    }
  })();

  return (
    <div className="space-y-4">
      <div className="text-center py-4">
        <div className="flex flex-col gap-2">
          <p className="text-2xl font-bold">
            {isEmptyDeck
              ? "Ready to start learning?"
              : "All caught up!"}
          </p>
          {isEmptyDeck ? (
            <p className="text-muted-foreground">
              We'll start with a couple words you might know.
            </p>
          ) : (
            <p className="text-muted-foreground">
              {nextTargetLanguageWord ? (
                <>
                  You'll review{" "}
                  <span className="font-semibold">
                    {nextTargetLanguageWord}
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

      {isEmptyDeck ? (
        <div className="flex justify-center">
          <Button
            onClick={() => addNextCards(undefined, add_cards[0]?.[0] ?? 1)}
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
      <Card className="overflow-hidden py-4 gap-2" animate>
        <p className="text-lg font-semibold px-4 sm:px-8 text-center">
          {goalDone
            ? <>You're all done with<br /><span className="uppercase font-bold">{goalLabel}!</span></>
            : showLightWorkloadNotification && thresholdTarget !== null
            ? <>Soon you'll hit {thresholdTarget}% on<br /><span className="uppercase font-bold">{goalLabel}!</span></>
            : showLightWorkloadNotification
            ? <>Keep up the momentum on<br /><span className="uppercase font-bold">{goalLabel}!</span></>
            : <>You're doing great on<br /><span className="uppercase font-bold">{goalLabel}!</span></>}
        </p>
        <div className="flex items-center justify-between gap-0">
          <button
            onClick={() => navigateGoal("left")}
            className={`p-2 self-stretch transition-colors ${canGoLeft ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
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
                  Yap has word lists for Pimsleur, but is not affiliated with Pimsleur in any way.
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
                  {goalImageUrl ? (
                    <img
                      src={goalImageUrl}
                      alt={goalLabel}
                      className={`w-full h-full object-cover opacity-90 saturate-70 dark:opacity-70 dark:saturate-80 hover:opacity-100 hover:saturate-100 transition-all ${effectiveGoal.type === "essential" ? "dark:invert dark:hue-rotate-180" : ""}`}
                    />
                  ) : (
                    <div className="w-full h-full bg-muted flex items-center justify-center">
                      <Headphones className="h-8 w-8 text-muted-foreground" />
                    </div>
                  )}
                </div>
                <div className="order-1 sm:order-last flex-1 flex flex-col items-center sm:items-start gap-3 min-w-0 w-full sm:w-auto">
                  {goalDone ? (() => {
                    // Show "next lesson" / "next movie" button when goal is complete
                    const nextGoal = (() => {
                      switch (effectiveGoal.type) {
                        case "pimsleur": {
                          const best = deck.get_best_pimsleur_goal();
                          if (best && best.type === "PimsleurLesson") {
                            return { goal: { type: "pimsleur" as const, level: best.level, lesson: best.lesson }, label: "Next lesson" };
                          }
                          return null;
                        }
                        case "movie": {
                          const best = deck.get_best_movie_goal();
                          if (best && best.type === "Movie") {
                            return { goal: { type: "movie" as const, movieId: best.id }, label: "Next movie" };
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
                  })() : add_cards.length > 0 ? (
                    <div className="flex">
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            onClick={() => addNextCards(add_cards[0][1], add_cards[0][0])}
                            variant="default"
                            size="lg"
                            className={`group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg ${
                              add_cards.length > 1 ? "rounded-r-none" : ""
                            }`}
                          >
                            <span className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent translate-x-[-200%] group-hover:translate-x-[200%] transition-transform duration-1000"></span>
                            <Sparkles className="h-5 w-5 mr-2 animate-pulse" />
                            Learn {add_cards[0][0]} new{" "}
                            {add_cards[0][1] === undefined
                              ? ""
                              : add_cards[0][1] === "TargetLanguage"
                              ? targetLanguageSpan
                              : add_cards[0][1] === "Listening"
                              ? listeningSpan
                              : pronunciationSpan}{" "}
                            {add_cards[0][0] === 1 ? "card" : "cards"}
                            {thresholdTarget !== null && !showLightWorkloadNotification && <> to hit {thresholdTarget}%</>}
                          </Button>
                        </TooltipTrigger>
                        {addCardOptions.preview.length > 0 && (
                          <TooltipContent>
                            {addCardOptions.preview.join(", ")}
                          </TooltipContent>
                        )}
                      </Tooltip>
                      {add_cards.length > 1 && (
                        <DropdownMenu>
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
                            {add_cards.slice(1).map(([count, card_type]) => (
                              <DropdownMenuItem
                                key={card_type || "smart"}
                                onClick={() => addNextCards(card_type, count)}
                                className="cursor-pointer"
                              >
                                <Sparkles className="h-4 w-4 mr-2" />
                                Learn {count}{" "}
                                {card_type === "TargetLanguage"
                                  ? targetLanguageSpan
                                  : card_type === "Listening"
                                  ? listeningSpan
                                  : card_type === "LetterPronunciation"
                                  ? pronunciationSpan
                                  : ""}{" "}
                                {count === 1 ? "card" : "cards"}
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      )}
                    </div>
                  ) : (
                    <p className="text-sm">
                      You've learned all available words!
                    </p>
                  )}

                  <Progress
                    value={goalPercentKnown}
                    showPercentage
                    label={goalDone ? "Done!" : undefined}
                    className="h-6"
                  />

                  <button
                    onClick={() => navigate("/goals")}
                    className="text-xs text-foreground/60 hover:text-foreground underline underline-offset-2 transition-colors text-left"
                  >
                    change goal
                  </button>
                </div>
              </>
            )}
          </div>

          <button
            onClick={() => navigateGoal("right")}
            className={`p-2 self-stretch transition-colors ${canGoRight ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
            disabled={!canGoRight}
            aria-label="Next goal"
          >
            <ChevronRight className="h-6 w-6" />
          </button>
        </div>
      </Card>
      )}

      {showEngagementPrompts && <EngagementPrompts language={targetLanguage} />}
    </div>
  );
}
