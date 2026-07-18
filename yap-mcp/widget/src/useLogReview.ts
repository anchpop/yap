// Grading over the bridge, shared by the self-graded card widgets. In the app
// onRating is a synchronous deck mutation; here it's an async call to
// log_review, keyed by the server nonce so a retried/replayed submit records
// the review once. That transport mismatch lives entirely here.
import { useCallback, useRef, useState } from "react";
import { app, resultText } from "./bridge";
import type { Rating } from "./types";
import type { Language } from "../../../yap-frontend-rs/pkg";

interface GradeableChallenge {
  language: Language;
  card: unknown;
  nonce: string;
}

export function useLogReview(
  challenge: GradeableChallenge,
  display: string,
  onError?: () => void,
) {
  const [grading, setGrading] = useState(false);
  const [graded, setGraded] = useState<Rating | null>(null);
  const [gradeError, setGradeError] = useState<string | null>(null);
  // Synchronous single-submit lock. State guards alone don't cut it: the app
  // components' keydown effects capture the first render's callback (empty
  // dependency lists), so a stale closure can call grade() with grading/graded
  // still false. The ref is checked-and-set synchronously, so a second call —
  // however stale its closure — can never race a pending submission and later
  // report a rating other than the one actually recorded.
  const submittedRef = useRef(false);

  const grade = useCallback(
    async (rating: Rating) => {
      if (submittedRef.current) return;
      submittedRef.current = true;
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
        let recorded = rating;
        try {
          const response = JSON.parse(resultText(result)) as {
            remaining_due?: number;
            rating?: Rating;
          };
          remaining = response.remaining_due;
          // An idempotent replay (retry after a lost response) returns the
          // originally recorded review — report that rating, not the attempt.
          recorded = response.rating ?? rating;
        } catch {
          // narration only; fine without it
        }
        setGraded(recorded);
        const summary =
          `user graded «${display}» as ${recorded}` +
          (remaining !== undefined ? ` — ${remaining} cards still due` : "");
        await app.updateModelContext({ content: [{ type: "text", text: summary }] });
      } catch (e) {
        // Release the lock so the user can retry.
        submittedRef.current = false;
        setGradeError(
          `could not save the review — ${e instanceof Error ? e.message : String(e)}`,
        );
        onError?.();
      } finally {
        setGrading(false);
      }
    },
    [challenge, display, onError],
  );

  return { grading, graded, gradeError, grade };
}
