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
  LoaderCircle,
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
import { TargetLanguageText } from "./TargetLanguageText";
import { ReviewPlanCard } from "./LockupOffer";
import { WeekProgressStrip } from "./WeekProgressStrip";
import { sentenceListToSelection, type SentenceList } from "@/hooks/useSentenceList";
import { useNavigate } from "react-router-dom";

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
  /// Due cards held back because their audio hasn't downloaded yet — they'll
  /// appear as challenges once the background prefetcher catches up.
  audioPendingCount: number;
  userInfo: UserInfo | undefined;
  sentenceList: SentenceList;
  setSentenceList: (sentenceList: SentenceList) => void;
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
  audioPendingCount,
  userInfo,
  sentenceList,
  setSentenceList,
  moviesWithMetadata,
  hasPimsleur,
}: NoCardsReadyProps) {
  const navigate = useNavigate();
  const [pimsleurAcknowledged, setPimsleurAcknowledged] = useState(
    () => localStorage.getItem("yap-pimsleur-acknowledged") === "true",
  );
  const sentenceListSelection = sentenceListToSelection(sentenceList);

  // One memoized call that does the expensive next_unknown_cards computation
  const info = useMemo(
    () => deck.get_no_cards_ready_info(bannedChallengeTypes, sentenceListSelection),
    [deck, bannedChallengeTypes, sentenceListSelection],
  );

  // Manual add options — computed lazily on dropdown open
  const [manualAddOptions, setManualAddOptions] = useState<ManualAddOption[]>([]);
  const loadManualAddOptions = useCallback(() => {
    if (manualAddOptions.length > 0) return;
    const types: CardType[] = ["TargetLanguage", "Listening", "LetterPronunciation"];
    const options = types
      .map((t) => deck.get_manual_add_option(t, sentenceListSelection))
      .filter((o) => userInfo !== undefined || o.card_type === "TargetLanguage" || o.card_type === "LetterPronunciation")
      .filter((o) => userInfo !== undefined || o.count > 0);
    setManualAddOptions(options);
  }, [deck, sentenceListSelection, userInfo, manualAddOptions.length]);

  const addSmartCards = useCallback(() => {
    if (info.smart_add_event) {
      addEvent(info.smart_add_event);
    }
  }, [info.smart_add_event, addEvent]);

  // While any locked cards are DUE, adding new cards is suppressed and
  // replaced by releasing the next batch from lockup. Locked cards scheduled
  // for the future don't trigger this — they're morally just future cards.
  // nextDueCard is a fresh object every parent render, so this re-evaluates
  // as time passes (e.g. when a locked card comes due).
  const releaseOffer = useMemo(
    // eslint-disable-next-line react-hooks/purity -- point-in-time check; re-evaluated as the parent re-renders
    () => deck.get_release_offer(Date.now()),
    [deck, nextDueCard], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const releaseLockedCards = useCallback(() => {
    if (releaseOffer) {
      addEvent(releaseOffer.unlock_event);
    }
  }, [releaseOffer, addEvent]);
  // Two-phase release: after reviewing today, first show a "Nice job!" rest
  // screen; the review plan only appears once the user asks for more
  const [showReleasePlan, setShowReleasePlan] = useState(false);
  const releasePlanShown =
    releaseOffer !== undefined &&
    (deck.get_today_time_spent() === 0 || showReleasePlan);
  const nextDueSoon = useMemo(
    () =>
      nextDueCard !== null &&
      // eslint-disable-next-line react-hooks/purity -- point-in-time check; parent re-renders every minute
      nextDueCard.due_timestamp_ms - Date.now() < 30 * 60 * 1000,
    [nextDueCard],
  );

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
  // No more cards to schedule AND nothing left to add — we've truly run out.
  const nothingToDo = noSchedulableCards && info.smart_add_count === 0;
  // The user has never explicitly added a card (placement-test-only counts).
  const hasNeverStudied = deck.num_cards_added() === 0 && !nothingToDo;

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

      if (event.code === "Space" || event.code === "Enter") {
        if (releaseOffer) {
          event.preventDefault();
          if (releasePlanShown) {
            releaseLockedCards();
          } else {
            setShowReleasePlan(true);
          }
        } else if (info.smart_add_event) {
          event.preventDefault();
          addSmartCards();
        }
      }
    };

    window.addEventListener("keydown", handleKeyPress);

    return () => {
      window.removeEventListener("keydown", handleKeyPress);
    };
  }, [
    addSmartCards,
    info.smart_add_event,
    releaseOffer,
    releaseLockedCards,
    releasePlanShown,
  ]);

  // Sentence list navigation — 3 tabs: essential, movie, pimsleur
  // Each tab auto-picks the best specific sentence list within that category
  type SentenceListCategory = "essential" | "movie" | "pimsleur";
  const categories: SentenceListCategory[] = [
    "essential",
    ...(moviesWithMetadata.length > 0 ? ["movie" as const] : []),
    ...(hasPimsleur ? ["pimsleur" as const] : []),
  ];

  const sentenceListCategory = (sl: SentenceList): SentenceListCategory => sl.type;

  const currentCategory = sentenceListCategory(sentenceList);
  const currentCategoryIndex = categories.indexOf(currentCategory);
  // If category not found (e.g. no movies available but sentenceList is movie), fall back to essential
  const effectiveIndex = currentCategoryIndex === -1 ? 0 : currentCategoryIndex;
  const effectiveSentenceList: SentenceList =
    currentCategoryIndex === -1 ? { type: "essential" } : sentenceList;

  const canGoLeft = effectiveIndex > 0;
  const canGoRight = effectiveIndex < categories.length - 1;

  const sentenceListForCategory = (category: SentenceListCategory): SentenceList => {
    switch (category) {
      case "essential":
        return { type: "essential" };
      case "movie": {
        const best = deck.get_best_movie_sentence_list();
        if (best && best.type === "Movie") {
          return { type: "movie", movieId: best.id };
        }
        // Fallback to first movie
        return { type: "movie", movieId: moviesWithMetadata[0].id };
      }
      case "pimsleur": {
        const best = deck.get_best_pimsleur_sentence_list();
        if (best && best.type === "PimsleurLesson") {
          return { type: "pimsleur", level: best.level, lesson: best.lesson };
        }
        return { type: "essential" };
      }
    }
  };

  const navigateSentenceList = (direction: "left" | "right") => {
    const nextIndex =
      direction === "left" ? effectiveIndex - 1 : effectiveIndex + 1;
    if (nextIndex >= 0 && nextIndex < categories.length) {
      setSentenceList(sentenceListForCategory(categories[nextIndex]));
    }
  };

  // Sentence list progress info
  const tierInfo = info.tier_info;
  const { sentenceListPercentKnown, sentenceListDone } = (() => {
    switch (effectiveSentenceList.type) {
      case "essential":
        return { sentenceListPercentKnown: tierInfo.percent_known, sentenceListDone: false };
      case "movie": {
        const movie = moviesWithMetadata.find(
          (m) => m.id === effectiveSentenceList.movieId,
        );
        return {
          sentenceListPercentKnown: movie?.percent_known ?? 0,
          sentenceListDone: movie?.all_available_learned ?? false,
        };
      }
      case "pimsleur": {
        const stats = deck.get_pimsleur_stats();
        const lesson = stats.find(
          (l) =>
            l.level === effectiveSentenceList.level &&
            l.lesson === effectiveSentenceList.lesson,
        );
        return {
          sentenceListPercentKnown: lesson?.percent_known ?? 0,
          sentenceListDone: lesson?.all_available_learned ?? false,
        };
      }
    }
  })();

  const sentenceListLabel = (() => {
    switch (effectiveSentenceList.type) {
      case "essential":
        return `${tierInfo.name} ${targetLanguage} Level ${tierInfo.level}`;
      case "movie":
        return (
          moviesWithMetadata.find((m) => m.id === effectiveSentenceList.movieId)
            ?.title ?? "Movie"
        );
      case "pimsleur":
        return `Pimsleur Level ${effectiveSentenceList.level}, Lesson ${effectiveSentenceList.lesson}`;
    }
  })();

  const currentRounded = Math.floor(sentenceListPercentKnown / 5) * 5;
  const afterRounded = Math.floor(info.percent_known_after / 5) * 5;
  const crossesThreshold = afterRounded > currentRounded;
  const thresholdTarget = crossesThreshold ? afterRounded : null;

  const sentenceListImage = (() => {
    switch (effectiveSentenceList.type) {
      case "essential":
        return { type: "url" as const, url: "/essential-course.webp" };
      case "movie":
        return { type: "movie" as const, movieId: effectiveSentenceList.movieId };
      case "pimsleur":
        return null;
    }
  })();

  // Due challenges exist but their audio hasn't downloaded yet (the review
  // screen only offers challenges the user can actually complete). This
  // normally resolves within seconds as the background prefetcher lands
  // clips — but it can persist offline, which is why it explains itself
  // rather than showing a bare spinner.
  if (audioPendingCount > 0) {
    return (
      <div className="flex flex-col flex-1 gap-4 pt-4">
        <div className="flex flex-col gap-2 text-center">
          <p className="text-2xl font-bold">Just a moment…</p>
          <p className="text-muted-foreground">
            Downloading the audio for your next challenge.
          </p>
        </div>
        <div className="flex justify-center py-4">
          <LoaderCircle className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
        <WeekProgressStrip deck={deck} className="mt-auto mb-2" />
      </div>
    );
  }

  // While cards remain set aside in lockup, the whole add-cards area is
  // replaced by releasing the next batch. Never says "all caught up" — that's
  // only true once nothing is locked. Two phases: a rest screen after today's
  // reviews, then the same review-plan page as the lockup offer.
  if (releaseOffer) {
    const minutesToday = Math.round(deck.get_today_time_spent() / 60);
    if (releasePlanShown) {
      return (
        <ReviewPlanCard
          title="Today's review plan:"
          cards={releaseOffer.release_preview}
          buttonLabel="Let's go!"
          onCommit={releaseLockedCards}
          deck={deck}
          targetLanguage={targetLanguage}
        />
      );
    }

    return (
      <div className="flex flex-col flex-1 gap-4 pt-4">
        <div className="flex flex-col gap-2 text-center">
          <p className="text-2xl font-bold">
            Nice! You reviewed for{" "}
            {minutesToday < 1
              ? "less than a minute"
              : `${minutesToday} ${minutesToday === 1 ? "minute" : "minutes"}`}{" "}
            today!
          </p>
          <p className="text-muted-foreground">
            You can take a break, or review more.
          </p>
          {nextDueSoon && (
            <NextReviewLine
              nextDueCard={nextDueCard}
              targetLanguage={targetLanguage}
            />
          )}
        </div>
        <div className="flex justify-center">
          <Button
            onClick={() => setShowReleasePlan(true)}
            size="lg"
            variant="outline"
          >
            Review more
          </Button>
        </div>
        <WeekProgressStrip deck={deck} className="mt-auto mb-2" />
      </div>
    );
  }

  return (
    <div className="flex flex-col flex-1 gap-4">
      <div className="text-center py-4">
        <div className="flex flex-col gap-2">
          <p className="text-2xl font-bold">
            {nothingToDo
              ? "All done!"
              : hasNeverStudied
                ? "Ready to start learning?"
                : noSchedulableCards
                  ? info.smart_add_regime === "Easy"
                    ? "Adding cards is how you learn more!"
                    : "Ready for more?"
                  : "All caught up!"}
          </p>
          {nothingToDo ? (
            <p className="text-muted-foreground">
              You've learned all available words!
            </p>
          ) : hasNeverStudied ? (
            <p className="text-muted-foreground">
              We'll start with a couple words you might know.
            </p>
          ) : noSchedulableCards ? (
            <p className="text-muted-foreground">
              {info.smart_add_regime === "Easy" && info.easy_cards_remaining > 0
                ? `${info.easy_cards_remaining} more easy ${info.easy_cards_remaining === 1 ? "word" : "words"}, then we'll add harder ones.`
                : "Add some cards to keep building your vocabulary."}
            </p>
          ) : (
            <NextReviewLine
              nextDueCard={nextDueCard}
              targetLanguage={targetLanguage}
            />
          )}
        </div>
      </div>

      {nothingToDo ? null : noSchedulableCards ? (
        <div className="flex justify-center">
          <Button
            onClick={addSmartCards}
            variant="default"
            size="lg"
            className="group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg"
          >
            <span className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent translate-x-[-200%] group-hover:translate-x-[200%] transition-transform duration-1000"></span>
            <Sparkles className="h-5 w-5 mr-2 animate-pulse" />
            {hasNeverStudied
              ? "Start learning"
              : `Add ${info.smart_add_count} ${info.smart_add_regime === "Easy" ? "easy " : ""}${info.smart_add_count === 1 ? "card" : "cards"}`}
          </Button>
        </div>
      ) : (
        <Card className="overflow-hidden px-2 py-4 gap-2" animate>
          <p className="text-lg font-semibold px-4 sm:px-8 text-center">
            {sentenceListDone ? (
              <>
                You're all done with
                <br />
                <span className="uppercase font-bold">{sentenceListLabel}!</span>
              </>
            ) : showLightWorkloadNotification && thresholdTarget !== null ? (
              <>
                Soon you'll hit {thresholdTarget}% on
                <br />
                <span className="uppercase font-bold">{sentenceListLabel}!</span>
              </>
            ) : showLightWorkloadNotification ? (
              <>
                Keep up the momentum on
                <br />
                <span className="uppercase font-bold">{sentenceListLabel}!</span>
              </>
            ) : (
              <>
                You're doing great on
                <br />
                <span className="uppercase font-bold">{sentenceListLabel}!</span>
              </>
            )}
          </p>
          <div className="flex items-center justify-between gap-0">
            <button
              onClick={() => navigateSentenceList("left")}
              className={`hidden sm:flex p-2 self-stretch items-center transition-colors ${canGoLeft ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
              disabled={!canGoLeft}
              aria-label="Previous sentence list"
            >
              <ChevronLeft className="h-6 w-6" />
            </button>

            <div className="flex-1 flex flex-col sm:flex-row items-center gap-4">
              {effectiveSentenceList.type === "pimsleur" && !pimsleurAcknowledged ? (
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
                    onClick={() => navigate("/sentence-lists")}
                    className="hidden sm:block sm:order-first w-24 h-36 flex-shrink-0 rounded-lg border border-border/50 overflow-hidden cursor-pointer hover:scale-105 transition-all"
                  >
                    {sentenceListImage?.type === "url" ? (
                      <img
                        src={sentenceListImage.url}
                        alt={sentenceListLabel}
                        className={`w-full h-full object-cover opacity-90 saturate-70 dark:opacity-70 dark:saturate-80 hover:opacity-100 hover:saturate-100 transition-all ${effectiveSentenceList.type === "essential" ? "dark:invert dark:hue-rotate-180" : ""}`}
                      />
                    ) : sentenceListImage?.type === "movie" ? (
                      <Poster
                        movieId={sentenceListImage.movieId}
                        deck={deck}
                        alt={sentenceListLabel}
                      />
                    ) : (
                      <div className="w-full h-full bg-muted flex items-center justify-center">
                        <Headphones className="h-8 w-8 text-muted-foreground" />
                      </div>
                    )}
                  </div>
                  <div className="order-1 sm:order-last flex-1 flex flex-col items-center sm:items-start gap-3 min-w-0 w-full sm:w-auto">
                    {sentenceListDone ? (
                      (() => {
                        // Show "next lesson" / "next movie" button when sentence list is complete
                        const nextSentenceList = (() => {
                          switch (effectiveSentenceList.type) {
                            case "pimsleur": {
                              const best = deck.get_best_pimsleur_sentence_list();
                              if (best && best.type === "PimsleurLesson") {
                                return {
                                  sentenceList: {
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
                              const best = deck.get_best_movie_sentence_list();
                              if (best && best.type === "Movie") {
                                return {
                                  sentenceList: {
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

                        return nextSentenceList ? (
                          <Button
                            onClick={() => setSentenceList(nextSentenceList.sentenceList)}
                            variant="default"
                            size="lg"
                            className="group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg"
                          >
                            <ChevronRight className="h-5 w-5 mr-2" />
                            {nextSentenceList.label}
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
                      value={sentenceListPercentKnown}
                      projectedValue={
                        sentenceListDone
                          ? undefined
                          : info.percent_known_after
                      }
                      showPercentage
                      label={sentenceListDone ? "Done!" : undefined}
                      className="h-6"
                    />

                    {effectiveSentenceList.type === "essential" && (
                      <p className="text-xs text-muted-foreground text-center sm:text-left">
                        When you complete this level, you'll understand{" "}
                        {tierInfo.percent_of_usage.toFixed(1)}% of everyday{" "}
                        {targetLanguage}.
                      </p>
                    )}

                    <button
                      onClick={() => navigate("/sentence-lists")}
                      className="text-xs text-foreground/60 hover:text-foreground underline underline-offset-2 transition-colors text-left"
                    >
                      change sentence list
                    </button>

                    {categories.length > 1 && (
                      <div className="flex sm:hidden items-center justify-between w-full">
                        <button
                          onClick={() => navigateSentenceList("left")}
                          className={`p-2 transition-colors ${canGoLeft ? "text-foreground/60 hover:text-foreground" : "text-transparent cursor-default"}`}
                          disabled={!canGoLeft}
                          aria-label="Previous sentence list"
                        >
                          <ChevronLeft className="h-6 w-6" />
                        </button>
                        <button
                          onClick={() => navigateSentenceList("right")}
                          className={`p-2 transition-colors ${canGoRight ? "text-foreground/60 hover:text-foreground" : "text-transparent cursor-default"}`}
                          disabled={!canGoRight}
                          aria-label="Next sentence list"
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
              onClick={() => navigateSentenceList("right")}
              className={`hidden sm:flex p-2 self-stretch items-center transition-colors ${canGoRight ? "text-foreground/60 hover:text-foreground hover:bg-muted/50" : "text-transparent cursor-default"}`}
              disabled={!canGoRight}
              aria-label="Next sentence list"
            >
              <ChevronRight className="h-6 w-6" />
            </button>
          </div>
        </Card>
      )}

      {showEngagementPrompts && <EngagementPrompts language={targetLanguage} />}

      {!noSchedulableCards && (
        <WeekProgressStrip deck={deck} className="mt-auto mb-2" />
      )}
    </div>
  );
});

/// "You'll review <word> in 2 minutes." / "Your next review is soon."
function NextReviewLine({
  nextDueCard,
  targetLanguage,
}: {
  nextDueCard: CardSummary | null;
  targetLanguage: Language;
}) {
  let nextTargetLanguageWord: string | null = null;
  if (
    nextDueCard &&
    (nextDueCard.card_indicator.type === "WrittenPhrase" ||
      nextDueCard.card_indicator.type === "WrittenGram")
  ) {
    nextTargetLanguageWord = nextDueCard.card_text;
  }

  return (
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
  );
}

