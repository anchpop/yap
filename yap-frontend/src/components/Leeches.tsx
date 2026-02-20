import { useState, useEffect } from "react";
import { type Deck } from "../../../yap-frontend-rs/pkg";
import { Badge } from "@/components/ui/badge";
import TimeAgo from "react-timeago";

export function Leeches({ deck }: { deck: Deck }) {
  const [currentTimestamp, setCurrentTimestamp] = useState(() => Date.now());
  const [revealedListeningCards, setRevealedListeningCards] = useState<
    Set<string>
  >(() => new Set());

  // Update timestamp periodically to keep timing fresh
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTimestamp(Date.now());
    }, 10000); // Update every 10 seconds

    return () => clearInterval(interval);
  }, []);

  const leeches = deck.get_leeches();

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

  return (
    <div className="flex-1 overflow-hidden flex flex-col">
      <div className="border-b pb-4 mb-4 p-2">
        <p className="text-sm">
          Leeches are cards you're really struggling with. The hardest few cards
          can take disproportionate time, so it's more efficient to set them
          aside for a while.
        </p>
        <p className="text-sm mt-2">
          You have {leeches.length} {leeches.length === 1 ? "leech" : "leeches"}
          .
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {leeches.length === 0 ? (
          <div className="text-center py-12">
            <p className="text-lg mb-2">No leeches!</p>
            <p className="text-sm">
              Keep up the good work! You're making steady progress with all your
              cards.
            </p>
          </div>
        ) : (
          <div className="bg-card border rounded-lg overflow-hidden">
            <table className="w-full table-fixed">
              <thead>
                <tr className="border-b bg-muted/50">
                  <th className="text-left p-3 font-medium w-1/3">Word</th>
                  <th className="text-left p-3 font-medium w-1/3">State</th>
                  <th className="text-left p-3 font-medium w-1/3">Ready</th>
                </tr>
              </thead>
              <tbody>
                {leeches.map((card, index) => {
                  let shortDescription = "";
                  let pos = "";
                  const tags: string[] = [];

                  const isListeningHeteronym =
                    card.card_indicator.type === "ListeningHeteronym";
                  let listeningCardKey: string | null = null;

                  if (card.card_indicator.type === "TargetLanguage") {
                    const lexeme = card.card_indicator.lexeme;
                    if (lexeme.type === "Heteronym") {
                      shortDescription = lexeme.heteronym.word;
                      pos = lexeme.heteronym.pos;
                    } else {
                      shortDescription = lexeme.phrase;
                    }
                  } else if (card.card_indicator.type === "ListeningHomophonous") {
                    shortDescription = `/${card.card_indicator.pronunciation}/`;
                  } else if (isListeningHeteronym) {
                    // ListeningHeteronym always contains a heteronym directly
                    shortDescription = card.card_indicator.heteronym.word;
                    tags.push("listening");
                    listeningCardKey = JSON.stringify(card.card_indicator);
                  } else if (card.card_indicator.type === "LetterPronunciation") {
                    shortDescription = `[${card.card_indicator.pattern}]`;
                  }

                  const isReady = card.due_timestamp_ms <= currentTimestamp;
                  const isListeningCardRevealed = listeningCardKey
                    ? revealedListeningCards.has(listeningCardKey)
                    : false;

                  const wordCellContent = isListeningHeteronym ? (
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
                          <Badge className="bg-green-500/20 text-green-600 dark:text-green-400 border-green-500/30">
                            Ready now
                          </Badge>
                        ) : (
                          <TimeAgo date={new Date(card.due_timestamp_ms)} />
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
