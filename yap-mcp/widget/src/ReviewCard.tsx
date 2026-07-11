import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, MoreVertical } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AudioButton } from "@/components/AudioButton";
import { TargetLanguageText } from "@/components/TargetLanguageText";
import { app, resultText } from "./bridge";
import type { Challenge, Rating } from "./types";

// The same review interaction as the app's Flashcard, driven by tool data
// instead of the WASM deck: tap (or Space) reveals, then grade. Grades go
// straight to log_review over the bridge — the user's own finger, no model
// in between — and the outcome is posted back into the model's context.
export function ReviewCard({ challenge }: { challenge: Challenge }) {
  const [revealed, setRevealed] = useState(false);
  const [grading, setGrading] = useState(false);
  const [graded, setGraded] = useState<Rating | null>(null);
  const [gradeError, setGradeError] = useState<string | null>(null);
  const [autoplayed, setAutoplayed] = useState(false);

  const { word, sentence, is_new: isNew } = challenge;
  // Listening cards hide the text until revealed, same as in the app.
  const listenFirst = challenge.kind === "listening";
  const leftLabel = isNew ? "Didn't know" : "Forgot";
  const rightLabel = isNew ? "Already knew" : "Remembered";

  const grade = useCallback(
    async (rating: Rating) => {
      if (grading || graded) return;
      setGrading(true);
      setGradeError(null);
      try {
        const result = await app.callServerTool({
          name: "log_review",
          arguments: {
            language: challenge.language,
            card: challenge.card,
            rating,
          },
        });
        if (result.isError) {
          throw new Error(resultText(result) || "log_review failed");
        }
        let remaining: number | undefined;
        try {
          remaining = (JSON.parse(resultText(result)) as { remaining_due?: number })
            .remaining_due;
        } catch {
          // narration only; fine without it
        }
        setGraded(rating);
        const summary =
          `user graded «${word.display}» as ${rating}` +
          (remaining !== undefined ? ` — ${remaining} cards still due` : "");
        await app.updateModelContext({ content: [{ type: "text", text: summary }] });
      } catch (e) {
        setGradeError(
          `could not save the review — ${e instanceof Error ? e.message : String(e)}`,
        );
      } finally {
        setGrading(false);
      }
    },
    [challenge, grading, graded, word.display],
  );

  // Same keyboard map as the app: Space/↓ reveals, ← forgot, → remembered.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        setRevealed(true);
      } else if (revealed && e.key === "ArrowLeft") {
        void grade("again");
      } else if (revealed && e.key === "ArrowRight") {
        void grade("remembered");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [revealed, grade]);

  const canGrade = revealed && !grading && !graded;

  return (
    <div className="flex flex-col gap-2 max-w-md mx-auto">
      <Card
        className="p-4 cursor-pointer gap-0"
        onClick={() => setRevealed(true)}
      >
        <div className="text-center flex flex-col gap-4 items-center">
          <div onClick={(e) => e.stopPropagation()}>
            <AudioButton
              audioRequest={challenge.audio}
              accessToken={undefined}
              autoPlay={!autoplayed && listenFirst}
              visualizer
              onSuccess={() => setAutoplayed(true)}
            />
          </div>

          {sentence ? (
            <>
              {(!listenFirst || revealed) && (
                <div className="text-2xl">
                  <TargetLanguageText language={challenge.language}>
                    {sentence.text}
                  </TargetLanguageText>
                </div>
              )}
              {revealed && sentence.translations[0] && (
                <p className="text-muted-foreground text-lg">{sentence.translations[0]}</p>
              )}
              {revealed && sentence.sources.length > 0 && (
                <p className="text-xs text-muted-foreground font-mono">
                  {sentence.sources.join(" · ")}
                </p>
              )}
            </>
          ) : (
            <>
              {(!listenFirst || revealed) && (
                <div className="text-3xl font-semibold">
                  <TargetLanguageText language={challenge.language}>
                    {word.display}
                  </TargetLanguageText>
                </div>
              )}
              {revealed && word.gloss && (
                <p className="text-muted-foreground text-lg">{word.gloss}</p>
              )}
            </>
          )}

          {!revealed && (
            <p className="text-xs text-muted-foreground">tap to reveal</p>
          )}
        </div>
      </Card>

      {revealed && !graded && (
        <div className="flex items-stretch gap-1">
          <div className="grid grid-cols-2 flex-1">
            <Button
              onClick={() => void grade("again")}
              variant="destructive"
              size="lg"
              className="h-14 text-lg rounded-r-none group"
              disabled={!canGrade}
            >
              <span className="relative flex items-center justify-center">
                <kbd className="absolute right-full mr-2 h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                  <ArrowLeft className="h-3 w-3" />
                </kbd>
                {leftLabel}
              </span>
            </Button>
            <Button
              onClick={() => void grade("remembered")}
              variant="default"
              size="lg"
              className="h-14 text-lg rounded-l-none group"
              disabled={!canGrade}
            >
              <span className="relative flex items-center justify-center">
                {rightLabel}
                <kbd className="absolute left-full ml-2 h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                  <ArrowRight className="h-3 w-3" />
                </kbd>
              </span>
            </Button>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="lg" className="h-14" disabled={!canGrade}>
                <MoreVertical className="h-6 w-6" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => void grade("easy")}>Easy</DropdownMenuItem>
              <DropdownMenuItem onClick={() => void grade("good")}>Good</DropdownMenuItem>
              <DropdownMenuItem onClick={() => void grade("hard")}>Hard</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      )}

      {graded && (
        <p className="text-sm text-muted-foreground text-center font-mono">
          graded — {graded === "again" ? leftLabel.toLowerCase() : graded}
        </p>
      )}
      {gradeError && (
        <p className="text-sm text-muted-foreground text-center font-mono">{gradeError}</p>
      )}
    </div>
  );
}
