import { type ReactNode } from "react";

/**
 * Highlights occurrences of `term` within `sentence` using a yellow background.
 * Case-insensitive matching. Returns a ReactNode with highlighted spans.
 */
export function highlightTermInSentence(
  sentence: string,
  term: string
): ReactNode {
  if (!term) return sentence;

  const lowerSentence = sentence.toLowerCase();
  const lowerTerm = term.toLowerCase();
  const index = lowerSentence.indexOf(lowerTerm);

  if (index === -1) return sentence;

  const before = sentence.slice(0, index);
  const matched = sentence.slice(index, index + term.length);
  const after = sentence.slice(index + term.length);

  return (
    <>
      {before}
      <span className="bg-yellow-500/30 rounded px-0.5">{matched}</span>
      {after}
    </>
  );
}
