import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button.tsx";
import { Card } from "@/components/ui/card";
import { ArrowLeft } from "lucide-react";
import type { Deck } from "../../../yap-frontend-rs/pkg";

const NUM_ROUNDS = 3;

interface PlacementTestProps {
  deck: Deck;
  onComplete: (results: {
    knownWords: string[];
    unknownWords: string[];
  }) => void;
}

export function PlacementTest({ deck, onComplete }: PlacementTestProps) {
  const [round, setRound] = useState(1);
  const [words, setWords] = useState<string[]>([]);
  const [selectedWords, setSelectedWords] = useState<Set<string>>(new Set());
  const [knownWords, setKnownWords] = useState<string[]>([]);
  const [unknownWords, setUnknownWords] = useState<string[]>([]);

  // Load words for the current round
  useEffect(() => {
    if (round <= NUM_ROUNDS) {
      // Use accumulated known/unknown words for adaptive word selection
      const placementWords = deck.get_placement_test(knownWords, unknownWords);
      setWords(placementWords);
      setSelectedWords(new Set());
    }
  }, [round, deck, knownWords, unknownWords]);

  const toggleWord = (word: string) => {
    setSelectedWords((prev) => {
      const next = new Set(prev);
      if (next.has(word)) {
        next.delete(word);
      } else {
        next.add(word);
      }
      return next;
    });
  };

  const handleNext = () => {
    // Words that were selected are known, others are unknown
    const known = words.filter((w) => selectedWords.has(w));
    const unknown = words.filter((w) => !selectedWords.has(w));

    setKnownWords((prev) => [...prev, ...known]);
    setUnknownWords((prev) => [...prev, ...unknown]);

    if (round < NUM_ROUNDS) {
      setRound(round + 1);
    } else {
      // Done with placement test
      setRound(NUM_ROUNDS + 1);
    }
  };

  const handleBack = () => {
    if (round > 1 && round <= NUM_ROUNDS) {
      // Go back to the very beginning
      setRound(1);
      setKnownWords([]);
      setUnknownWords([]);
      setSelectedWords(new Set());
    }
  };

  if (round > NUM_ROUNDS) {
    return (
      <div className="flex justify-center items-center">
        <Card className="max-w w-full p-8 text-center" animate>
          <h2 className="text-2xl font-semibold mb-4">Ready to Start!</h2>
          <p className="text-muted-foreground mb-6">
            We've analyzed your knowledge level and will tailor your learning
            experience.
          </p>
          <Button
            onClick={() => onComplete({ knownWords, unknownWords })}
            size="lg"
            className="w-full"
          >
            Begin Learning
          </Button>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex justify-center items-center">
      <Card className="max-w-2xl w-full p-6" animate>
        <div className="mb-6">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              {round > 1 && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={handleBack}
                  className="h-8 w-8"
                  title="Go back"
                >
                  <ArrowLeft className="w-5 h-5" />
                </Button>
              )}
              <h2 className="text-xl font-semibold">Placement Test</h2>
            </div>
            <span className="text-sm text-muted-foreground">
              Round {round} of {NUM_ROUNDS}
            </span>
          </div>
          <p className="text-muted-foreground">
            Tap the words you know, then press Next
          </p>
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-3 gap-3 mb-6">
          {words.map((word) => (
            <button
              key={word}
              onClick={() => toggleWord(word)}
              className={`
                p-4 rounded-lg border-2 transition-all
                ${
                  selectedWords.has(word)
                    ? "border-primary bg-primary/10 scale-95"
                    : "border-border hover:border-primary/50 hover:bg-accent"
                }
              `}
            >
              <span className="text-lg font-medium">{word}</span>
            </button>
          ))}
        </div>

        <Button onClick={handleNext} className="w-full" size="lg">
          Next
        </Button>
      </Card>
    </div>
  );
}
