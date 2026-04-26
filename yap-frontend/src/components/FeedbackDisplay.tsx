import { useMemo } from "react";
import Markdown, { type Components } from "react-markdown";
import rehypeRaw from "rehype-raw";
import type { Language } from "../../../yap-frontend-rs/pkg";
import { languageToIso6391 } from "@/lib/utils";

interface FeedbackDisplayProps {
  encouragement?: string;
  explanation?: string;
  perfect?: boolean;
  targetLanguage: Language;
}

export function FeedbackDisplay({
  encouragement,
  explanation,
  perfect = false,
  targetLanguage,
}: FeedbackDisplayProps) {
  // Custom tags emitted by the autograder LLM. Keys are lowercased because the
  // HTML parser normalizes tag names. Cast to Components to bypass the
  // intrinsic-element type constraint.
  const components = useMemo(
    () =>
      ({
        word: ({ children }: { children?: React.ReactNode }) => (
          <span lang={languageToIso6391(targetLanguage)}>{children}</span>
        ),
      }) as Components,
    [targetLanguage],
  );

  if (!encouragement && !explanation) {
    return null;
  }

  return (
    <div className="rounded-lg p-4 border bg-blue-500/10 border-blue-500/20">
      <p className="text-sm font-medium mb-1 text-blue-600 dark:text-blue-400">
        Feedback:
      </p>
      <div className="space-y-3">
        {encouragement && (
          <div className="animate-fade-in px-3 py-2 rounded-md bg-gradient-to-r from-green-500/5 to-emerald-500/5 border-l-2 border-green-500/40">
            <div className="flex items-start gap-2">
              <span className="text-lg leading-none">
                {perfect ? "🎉" : "☀️"}
              </span>
              <div className="flex-1 font-medium text-green-700 dark:text-green-300">
                <Markdown rehypePlugins={[rehypeRaw]} components={components}>
                  {encouragement}
                </Markdown>
              </div>
            </div>
          </div>
        )}

        {explanation && (
          <div className="animate-fade-in-delay-2">
            <Markdown rehypePlugins={[rehypeRaw]} components={components}>
              {explanation}
            </Markdown>
          </div>
        )}
      </div>
    </div>
  );
}
