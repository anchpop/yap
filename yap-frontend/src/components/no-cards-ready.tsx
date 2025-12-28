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
import { ChevronDown, AlertCircle, Sparkles } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Card } from "@/components/ui/card";
import type { UserInfo } from "@/App";
import { AlmostThereMovie } from "@/components/AlmostThereMovie";
import { useEffect } from "react";

interface MovieWithMetadata {
  id: string;
  percent_known: number;
  cards_to_next_milestone: number | null | undefined;
  title?: string;
  year?: number;
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
  closestToMilestone?: MovieWithMetadata;
}

export function NoCardsReady({
  nextDueCard,
  showEngagementPrompts,
  addNextCards,
  targetLanguage,
  deck,
  bannedChallengeTypes,
  userInfo,
  closestToMilestone,
}: NoCardsReadyProps) {
  const addCardOptionsRaw = deck.add_card_options(bannedChallengeTypes);
  const addCardOptions: AddCardOptions = userInfo === undefined
    ? { smart_add: 0, manual_add: addCardOptionsRaw.manual_add.map(([count, card_type]) => [card_type == "TargetLanguage" || card_type == "LetterPronunciation" ? count : 0, card_type] as [number, CardType]) }
    : addCardOptionsRaw;
  let nextTargetLanguageWord: string | null = null;
  if (nextDueCard && "TargetLanguage" in nextDueCard.card_indicator) {
    const lexeme = nextDueCard.card_indicator.TargetLanguage.lexeme;
    nextTargetLanguageWord =
      "Heteronym" in lexeme
        ? lexeme.Heteronym.word
        : lexeme.Multiword;
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
      upcomingStats.max_per_day < 10) && // Less upcoming reviews than past 3 weeks average
    upcomingStats.max_per_day <= 50 && // No single day has more than 50 reviews
    (numCanSmartAdd > 0 ||
      numCanAddTargetLanguage > 0 ||
      numCanAddListening > 0 ||
      numCanAddLetterPronunciation > 0) && // Can add cards
    deck.num_cards() > 40; // has used yap a bit

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

  // Handle empty deck case
  const isEmptyDeck = deck.num_cards() === 0;

  // Keyboard shortcut to add cards
  useEffect(() => {
    const handleKeyPress = (event: KeyboardEvent) => {
      // Don't handle shortcuts if user is typing in an input field
      const target = event.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT') {
        return;
      }

      if ((event.code === 'Space' || event.code === 'Enter') && add_cards.length > 0) {
        event.preventDefault();
        addNextCards(add_cards[0][1], add_cards[0][0]);
      }
    };

    window.addEventListener('keydown', handleKeyPress);

    return () => {
      window.removeEventListener('keydown', handleKeyPress);
    };
  }, [addNextCards, add_cards]);

  return (
    <div className="space-y-4">
        <Card animate className="text-center p-6">
        {showLightWorkloadNotification && (
          <Alert>
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              Your upcoming workload looks a little light. Consider adding new
              cards to maintain your learning momentum!
            </AlertDescription>
          </Alert>
        )}
        <div className="flex flex-col gap-2">
          <p className="text-lg">{isEmptyDeck ? "Ready to start learning?" : "Nothing ready for review!"}</p>
          {isEmptyDeck ? (
            <p className="text-muted-foreground">
              We'll start with the most important words.
            </p>
          ) : (
            <p className="text-muted-foreground">
              {nextTargetLanguageWord ? (
                <>
                  You'll review <span className="font-semibold">
                    {nextTargetLanguageWord}
                  </span>
                  {' '}
                  {nextDueCard ? (
                    <TimeAgo date={new Date(nextDueCard.due_timestamp_ms)} />
                  ) : (
                    "soon"
                  )}
                  .
                </>
              ) : (
                <>
                  Great job! Your next review is{" "}
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

        <div className="space-y-4">
          {add_cards.length > 0 ? (
            <div className="flex justify-center">
              <Button
                onClick={() => addNextCards(add_cards[0][1], add_cards[0][0])}
                variant="default"
                size="lg"
                className={`group relative overflow-hidden transition-all hover:scale-105 hover:shadow-lg ${add_cards.length > 1 ? "rounded-r-none" : ""}`}
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
              </Button>
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
            <p className="text-muted-foreground">
              You've learned all available words! Keep practicing to master
              them.
            </p>
          )}
        </div>
        </Card>

      {closestToMilestone && (
        <AlmostThereMovie movie={closestToMilestone} />
      )}

      {showEngagementPrompts && (
        <EngagementPrompts language={targetLanguage} />
      )}
    </div>
  );
}
