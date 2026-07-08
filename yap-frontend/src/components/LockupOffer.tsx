import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type {
  CardSummary,
  Deck,
  DeckEvent,
  Language,
  LockupOffer,
} from "../../../yap-frontend-rs/pkg";
import { TargetLanguageText } from "./TargetLanguageText";
import { WeekProgressStrip } from "./WeekProgressStrip";

const CARD_GROUPS = [
  { type: "WrittenGram", label: "reading" },
  { type: "ListeningGram", label: "listening" },
  { type: "LetterPronunciation", label: "pronunciation" },
] as const;

interface ReviewPlanCardProps {
  title: string;
  cards: CardSummary[];
  buttonLabel: string;
  onCommit: () => void;
  deck: Deck;
  targetLanguage: Language;
}

/// Shared layout for committing to a set of reviews: the lockup offer
/// ("Today's review plan") and releasing more cards from lockup. Shows the
/// cards grouped by type above a single commit button.
export function ReviewPlanCard({
  title,
  cards,
  buttonLabel,
  onCommit,
  deck,
  targetLanguage,
}: ReviewPlanCardProps) {
  const byType = new Map<string, CardSummary[]>();
  for (const card of cards) {
    const type = card.card_indicator.type;
    byType.set(type, [...(byType.get(type) ?? []), card]);
  }

  return (
    <div className="space-y-4">
      <Card className="max-w w-full p-6 gap-6 select-none" animate>
        <h2 className="text-xl font-semibold text-center">{title}</h2>

        {CARD_GROUPS.map(({ type, label }) => {
          const group = byType.get(type);
          if (!group || group.length === 0) return null;
          return (
            <div key={type} className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground text-center">
                {group.length} {label} {group.length === 1 ? "card" : "cards"}
              </p>
              <div className="flex flex-wrap justify-center gap-y-1">
                {group.map((card, i) => (
                  <span
                    key={i}
                    className={`px-3 text-sm font-medium ${i > 0 ? "border-l border-border" : ""}`}
                  >
                    <TargetLanguageText language={targetLanguage}>
                      {card.card_text}
                    </TargetLanguageText>
                  </span>
                ))}
              </div>
            </div>
          );
        })}

        <Button onClick={onCommit} size="lg" className="w-full">
          {buttonLabel}
        </Button>
      </Card>

      <WeekProgressStrip deck={deck} />
    </div>
  );
}

interface LockupOfferScreenProps {
  offer: LockupOffer;
  deck: Deck;
  targetLanguage: Language;
  onAccept: (event: DeckEvent) => void;
}

/// Session-start screen shown when a review backlog builds up: keeps the most
/// due cards active and sets the rest aside ("lockup").
export function LockupOfferScreen({
  offer,
  deck,
  targetLanguage,
  onAccept,
}: LockupOfferScreenProps) {
  // wasm getters clone on every access, so read them once
  const preview = offer.keep_preview;
  const lockEvent = offer.lock_event;

  return (
    <ReviewPlanCard
      title="Today's review plan:"
      cards={preview}
      buttonLabel="Let's go!"
      onCommit={() => onAccept(lockEvent)}
      deck={deck}
      targetLanguage={targetLanguage}
    />
  );
}
