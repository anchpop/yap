import type { Deck } from '../../../yap-frontend-rs/pkg';

interface MovieMetadata {
  id: string;
  title: string;
  year: number | undefined;
  poster_bytes: number[] | undefined;
}

// Global cache for movie metadata
// NOTE: This is technically spaghetti code - we're using a global cache that persists
// across deck changes, which can be problematic. However, it's the simplest way to
// avoid re-fetching movie metadata (especially poster JPEGs) on every render or deck
// change. The alternative would be a proper React context or state management solution,
// but that adds complexity. We cache by language+movieId to handle different languages.
const movieMetadataCache = new Map<string, MovieMetadata>();

/**
 * Get movie metadata with caching. Only fetches uncached movies from the deck.
 * Cache key includes target language since the same movie ID might have different
 * metadata (e.g., different posters or titles) in different language contexts.
 */
export function getMovieMetadata(deck: Deck, movieIds: string[]): MovieMetadata[] {
  const targetLanguage = deck.get_target_language();
  const uncachedIds: string[] = [];
  const results: MovieMetadata[] = [];

  // Check which movies we need to fetch
  for (const id of movieIds) {
    const cacheKey = `${targetLanguage}-${id}`;
    const cached = movieMetadataCache.get(cacheKey);
    if (cached) {
      results.push(cached);
    } else {
      uncachedIds.push(id);
    }
  }

  // Fetch only uncached movies
  if (uncachedIds.length > 0) {
    const newMetadata = deck.get_movie_metadata(uncachedIds);
    for (const metadata of newMetadata) {
      const cacheKey = `${targetLanguage}-${metadata.id}`;
      movieMetadataCache.set(cacheKey, metadata);
      results.push(metadata);
    }
  }

  return results;
}

/**
 * Clear the movie metadata cache (useful for testing or memory management)
 */
export function clearMovieCache() {
  movieMetadataCache.clear();
}
