import { useCallback, useState } from "react";
import { Flashcard } from "@/components/Flashcard";
import { app, resultText } from "./bridge";
import type { FlashcardChallenge, Rating } from "./types";
import type { CardContent, Literal } from "../../../yap-frontend-rs/pkg";

// The widget owns no flashcard UI of its own: it renders the app's real
// Flashcard component (full card back, morphology, homophone grid, drag-to-grade)
// and only adapts the one forced difference — grading. In the app onRating is a
// synchronous deck mutation; here it's an async bridge call to log_review, keyed
// by the server nonce so a retried/replayed submit records the review once. That
// transport mismatch lives entirely in this file, which is where it belongs.

function literalsText(gram: Literal<string>[]): string {
  return gram
    .map((l) => l.word.text + l.whitespace)
    .join("")
    .trim();
}

function contentDisplay(content: CardContent): string {
  if (content.type === "Gram") return literalsText(content.gram);
  const first = content.possible_grams[0];
  return first ? literalsText(first[1]) : "the card";
}

export function WidgetFlashcard({ challenge }: { challenge: FlashcardChallenge }) {
  const [grading, setGrading] = useState(false);
  const [graded, setGraded] = useState<Rating | null>(null);
  const [gradeError, setGradeError] = useState<string | null>(null);
  const [autoplayed, setAutoplayed] = useState(false);
  // A drag-grade flings the card off-screen (framer-motion x:300) before
  // onRating fires; the app never notices because the parent swaps in the next
  // card, but here the card stays mounted. If the bridge call then errors we'd
  // strand it off-screen with no way back — so bump this to remount the card,
  // resetting its transform and letting the user retry.
  const [retryKey, setRetryKey] = useState(0);

  const display = contentDisplay(challenge.content);
  const leftLabel = challenge.is_new ? "didn't know" : "forgot";

  const gradeAsync = useCallback(
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
            // The server-minted nonce is this card's idempotency key: a retried
            // grade (lost response, or a widget reload replaying the tool result)
            // logs the review only once.
            idempotency_token: challenge.nonce,
          },
        });
        if (result.isError) {
          throw new Error(resultText(result) || "log_review failed");
        }
        let remaining: number | undefined;
        try {
          remaining = (
            JSON.parse(resultText(result)) as { remaining_due?: number }
          ).remaining_due;
        } catch {
          // narration only; fine without it
        }
        setGraded(rating);
        const summary =
          `user graded «${display}» as ${rating}` +
          (remaining !== undefined ? ` — ${remaining} cards still due` : "");
        await app.updateModelContext({ content: [{ type: "text", text: summary }] });
      } catch (e) {
        setGradeError(
          `could not save the review — ${e instanceof Error ? e.message : String(e)}`,
        );
        // Remount the card so a drag-grade's off-screen fling is undone and the
        // user can retry. (No-op visually for button/keyboard grades.)
        setRetryKey((k) => k + 1);
      } finally {
        setGrading(false);
      }
    },
    [challenge, grading, graded, display],
  );

  if (graded) {
    return (
      <p className="text-sm text-muted-foreground text-center font-mono py-6">
        graded — {graded === "again" ? leftLabel : graded}
      </p>
    );
  }

  return (
    <div className="max-w-md mx-auto min-h-[24rem] flex flex-col">
      <Flashcard
        key={retryKey}
        audioRequest={challenge.audio}
        content={challenge.content}
        totalCount={challenge.total_count}
        timesTypeSeen={challenge.times_type_seen}
        isNew={challenge.is_new}
        targetLanguage={challenge.language}
        nativeLanguage={challenge.native_language}
        accessToken={undefined}
        autoplayed={autoplayed}
        setAutoplayed={() => setAutoplayed(true)}
        onRating={(rating) => void gradeAsync(rating)}
      />
      {grading && (
        <p className="text-sm text-muted-foreground text-center font-mono pt-2">
          saving…
        </p>
      )}
      {gradeError && (
        <p className="text-sm text-muted-foreground text-center font-mono pt-2">
          {gradeError}
        </p>
      )}
    </div>
  );
}
