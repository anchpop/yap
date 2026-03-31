import { MoviePosterCard } from "./MoviePosterCard";
import type { Deck, MovieMetadataBasic } from "../../../../yap-frontend-rs/pkg";

interface MoviePosterGridProps {
  movieData: MovieMetadataBasic[];
  deck: Deck;
}

/**
 * Displays a limited grid of movie posters: 2 on mobile, 4 on desktop.
 */
export function MoviePosterGrid({ movieData, deck }: MoviePosterGridProps) {
  if (movieData.length === 0) return null;

  return (
    <div className="mt-3 grid grid-cols-2 md:grid-cols-4 gap-3">
      {movieData.slice(0, 4).map((movie, index) => (
        <MoviePosterCard
          key={movie.id}
          id={movie.id}
          title={movie.title}
          year={movie.year}
          deck={deck}
          className={index >= 2 ? "hidden md:block" : undefined}
        />
      ))}
    </div>
  );
}
