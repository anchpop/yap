import { type Language } from "../../../yap-frontend-rs/pkg";
import { TargetLanguageText } from "./TargetLanguageText";

export type BreakdownRow = [string, string | null | undefined, string | null | undefined];

export function MorphemeBreakdown({
  breakdown,
  targetLanguage,
  baseDelay = 0,
  className,
}: {
  breakdown: BreakdownRow[];
  targetLanguage: Language;
  baseDelay?: number;
  className?: string;
}) {
  if (breakdown.length === 0) return null;
  const hasCanonicalRow = breakdown.some(([, canonical]) => canonical != null);
  const glossRow = hasCanonicalRow ? 3 : 2;
  return (
    <div className={className}>
      <div
        className="inline-grid gap-x-4 gap-y-1 justify-items-start"
        style={{
          gridTemplateColumns: `repeat(${breakdown.length}, auto)`,
        }}
      >
        {breakdown.flatMap(([surface, canonical, gloss], i) => {
          const delay = baseDelay + i * 0.2;
          const fadeStyle =
            baseDelay > 0
              ? {
                  opacity: 0,
                  animation: `fade-in 0.4s ease-out ${delay}s forwards`,
                }
              : undefined;
          const cells = [
            <div
              key={`s-${i}`}
              className="font-medium"
              style={{
                gridRow: 1,
                gridColumn: i + 1,
                ...fadeStyle,
              }}
            >
              <TargetLanguageText language={targetLanguage}>
                {surface}
              </TargetLanguageText>
            </div>,
            <div
              key={`g-${i}`}
              className="text-muted-foreground"
              style={{
                gridRow: glossRow,
                gridColumn: i + 1,
                ...fadeStyle,
              }}
            >
              {gloss ?? ""}
            </div>,
          ];
          if (hasCanonicalRow) {
            cells.push(
              <div
                key={`c-${i}`}
                className="text-muted-foreground italic"
                style={{
                  gridRow: 2,
                  gridColumn: i + 1,
                  ...fadeStyle,
                }}
              >
                {canonical != null ? (
                  <TargetLanguageText language={targetLanguage}>
                    {canonical}
                  </TargetLanguageText>
                ) : (
                  ""
                )}
              </div>,
            );
          }
          return cells;
        })}
      </div>
    </div>
  );
}
