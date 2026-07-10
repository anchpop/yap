import { memo, useState, lazy, Suspense, useDeferredValue, useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import TimeAgo from "react-timeago";
import type { Deck, Language } from "../../../yap-frontend-rs/pkg";
import { TargetLanguageText } from "./TargetLanguageText";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useInterval } from "react-use";
import { NumericStats } from "./numeric-stats";
import { Card } from "@/components/ui/card";

// Lazy load the chart component - only loads when needed
const FrequencyKnowledgeChart = lazy(() =>
  import("./FrequencyKnowledgeChart").then((module) => ({
    default: module.FrequencyKnowledgeChart,
  })),
);

interface StatsProps {
  deck: Deck;
  targetLanguage: Language;
}

export const Stats = memo(function Stats({ deck: deckProp, targetLanguage }: StatsProps) {
  const deck = useDeferredValue(deckProp);
  const [currentTimestamp, setCurrentTimestamp] = useState(() => Date.now());

  // Update timestamp periodically to keep stats fresh
  useInterval(
    () => {
      setCurrentTimestamp(Date.now());
    },
    10000, // Update every 10 seconds
  );

  // Deliberately lockup-agnostic: locked cards are still cards in the deck,
  // so they count as ready/not-ready like any other
  const { readyCards, allCardsSummary } = useMemo(() => {
    const allCardsSummary = deck.get_all_cards_summary();
    const readyCards = allCardsSummary.filter(
      (card) => card.due_timestamp_ms <= currentTimestamp,
    );
    return { readyCards, allCardsSummary };
  }, [deck, currentTimestamp]);
  const notReadyCards = allCardsSummary.filter(
    (card) => card.due_timestamp_ms > currentTimestamp,
  );

  const [visibleCount, setVisibleCount] = useState(10);
  const [nextBatchSize, setNextBatchSize] = useState(10);
  const allCards = [...readyCards, ...notReadyCards];
  const visibleCards = allCards.slice(0, visibleCount);

  const [revealedListeningCards, setRevealedListeningCards] = useState<
    Set<string>
  >(() => new Set());

  const handleRevealListeningCard = (key: string) => {
    setRevealedListeningCards((prev) => {
      if (prev.has(key)) {
        return prev;
      }

      const next = new Set(prev);
      next.add(key);
      return next;
    });
  };

  const [isGraphsOpen, setIsGraphsOpen] = useState(false);

  return (
    <div className="mt-4">
      <NumericStats
        xp={deck.get_xp()}
        totalCards={allCardsSummary.length}
        cardsReady={readyCards.length}
        percentKnown={deck.get_percent_of_words_known() * 100}
        dailyStreak={deck.get_daily_streak()}
        totalReviews={deck.get_total_reviews()}
        targetLanguage={targetLanguage}
        todayReviews={deck.get_today_reviews()}
        todayTimeSpent={deck.get_today_time_spent()}
        dailyTargetSeconds={deck.get_daily_review_target()}
      />
      <Card className="overflow-hidden p-0 gap-0" animate>
        <table className="w-full table-fixed">
          <thead>
            <tr className="border-b bg-muted/50">
              <th className="text-left p-3 font-medium w-1/4">Word</th>
              <th className="text-left p-3 font-medium w-1/4">State</th>
              <th className="text-left p-3 font-medium w-1/2">Ready</th>
            </tr>
          </thead>
          <tbody>
            {(() => {
              // Find card_text values that appear more than once
              const textCounts = new Map<string, number>();
              for (const card of visibleCards) {
                textCounts.set(
                  card.card_text,
                  (textCounts.get(card.card_text) || 0) + 1,
                );
              }
              const duplicateTexts = new Set(
                [...textCounts.entries()]
                  .filter(([, count]) => count > 1)
                  .map(([text]) => text),
              );

              return visibleCards.map((card, index) => {
                const shortDescription = card.card_text;
                const subtitle = card.card_subtitle;
                const showSubtitle =
                  subtitle && duplicateTexts.has(shortDescription);

                const isListeningGram =
                  card.card_indicator.type === "ListeningGram";
                const listeningCardKey = isListeningGram
                  ? JSON.stringify(card.card_indicator)
                  : null;

                const isReady = card.due_timestamp_ms <= currentTimestamp;
                const isListeningCardRevealed = listeningCardKey
                  ? revealedListeningCards.has(listeningCardKey)
                  : false;

                const wordCellContent = isListeningGram ? (
                  isListeningCardRevealed ? (
                    <TargetLanguageText language={targetLanguage}>
                      {shortDescription}
                    </TargetLanguageText>
                  ) : (
                    <button
                      type="button"
                      onClick={() =>
                        listeningCardKey &&
                        handleRevealListeningCard(listeningCardKey)
                      }
                      className="inline-flex items-center gap-2 rounded-sm bg-transparent p-0 text-left text-base font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                      aria-label="Reveal listening lexeme"
                    >
                      <span className="select-none blur-sm">
                        <TargetLanguageText language={targetLanguage}>
                          {shortDescription}
                        </TargetLanguageText>
                      </span>
                      <span className="text-xs italic text-muted-foreground">
                        Tap to reveal
                      </span>
                    </button>
                  )
                ) : (
                  <TargetLanguageText language={targetLanguage}>
                    {shortDescription}
                  </TargetLanguageText>
                );
                return (
                  <tr
                    key={index}
                    className={`border-b ${isReady ? "bg-green-500/10" : ""}`}
                  >
                    <td className="p-3 font-medium">
                      {wordCellContent}
                      {showSubtitle && (
                        <span className="ml-2 text-muted-foreground text-sm">
                          ({subtitle})
                        </span>
                      )}
                    </td>
                    <td className="p-3">
                      <Badge variant="outline">{card.state}</Badge>
                    </td>
                    <td className="p-3 text-sm text-muted-foreground">
                      {isReady ? (
                        <Badge className="bg-green-500/20 text-green-600 dark:text-green-400 border-green-500/30">
                          Ready now
                        </Badge>
                      ) : (
                        <TimeAgo date={new Date(card.due_timestamp_ms)} />
                      )}
                    </td>
                  </tr>
                );
              });
            })()}
          </tbody>
        </table>
        {allCards.length > visibleCount && (
          <div className="border-t">
            <button
              onClick={() => {
                setVisibleCount((c) => c + nextBatchSize);
                setNextBatchSize((s) => s * 10);
              }}
              className="w-full py-3 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors duration-200 font-medium"
            >
              Show {Math.min(nextBatchSize, allCards.length - visibleCount)}{" "}
              more cards
            </button>
          </div>
        )}
      </Card>

      {/* Collapsible Graphs Section */}
      <Collapsible
        open={isGraphsOpen}
        onOpenChange={setIsGraphsOpen}
        className="mt-6"
      >
        <CollapsibleTrigger className="flex items-center gap-2 text-lg font-semibold hover:text-muted-foreground transition-colors">
          {isGraphsOpen ? (
            <ChevronDown className="h-5 w-5" />
          ) : (
            <ChevronRight className="h-5 w-5" />
          )}
          Graphs
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-4">
          <Card className="p-4 gap-4">
            <h3 className="text-base font-semibold">
              Pre-existing Knowledge by Word Frequency
            </h3>
            <p className="text-sm text-muted-foreground">
              This is used to help Yap decide which words to teach first. (Yap
              tries to avoid teaching you words you already know!)
            </p>
            <Suspense
              fallback={
                <div className="h-[400px] flex items-center justify-center text-muted-foreground">
                  <p>Loading chart...</p>
                </div>
              }
            >
              <FrequencyKnowledgeChart deck={deck} />
            </Suspense>
          </Card>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
});
