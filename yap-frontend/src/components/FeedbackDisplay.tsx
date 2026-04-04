import Markdown from "react-markdown";

interface FeedbackDisplayProps {
  encouragement?: string;
  explanation?: string;
  perfect?: boolean;
}

export function FeedbackDisplay({
  encouragement,
  explanation,
  perfect = false,
}: FeedbackDisplayProps) {
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
                <Markdown>{encouragement}</Markdown>
              </div>
            </div>
          </div>
        )}

        {explanation && (
          <div className="animate-fade-in-delay-2">
            <Markdown>{explanation}</Markdown>
          </div>
        )}
      </div>
    </div>
  );
}
