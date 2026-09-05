import type { Deck, MovieMetadataBasic } from "../../../yap-frontend-rs/pkg";

const movieMetadataCache = new Map<string, MovieMetadataBasic>();

// Titles can differ between target languages for the same movie ID.
export function getMovieMetadata(
  deck: Deck,
  movieIds: string[],
): MovieMetadataBasic[] {
  const targetLanguage = deck.get_target_language();
  const uncachedIds: string[] = [];
  const results: MovieMetadataBasic[] = [];

  for (const id of movieIds) {
    const cacheKey = `${targetLanguage}-${id}`;
    const cached = movieMetadataCache.get(cacheKey);
    if (cached) {
      results.push(cached);
    } else {
      uncachedIds.push(id);
    }
  }

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
