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
import { WeekProgressStrip } from "./no-cards-ready";

interface LockupOfferScreenProps {
  offer: LockupOffer;
  deck: Deck;
  targetLanguage: Language;
  onAccept: (event: DeckEvent) => void;
}

const CARD_GROUPS = [
  { type: "WrittenGram", label: "reading" },
  { type: "ListeningGram", label: "listening" },
  { type: "LetterPronunciation", label: "pronunciation" },
] as const;

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

  const byType = new Map<string, CardSummary[]>();
  for (const card of preview) {
    const type = card.card_indicator.type;
    byType.set(type, [...(byType.get(type) ?? []), card]);
  }

  return (
    <div className="space-y-4">
      <Card className="max-w w-full p-6 gap-4 select-none" animate>
        <h2 className="text-xl font-semibold text-center">
          Let's review these cards today
        </h2>

        {CARD_GROUPS.map(({ type, label }) => {
          const cards = byType.get(type);
          if (!cards || cards.length === 0) return null;
          return (
            <div key={type} className="space-y-2">
              <div className="flex items-center gap-3">
                <div className="flex-1 border-t border-border" />
                <p className="text-sm font-medium text-muted-foreground">
                  {cards.length} {label} {cards.length === 1 ? "card" : "cards"}
                </p>
                <div className="flex-1 border-t border-border" />
              </div>
              <div className="flex flex-wrap justify-center gap-y-1">
                {cards.map((card, i) => (
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

        <Button
          onClick={() => onAccept(lockEvent)}
          size="lg"
          className="w-full"
        >
          Let's go
        </Button>
      </Card>

      <WeekProgressStrip deck={deck} />
    </div>
  );
}
