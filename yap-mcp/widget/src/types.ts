// Mirror of present_card / present_translation's structuredContent.challenge
// (see yap-mcp/src/server.rs). The card object is opaque: passed back
// verbatim to log_review / grade_translation, never inspected here.
import type { AudioRequest, Language, Rating } from "../../../yap-frontend-rs/pkg";

interface ChallengeBase {
  // Server-minted id for this presentation, echoed back as the review's
  // idempotency token so a retried/re-rendered submit records it once.
  nonce: string;
  language: Language;
  is_new: boolean;
  card: unknown;
  audio: AudioRequest;
}

export interface FlashcardChallenge extends ChallengeBase {
  type: "flashcard";
  kind: "written" | "listening" | "pronunciation";
  word: { display: string; gloss: string };
}

export interface SentenceLiteral {
  text: string;
  whitespace: string;
  gradable: boolean;
}

export interface TranslationChallenge extends ChallengeBase {
  type: "translation";
  sentence: {
    text: string;
    literals: SentenceLiteral[];
    sources: string[];
  };
}

export type Challenge = FlashcardChallenge | TranslationChallenge;

// Mirror of grade_translation's structured result.
export interface GradeResult {
  perfect: boolean;
  correct_translation: string | null;
  encouragement: string | null;
  explanation: string | null;
  autograding_error: string | null;
  literal_grades: ("remembered" | "forgot" | null)[];
  phrases_remembered: string[];
  phrases_forgot: string[];
  remaining_due: number;
}

export type { AudioRequest, Rating };
