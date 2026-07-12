import { useCallback, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { AudioButton } from "@/components/AudioButton";
import {
  ChallengeSentence,
  TranslationVerdict,
  type TranslationVerdictData,
} from "@/components/challenges/translation-verdict";
import { app, resultText } from "./bridge";
import type { GradeResult, TranslationChallenge } from "./types";

// The widget's translation container: read the sentence, type a translation,
// submit. The colored sentence and the verdict stack are the app's shared
// leaves (ChallengeSentence / TranslationVerdict) rendered verbatim — the only
// widget-owned parts are the textarea, the submit, and the grade_translation
// bridge call (which autogrades on the server and logs the review itself).
// The app's per-phrase grade-adjust UI is deliberately absent (see the merge
// plan): in an MCP host the user can just tell the model.
export function TranslationCard({ challenge }: { challenge: TranslationChallenge }) {
  const [value, setValue] = useState("");
  const [grading, setGrading] = useState(false);
  const [result, setResult] = useState<GradeResult | null>(null);
  const [submitted, setSubmitted] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  // The server-minted nonce is this challenge's idempotency key: a retried
  // submit (e.g. the response was lost after the server already graded, or
  // the widget reloaded and replayed the same tool result) records the
  // review once and replays the original grade instead of re-running the LLM.
  const idempotencyToken = challenge.nonce;

  const { sentence } = challenge;

  const submit = useCallback(async () => {
    const submission = value.trim();
    if (!submission || grading || result) return;
    setGrading(true);
    setError(null);
    try {
      const res = await app.callServerTool({
        name: "grade_translation",
        arguments: {
          language: challenge.language,
          card: challenge.card,
          sentence: sentence.text,
          submission,
          idempotency_token: idempotencyToken,
        },
      });
      if (res.isError) {
        throw new Error(resultText(res) || "grade_translation failed");
      }
      const graded =
        (res.structuredContent as GradeResult | undefined) ??
        (JSON.parse(resultText(res)) as GradeResult);
      setResult(graded);
      setSubmitted(submission);

      const forgot = [
        ...sentence.literals
          .filter((_, i) => graded.literal_grades[i] === "forgot")
          .map((literal) => literal.word.text),
        ...graded.phrases_forgot,
      ];
      const summary = graded.perfect
        ? `user translated «${sentence.text}» as «${submission}» — perfect` +
          ` (${graded.remaining_due} cards still due)`
        : `user translated «${sentence.text}» as «${submission}»` +
          (graded.correct_translation
            ? ` — correct translation: «${graded.correct_translation}»`
            : "") +
          (forgot.length > 0 ? `; forgot: ${forgot.join(", ")}` : "; no words forgotten") +
          (graded.autograding_error ? " (heuristic grading — AI grader unavailable)" : "") +
          ` (${graded.remaining_due} cards still due)`;
      await app.updateModelContext({ content: [{ type: "text", text: summary }] });
    } catch (e) {
      setError(
        `could not grade the translation — ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setGrading(false);
    }
  }, [value, grading, result, challenge, sentence, idempotencyToken]);

  const verdict: TranslationVerdictData | null = result
    ? {
        userTranslation: submitted,
        correctTranslation: result.correct_translation ?? "",
        isPerfect: result.perfect,
        encouragement: result.encouragement,
        explanation: result.explanation,
        autogradingError: result.autograding_error,
      }
    : null;

  return (
    <div className="flex flex-col gap-2 max-w-md mx-auto">
      <Card className="p-4 gap-0">
        <div className="text-center flex flex-col gap-4 items-center">
          <AudioButton
            audioRequest={challenge.audio}
            accessToken={undefined}
            visualizer
          />

          <ChallengeSentence
            literals={sentence.literals}
            grades={result?.literal_grades}
            isPerfect={result?.perfect}
            targetLanguage={challenge.language}
          />

          {!result && (
            <>
              <Textarea
                ref={inputRef}
                placeholder="Translation..."
                value={value}
                onChange={(e) => setValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    void submit();
                  }
                }}
                className="text-lg min-h-0"
                rows={1}
                autoFocus
                disabled={grading}
              />
              <Button
                onClick={() => void submit()}
                size="lg"
                className="w-full h-12 text-lg"
                disabled={grading || !value.trim()}
              >
                {grading ? "Grading…" : "Check"}
              </Button>
            </>
          )}

          {verdict && result && (
            <div className="w-full flex flex-col gap-4 text-left">
              <TranslationVerdict
                verdict={verdict}
                targetLanguage={challenge.language}
              />
              {result.phrases_forgot.length > 0 && (
                <p className="text-sm text-red-600 dark:text-red-400">
                  phrases missed: {result.phrases_forgot.join(", ")}
                </p>
              )}
              {sentence.sources.length > 0 && (
                <p className="text-xs text-muted-foreground font-mono">
                  {sentence.sources.join(" · ")}
                </p>
              )}
            </div>
          )}
        </div>
      </Card>

      {result && (
        <p className="text-sm text-muted-foreground text-center font-mono">
          {result.perfect ? "perfect — review logged" : "review logged"}
        </p>
      )}
      {error && (
        <p className="text-sm text-muted-foreground text-center font-mono">{error}</p>
      )}
    </div>
  );
}
