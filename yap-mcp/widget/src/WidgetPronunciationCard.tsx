import { useCallback, useState } from "react";
import { PronunciationChallenge } from "@/components/challenges/PronunciationChallenge";
import { app } from "./bridge";
import { useLogReview } from "./useLogReview";
import type { PronunciationChallenge as PronunciationChallengeData, Rating } from "./types";

// Renders the app's real PronunciationChallenge component (pattern, example
// words with audio, guide text) and adapts grading via useLogReview. The one
// app affordance with no widget equivalent is "can't speak now" — the app
// banishes Speaking challenges for the session; here we just skip the card
// without logging a review and tell the model why.

export function WidgetPronunciationCard({
  challenge,
}: {
  challenge: PronunciationChallengeData;
}) {
  const [skipped, setSkipped] = useState(false);
  const { grading, graded, gradeError, grade } = useLogReview(
    challenge,
    challenge.pattern,
  );

  const cantSpeak = useCallback(() => {
    // A grade in flight (or done) wins — don't tell the model the card was
    // skipped without a review when one was just logged.
    if (grading || graded) return;
    setSkipped(true);
    void app
      .updateModelContext({
        content: [
          {
            type: "text",
            text: `user can't speak right now — skipped the pronunciation card «${challenge.pattern}» without logging a review; don't present more pronunciation cards this session`,
          },
        ],
      })
      .catch(() => {});
  }, [challenge.pattern, grading, graded]);

  if (skipped) {
    return (
      <p className="text-sm text-muted-foreground text-center font-mono py-6">
        skipped — can't speak right now
      </p>
    );
  }

  if (graded) {
    const leftLabel = challenge.is_new ? "didn't know" : "forgot";
    return (
      <p className="text-sm text-muted-foreground text-center font-mono py-6">
        graded — {graded === "again" ? leftLabel : graded}
      </p>
    );
  }

  return (
    <div className="max-w-md mx-auto min-h-[24rem] flex flex-col">
      <PronunciationChallenge
        pattern={challenge.pattern}
        guide={challenge.guide}
        audioRequests={challenge.audio_requests}
        onRating={(rating: Rating) => void grade(rating)}
        accessToken={undefined}
        onCantSpeak={cantSpeak}
        targetLanguage={challenge.language}
        connector={challenge.connector}
        isNew={challenge.is_new}
        timesTypeSeen={challenge.times_type_seen}
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
