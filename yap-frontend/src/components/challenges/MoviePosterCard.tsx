import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";
import { Poster } from "@/components/Poster";
import type { Deck, MovieMetadataBasic } from "../../../../yap-frontend-rs/pkg";

interface MoviePosterCardProps {
  movie: MovieMetadataBasic;
  deck: Deck;
  className?: string;
  overlayExtra?: ReactNode;
  children?: ReactNode;
}

export function MoviePosterCard({
  movie,
  deck,
  className,
  overlayExtra,
  children,
}: MoviePosterCardProps) {
  return (
    <Card
      key={movie.id}
      className={`overflow-hidden p-0 transition-all group gap-0${className ? ` ${className}` : ""}`}
      animate
    >
      <div className="relative aspect-[2/3] bg-muted">
        <Poster movieId={movie.id} deck={deck} alt={movie.title} />
        <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-black/20 to-transparent opacity-0 group-hover:opacity-100 transition-opacity">
          {movie.rotten_tomatoes_score !== undefined && (
            <div className="absolute top-2 right-2 flex items-center gap-1 rounded-full bg-black/60 px-2 py-0.5 text-xs font-semibold text-white">
              <span>🍅</span>
              <span>{movie.rotten_tomatoes_score}%</span>
            </div>
          )}
          <div className="absolute bottom-0 left-0 right-0 p-3">
            <div className="text-white text-sm font-semibold line-clamp-2">
              {movie.title}
            </div>
            {movie.year && (
              <div className="text-white/70 text-xs mt-1">{movie.year}</div>
            )}
            {overlayExtra}
          </div>
        </div>
      </div>
      {children}
    </Card>
  );
}
