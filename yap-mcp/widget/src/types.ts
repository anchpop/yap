// Mirror of present_card's structuredContent.challenge (see
// yap-mcp/src/server.rs). The card object is opaque: passed back verbatim to
// log_review, never inspected here.
import type { AudioRequest, Language, Rating } from "../../../yap-frontend-rs/pkg";

export interface Challenge {
  language: Language;
  kind: "written" | "listening" | "pronunciation";
  is_new: boolean;
  card: unknown;
  word: { display: string; gloss: string };
  sentence: {
    text: string;
    translations: string[];
    sources: string[];
  } | null;
  audio: AudioRequest;
}

export type { AudioRequest, Rating };
