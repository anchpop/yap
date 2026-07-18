import { useCallback, useState } from "react";
import { Flashcard } from "@/components/Flashcard";
import { useLogReview } from "./useLogReview";
import type { FlashcardChallenge, Rating } from "./types";
import type { CardContent, Literal } from "../../../yap-frontend-rs/pkg";

// The widget owns no flashcard UI of its own: it renders the app's real
// Flashcard component (full card back, morphology, homophone grid, drag-to-grade)
// and only adapts the one forced difference — grading, via useLogReview.

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
  const [autoplayed, setAutoplayed] = useState(false);
  // A drag-grade flings the card off-screen (framer-motion x:300) before
  // onRating fires; the app never notices because the parent swaps in the next
  // card, but here the card stays mounted. If the bridge call then errors we'd
  // strand it off-screen with no way back — so bump this to remount the card,
  // resetting its transform and letting the user retry.
  const [retryKey, setRetryKey] = useState(0);
  const remount = useCallback(() => setRetryKey((k) => k + 1), []);

  const display = contentDisplay(challenge.content);
  const leftLabel = challenge.is_new ? "didn't know" : "forgot";
  const { grading, graded, gradeError, grade } = useLogReview(
    challenge,
    display,
    // (Remount is a no-op visually for button/keyboard grades.)
    remount,
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
        onRating={(rating: Rating) => void grade(rating)}
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
