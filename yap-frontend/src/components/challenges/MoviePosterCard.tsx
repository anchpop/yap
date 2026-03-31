import { Card } from "@/components/ui/card";
import { Poster } from "@/components/Poster";
import type { ReactNode } from "react";
import type { Deck } from "../../../../yap-frontend-rs/pkg";

interface MoviePosterCardProps {
  id: string;
  title: string;
  year: number | undefined;
  deck: Deck;
  className?: string;
  children?: ReactNode;
}

export function MoviePosterCard({
  id,
  title,
  year,
  deck,
  className,
  children,
}: MoviePosterCardProps) {
  return (
    <Card
      key={id}
      className={`overflow-hidden p-0 transition-all group gap-0${className ? ` ${className}` : ""}`}
      animate
    >
      <div className="relative aspect-[2/3] bg-muted">
        <Poster movieId={id} deck={deck} alt={title} />
        <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-black/20 to-transparent opacity-0 group-hover:opacity-100 transition-opacity">
          <div className="absolute bottom-0 left-0 right-0 p-3">
            <div className="text-white text-sm font-semibold line-clamp-2">
              {title}
            </div>
            {year && (
              <div className="text-white/70 text-xs mt-1">
                {year}
              </div>
            )}
          </div>
        </div>
      </div>
      {children}
    </Card>
  );
}
