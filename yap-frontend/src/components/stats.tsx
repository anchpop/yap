import { useState, lazy, Suspense, useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import TimeAgo from "react-timeago";
import type { Deck } from "../../../yap-frontend-rs/pkg";
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
  }))
);

interface StatsProps {
  deck: Deck;
}

export function Stats({ deck }: StatsProps) {
  const [currentTimestamp, setCurrentTimestamp] = useState(() => Date.now());

  // Update timestamp periodically to keep stats fresh
  useInterval(
    () => {
      setCurrentTimestamp(Date.now());
    },
    10000 // Update every 10 seconds
  );

  const { reviewInfo, readyCards, allCardsSummary } = useMemo(() => {
    const reviewInfo = deck.get_review_info([], currentTimestamp);
    const allCardsSummary = deck.get_all_cards_summary();
    const readyCards = allCardsSummary.filter(
      (card) => card.due_timestamp_ms <= currentTimestamp
    );
    return { reviewInfo, readyCards, allCardsSummary };
  }, [deck, currentTimestamp]);
  const notReadyCards = allCardsSummary.filter(
    (card) => card.due_timestamp_ms > currentTimestamp
  );

  const [visibleCount, setVisibleCount] = useState(10);
  const [nextBatchSize, setNextBatchSize] = useState(10);
  const visibleCards = [...readyCards, ...notReadyCards.slice(0, visibleCount)];

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
        cardsReady={reviewInfo.due_count || 0}
        percentKnown={deck.get_percent_of_words_known() * 100}
        dailyStreak={deck.get_daily_streak()}
        totalReviews={deck.get_total_reviews()}
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
            {visibleCards.map((card, index) => {
              let shortDescription = "";
              let pos = "";
              const tags: string[] = [];

              const isListeningLexeme =
                "ListeningLexeme" in card.card_indicator;
              let listeningCardKey: string | null = null;

              if ("TargetLanguage" in card.card_indicator) {
                if ("Heteronym" in card.card_indicator.TargetLanguage.lexeme) {
                  shortDescription =
                    card.card_indicator.TargetLanguage.lexeme.Heteronym.word;
                  pos = card.card_indicator.TargetLanguage.lexeme.Heteronym.pos;
                } else {
                  shortDescription =
                    card.card_indicator.TargetLanguage.lexeme.Multiword;
                }
              } else if ("ListeningHomophonous" in card.card_indicator) {
                shortDescription = `/${card.card_indicator.ListeningHomophonous.pronunciation}/`;
              } else if (isListeningLexeme) {
                if ("Heteronym" in card.card_indicator.ListeningLexeme.lexeme) {
                  shortDescription =
                    card.card_indicator.ListeningLexeme.lexeme.Heteronym.word;
                } else {
                  shortDescription =
                    card.card_indicator.ListeningLexeme.lexeme.Multiword;
                }
                tags.push("listening");
                listeningCardKey = JSON.stringify(card.card_indicator);
              } else if ("LetterPronunciation" in card.card_indicator) {
                shortDescription = `[${card.card_indicator.LetterPronunciation.pattern}]`;
              }

              const isReady = card.due_timestamp_ms <= currentTimestamp;
              const isListeningCardRevealed = listeningCardKey
                ? revealedListeningCards.has(listeningCardKey)
                : false;

              const wordCellContent = isListeningLexeme ? (
                isListeningCardRevealed ? (
                  shortDescription
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
                      {shortDescription}
                    </span>
                    <span className="text-xs italic text-muted-foreground">
                      Tap to reveal
                    </span>
                  </button>
                )
              ) : (
                shortDescription
              );
              return (
                <tr
                  key={index}
                  className={`border-b ${isReady ? "bg-green-500/10" : ""}`}
                >
                  <td className="p-3 font-medium">
                    {wordCellContent}
                    {[pos && pos.toLowerCase(), ...tags]
                      .filter(Boolean)
                      .map((tag, idx) => (
                        <span
                          key={`${shortDescription}-${tag}-${idx}`}
                          className="ml-2 text-muted-foreground text-sm"
                        >
                          ({tag})
                        </span>
                      ))}
                  </td>
                  <td className="p-3">
                    <Badge variant="outline">{card.state}</Badge>
                  </td>
                  <td className="p-3 text-sm text-muted-foreground">
                    {isReady ? (
                      <span className="text-green-500 font-medium">
                        Ready now
                      </span>
                    ) : (
                      <TimeAgo date={new Date(card.due_timestamp_ms)} />
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {notReadyCards.length > visibleCount && (
          <div className="border-t">
            <button
              onClick={() => {
                setVisibleCount((c) => c + nextBatchSize);
                setNextBatchSize((s) => s * 10);
              }}
              className="w-full py-3 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors duration-200 font-medium"
            >
              Show{" "}
              {Math.min(nextBatchSize, notReadyCards.length - visibleCount)}{" "}
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
          <div className="bg-card border rounded-lg p-6">
            <h3 className="text-base font-semibold mb-4">
              Pre-existing Knowledge by Word Frequency
            </h3>
            <p className="text-sm text-muted-foreground mb-4">
              This is used to help Yap decide which words to teach first. (Yap tries to avoid teaching you words you already know!)
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
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
