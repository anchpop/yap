// Shared, WASM-value-free presentation for a translation grade — rendered by
// BOTH the app's TranslationChallenge and the yap-mcp widget's TranslationCard.
// The verdict layout (and the colored sentence, the class that produced the
// `<word>`-tag divergence bug) lives here once, so the two containers can never
// drift again. Everything imported here is `import type` from the pkg or a
// wasm-free leaf; the widget's wasm-guard build enforces that.
import type { Language, Literal } from "../../../../yap-frontend-rs/pkg";
import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";
import { TargetLanguageText } from "@/components/TargetLanguageText";
import { FeedbackDisplay } from "@/components/FeedbackDisplay";

// One normalized per-literal grade, aligned to the sentence's literals in order
// (null = ungradable). The app maps its WASM `LiteralGrades` ("Remembered" /
// "Forgot") into this once; the server already emits it lowercase. Having a
// single casing kills the "remembered" vs "Remembered" mismatch at the boundary.
export type LiteralGrade = "remembered" | "forgot" | null;

/** The normalized data a graded verdict renders from — same shape both sides. */
export interface TranslationVerdictData {
  userTranslation: string;
  correctTranslation: string;
  isPerfect: boolean;
  encouragement: string | null;
  explanation: string | null;
  autogradingError: string | null;
}

interface ChallengeSentenceProps {
  literals: Literal<string>[];
  grades?: LiteralGrade[];
  isPerfect?: boolean;
  targetLanguage: Language;
  // Tap-to-define is app-only; omit these in the widget and the words render
  // as plain (non-interactive) colored text.
  onWordTap?: (index: number) => void;
  tappedWords?: Set<number>;
  literalGramIndices?: number[];
  tappedGramGroups?: Set<number>;
}

const EMPTY_SET: Set<number> = new Set();

export function ChallengeSentence({
  literals,
  grades,
  isPerfect,
  targetLanguage,
  onWordTap,
  tappedWords = EMPTY_SET,
  literalGramIndices = [],
  tappedGramGroups = EMPTY_SET,
}: ChallengeSentenceProps) {
  const getLiteralColorClass = (literal: Literal<string>, i: number) => {
    if (isPerfect) {
      return "text-green-600 dark:text-green-400";
    }

    const isHeteronym =
      (literal.word.word_type as { type?: string })?.type === "Heteronym";

    // Highlight all literals in a tapped gram group
    const gramGroup = literalGramIndices[i];
    if (gramGroup !== undefined && tappedGramGroups.has(gramGroup)) {
      return "text-yellow-500 dark:text-yellow-400";
    }

    // Also highlight individually tapped words (backwards compat)
    if (isHeteronym && tappedWords.has(i)) {
      return "text-yellow-500 dark:text-yellow-400";
    }

    if (!grades || !isHeteronym) {
      return "";
    }

    const grade = grades[i];
    if (grade === "remembered") return "text-green-600 dark:text-green-400";
    if (grade === "forgot") return "text-red-600 dark:text-red-400";

    return "";
  };

  return (
    <h2 className="text-2xl font-semibold">
      {literals.map((literal: Literal<string>, i: number) => {
        const colorClass = getLiteralColorClass(literal, i);
        const isHeteronym =
          (literal.word.word_type as { type?: string })?.type === "Heteronym";
        const interactive = isHeteronym && !!onWordTap;

        return (
          <span key={i}>
            <span
              className={cn(
                colorClass,
                interactive
                  ? "cursor-pointer underline-offset-3 underline decoration-dotted hover:decoration-solid hover:decoration-3 transition-transform hover:scale-105 inline-block"
                  : "",
              )}
              onClick={() => {
                if (interactive) {
                  onWordTap(i);
                }
              }}
            >
              <TargetLanguageText language={targetLanguage}>
                {literal.word.text}
              </TargetLanguageText>
            </span>
            {literal.whitespace}
          </span>
        );
      })}
    </h2>
  );
}

export function YourTranslation({ userTranslation }: { userTranslation: string }) {
  return (
    <div className="rounded-lg p-4 border">
      <p className="text-sm font-medium mb-1">Your translation:</p>
      <p className="text-lg font-medium">{userTranslation}</p>
    </div>
  );
}

export function CorrectTranslation({ sentence }: { sentence: string }) {
  return (
    <div className="bg-green-500/10 rounded-lg p-4 border border-green-500/20">
      <p className="text-sm font-medium text-green-600 dark:text-green-400 mb-1">
        Correct translation:
      </p>
      <p className="text-lg font-medium">{sentence}</p>
    </div>
  );
}

export function FeedbackSkeleton() {
  return (
    <div className="space-y-4 mt-4 animate-feedback-in">
      <div className="space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    </div>
  );
}

export function AutogradeError() {
  return (
    <div className="rounded-lg p-4 border bg-yellow-500/10 border-yellow-500/20">
      <p className="text-sm font-medium mb-1 text-yellow-600 dark:text-yellow-400">
        Your submission could not be graded automatically. Please grade the
        words manually below.
      </p>
    </div>
  );
}

/**
 * The graded-verdict feedback stack (your/correct translation, autograde
 * fallback notice, LLM feedback). The app renders its manual grade-adjust UI
 * (PhraseStatuses) as a sibling after this; the widget renders nothing after.
 * The DOM structure matches the app's prior inline markup exactly.
 */
export function TranslationVerdict({
  verdict,
  targetLanguage,
}: {
  verdict: TranslationVerdictData;
  targetLanguage: Language;
}) {
  if (verdict.isPerfect) {
    return (
      <div className="space-y-2">
        <CorrectTranslation sentence={verdict.correctTranslation} />
        <FeedbackDisplay
          encouragement={verdict.encouragement ?? undefined}
          explanation={verdict.explanation ?? undefined}
          perfect
          targetLanguage={targetLanguage}
        />
      </div>
    );
  }

  return (
    <>
      <div className="space-y-2">
        <YourTranslation userTranslation={verdict.userTranslation} />
        <CorrectTranslation sentence={verdict.correctTranslation} />
      </div>

      {verdict.autogradingError && <AutogradeError />}

      <FeedbackDisplay
        encouragement={verdict.encouragement ?? undefined}
        explanation={verdict.explanation ?? undefined}
        targetLanguage={targetLanguage}
      />
    </>
  );
}
