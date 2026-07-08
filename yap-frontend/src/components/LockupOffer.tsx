import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type {
  CardSummary,
  DeckEvent,
  Language,
  LockupOffer,
} from "../../../yap-frontend-rs/pkg";
import { TargetLanguageText } from "./TargetLanguageText";

interface LockupOfferScreenProps {
  offer: LockupOffer;
  targetLanguage: Language;
  onAccept: (event: DeckEvent) => void;
}

const CARD_GROUPS = [
  { type: "WrittenGram", label: "reading", showSubtitle: true },
  { type: "ListeningGram", label: "listening", showSubtitle: false },
  { type: "LetterPronunciation", label: "pronunciation", showSubtitle: false },
] as const;

/// Session-start screen shown when a review backlog builds up: keeps the most
/// due cards active and sets the rest aside ("lockup").
export function LockupOfferScreen({
  offer,
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
    <Card className="max-w w-full p-6 gap-4 select-none" animate>
      <h2 className="text-xl font-semibold text-center">
        Let's review these cards today
      </h2>

      {CARD_GROUPS.map(({ type, label, showSubtitle }) => {
        const cards = byType.get(type);
        if (!cards || cards.length === 0) return null;
        return (
          <div key={type} className="space-y-2">
            <p className="text-sm font-medium text-muted-foreground text-center">
              {cards.length} {label} {cards.length === 1 ? "card" : "cards"}
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {cards.map((card, i) => (
                <span
                  key={i}
                  className="px-3 py-1.5 rounded-lg border border-border bg-accent/50 text-sm font-medium"
                >
                  <TargetLanguageText language={targetLanguage}>
                    {card.card_text}
                  </TargetLanguageText>
                  {showSubtitle && card.card_subtitle && (
                    <span className="ml-1.5 text-xs text-muted-foreground">
                      {card.card_subtitle}
                    </span>
                  )}
                </span>
              ))}
            </div>
          </div>
        );
      })}

      <p className="text-muted-foreground text-center">
        The rest are set aside for later.
      </p>

      <Button onClick={() => onAccept(lockEvent)} size="lg" className="w-full">
        Let's go
      </Button>
    </Card>
  );
}
