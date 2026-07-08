import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type {
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

  return (
    <Card className="max-w w-full p-6 gap-4 select-none" animate>
      <h2 className="text-xl font-semibold text-center">
        Let's review these {preview.length} cards today
      </h2>

      <div className="flex flex-wrap justify-center gap-2">
        {preview.map((card, i) => (
          <span
            key={i}
            className="px-3 py-1.5 rounded-lg border border-border bg-accent/50 text-sm font-medium"
          >
            <TargetLanguageText language={targetLanguage}>
              {card.card_text}
            </TargetLanguageText>
            {card.card_subtitle && (
              <span className="ml-1.5 text-xs text-muted-foreground">
                {card.card_subtitle}
              </span>
            )}
          </span>
        ))}
      </div>

      <p className="text-muted-foreground text-center">
        The rest are set aside for later.
      </p>

      <Button onClick={() => onAccept(lockEvent)} size="lg" className="w-full">
        Let's go
      </Button>
    </Card>
  );
}
