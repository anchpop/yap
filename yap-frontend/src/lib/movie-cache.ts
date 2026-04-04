import type { Deck, MovieMetadataBasic } from "../../../yap-frontend-rs/pkg";

const movieMetadataCache = new Map<string, MovieMetadataBasic>();

/**
 * Get movie metadata with caching. Only fetches uncached movies from the deck.
 * Cache key includes target language since the same movie ID might have different
 * metadata (e.g., different titles) in different language contexts.
 */
export function getMovieMetadata(
  deck: Deck,
  movieIds: string[],
): MovieMetadataBasic[] {
  const targetLanguage = deck.get_target_language();
  const uncachedIds: string[] = [];
  const results: MovieMetadataBasic[] = [];

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
