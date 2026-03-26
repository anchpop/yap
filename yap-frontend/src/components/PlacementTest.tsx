import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button.tsx";
import { Card } from "@/components/ui/card";
import { ArrowLeft, TriangleAlert } from "lucide-react";
import type { Deck, Language, PlacementTestWord } from "../../../yap-frontend-rs/pkg";
import { Progress } from "@/components/ui/progress";
import { TargetLanguageText } from "./TargetLanguageText";

const NUM_ROUNDS = 3;

interface PlacementTestProps {
  deck: Deck;
  targetLanguage: Language;
  onComplete: (results: {
    knownWords: string[];
    unknownWords: string[];
  }) => void;
}

export function PlacementTest({ deck, targetLanguage, onComplete }: PlacementTestProps) {
  const [round, setRound] = useState(1);
  const [words, setWords] = useState<PlacementTestWord[]>([]);
  const [selectedWords, setSelectedWords] = useState<Set<string>>(new Set());
  const [knownWords, setKnownWords] = useState<string[]>([]);
  const [unknownWords, setUnknownWords] = useState<string[]>([]);

  // Load words for the current round
  useEffect(() => {
    if (round <= NUM_ROUNDS) {
      // Use accumulated known/unknown words for adaptive word selection
      const placementWords = deck.get_placement_test(knownWords, unknownWords);
      if (placementWords.length === 0) {
        setRound(NUM_ROUNDS + 1);
      } else {
        setWords(placementWords);
        setSelectedWords(new Set());
      }
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
    const known = words.filter((w) => selectedWords.has(w.word)).map((w) => w.word);
    const unknown = words.filter((w) => !selectedWords.has(w.word)).map((w) => w.word);

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

  return (
    <Card className="max-w w-full p-0 gap-0 select-none" animate>
      <div className="px-[5px]">
        <Progress value={(round / (NUM_ROUNDS + 1)) * 100} />
      </div>
      <div className="p-6 space-y-4">
        {round > NUM_ROUNDS ? (
          (() => {
            const tooAdvanced =
              knownWords.length / (knownWords.length + unknownWords.length) >
              0.85;
            return (
              <>
                <div className="space-y-2">
                  <h2 className="text-xl font-semibold flex items-center gap-2">
                    {tooAdvanced && (
                      <TriangleAlert className="w-5 h-5 text-yellow-500" />
                    )}
                    {tooAdvanced ? "You might be too advanced" : "Ready to Start!"}
                  </h2>
                </div>
                <p className="text-muted-foreground">
                  {tooAdvanced
                    ? "Yap.Town is designed for intermediate learners. We'll still try our best to find words you don't know!"
                    : "We've analyzed your knowledge level and will tailor your learning experience."}
                </p>
                <Button
                  onClick={() => onComplete({ knownWords, unknownWords })}
                  size="lg"
                  className="w-full"
                >
                  {tooAdvanced ? "Continue Anyway" : "Begin Learning"}
                </Button>
              </>
            );
          })()
        ) : (
          <>
            <div className="space-y-2">
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
              {words.map((pw) => {
                const isSelected = selectedWords.has(pw.word);
                return (
                  <button
                    key={pw.word}
                    onClick={() => toggleWord(pw.word)}
                    className={`
                  p-4 rounded-lg border-2 transition-all
                  ${
                    isSelected
                      ? "border-primary bg-primary/10 scale-95"
                      : "border-border hover:border-primary/50 hover:bg-accent"
                  }
                `}
                  >
                    {isSelected ? (
                      <span key="def" className="text-lg text-muted-foreground truncate block placement-reveal">
                        {pw.definition}
                      </span>
                    ) : (
                      <span key="word" className="text-lg font-medium"><TargetLanguageText language={targetLanguage}>{pw.word}</TargetLanguageText></span>
                    )}
                  </button>
                );
              })}
            </div>

            <Button onClick={handleNext} className="w-full" size="lg">
              Next
            </Button>
          </>
        )}
      </div>
    </Card>
  );
}
