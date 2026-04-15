import { memo, useState, useDeferredValue } from "react";
import { MoviePosterCard } from "@/components/challenges/MoviePosterCard";
import type { Deck, MovieMetadataBasic } from "../../../yap-frontend-rs/pkg";

interface MovieWithMetadata extends MovieMetadataBasic {
  percent_known: number;
  cards_to_next_milestone: number | null | undefined;
}

interface MoviesProps {
  moviesWithMetadata: MovieWithMetadata[];
  targetLanguageIso?: string;
  deck: Deck;
}

export const Movies = memo(function Movies({
  moviesWithMetadata: moviesWithMetadataProp,
  targetLanguageIso,
  deck,
}: MoviesProps) {
  const moviesWithMetadata = useDeferredValue(moviesWithMetadataProp);
  const [showAllMovies, setShowAllMovies] = useState(false);

  const sortedMovies = targetLanguageIso
    ? [...moviesWithMetadata].sort((a, b) => {
        const aIsNative = a.original_language === targetLanguageIso ? 0 : 1;
        const bIsNative = b.original_language === targetLanguageIso ? 0 : 1;
        return aIsNative - bIsNative;
      })
    : moviesWithMetadata;
  const visibleMovies = showAllMovies ? sortedMovies : sortedMovies.slice(0, 8);

  if (moviesWithMetadata.length === 0) {
    return null;
  }

  return (
    <div className="mt-6">
      <h2 className="text-2xl font-semibold mb-3">Movies</h2>
      <p className="text-sm text-muted-foreground mb-4">
        You can usually watch a movie comfortably once you know 95% of the
        words.
      </p>
      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
        {visibleMovies.map((movie) => (
          <MoviePosterCard
            key={movie.id}
            movie={movie}
            deck={deck}
            overlayExtra={
              movie.cards_to_next_milestone !== null &&
              movie.cards_to_next_milestone !== undefined && (
                <div className="text-white/90 text-xs mt-2 font-medium">
                  {movie.cards_to_next_milestone}{" "}
                  {movie.cards_to_next_milestone === 1 ? "card" : "cards"} to{" "}
                  {Math.ceil(movie.percent_known / 5) * 5}%
                </div>
              )
            }
          >
            <div className="p-2 text-center relative overflow-hidden">
              <div
                className="absolute inset-0 bg-foreground/10"
                style={{
                  clipPath: `inset(0 ${100 - movie.percent_known}% 0 0)`,
                }}
              />
              <span className="relative text-sm font-mono font-semibold text-foreground">
                {Math.floor(movie.percent_known)}% known
              </span>
            </div>
          </MoviePosterCard>
        ))}
      </div>
      {!showAllMovies && sortedMovies.length > 10 && (
        <div className="mt-4">
          <button
            onClick={() => setShowAllMovies(true)}
            className="w-full py-3 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors duration-200 font-medium rounded-md border border-border"
          >
            Show all {sortedMovies.length} movies
          </button>
        </div>
      )}
    </div>
  );
});
