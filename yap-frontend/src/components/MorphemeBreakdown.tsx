import { type Language } from "../../../yap-frontend-rs/pkg";
import { cn } from "@/lib/pure";
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
    // min-w-0 so the scrolling grid below can actually clamp to the space it
    // has: without it this wrapper is floored at the grid's min-content width
    // and a long breakdown pushes past its container instead.
    <div className={cn("min-w-0", className)}>
      {/* An interlinear grid can't wrap without losing column alignment, so a
          long breakdown scrolls sideways rather than widening its container. */}
      <div
        className="inline-grid gap-x-4 gap-y-1 justify-items-start max-w-full overflow-x-auto"
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
              className="text-muted-foreground max-w-[12rem] line-clamp-3"
              style={{
                gridRow: glossRow,
                gridColumn: i + 1,
                ...fadeStyle,
              }}
              title={gloss ?? undefined}
            >
              {gloss ?? ""}
            </div>,
          ];
          if (hasCanonicalRow) {
            cells.push(
              <div
                key={`c-${i}`}
                className="text-muted-foreground italic max-w-[12rem] line-clamp-3"
                style={{
                  gridRow: 2,
                  gridColumn: i + 1,
                  ...fadeStyle,
                }}
                title={canonical ?? undefined}
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
