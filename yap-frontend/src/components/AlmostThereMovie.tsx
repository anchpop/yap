import { Card } from "@/components/ui/card";

interface MovieWithMetadata {
  id: string;
  percent_known: number;
  cards_to_next_milestone: number | null | undefined;
  title?: string;
  year?: number;
  poster_bytes?: number[];
}

interface AlmostThereMovieProps {
  movie: MovieWithMetadata;
}

export function AlmostThereMovie({ movie }: AlmostThereMovieProps) {
  // Helper function to convert poster bytes to data URL
  const getPosterDataUrl = (posterBytes: number[] | undefined) => {
    if (!posterBytes) return null;
    const uint8Array = new Uint8Array(posterBytes);
    let binaryString = "";
    const chunkSize = 8192;
    for (let i = 0; i < uint8Array.length; i += chunkSize) {
      const chunk = uint8Array.subarray(i, i + chunkSize);
      binaryString += String.fromCharCode(...chunk);
    }
    return `data:image/jpeg;base64,${btoa(binaryString)}`;
  };

  return (
    <Card variant="light" className="overflow-hidden p-0" animate>
      <div className="flex flex-row gap-0">
        <div className="w-24 sm:w-32 aspect-[2/3] bg-muted relative flex-shrink-0">
          {getPosterDataUrl(movie.poster_bytes) ? (
            <img
              src={getPosterDataUrl(movie.poster_bytes)!}
              alt={movie.title}
              className="w-full h-full object-cover opacity-90 saturate-70 dark:opacity-70 dark:saturate-80"
            />
          ) : (
            <div className="w-full h-full flex items-center justify-center text-4xl">
              🎬
            </div>
          )}
        </div>
        <div className="flex-1 p-4 flex flex-col justify-center">
          <div className="text-xs font-medium text-muted-foreground mb-1 uppercase">
            Almost There
          </div>
          <h3 className="text-lg font-semibold text-muted-foreground mb-1">
            {movie.title}
          </h3>
          {movie.year && (
            <div className="text-sm text-muted-foreground mb-2">
              {movie.year}
            </div>
          )}
          <p className="text-sm mb-3 text-muted-foreground">
            You're just{" "}
            <span className="font-semibold text-muted-foreground">
              {movie.cards_to_next_milestone}{" "}
              {movie.cards_to_next_milestone === 1 ? "card" : "cards"}
            </span>{" "}
            away from reaching{" "}
            <span className="font-semibold text-muted-foreground">
              {Math.ceil(movie.percent_known / 5) * 5}%
            </span>{" "}
            comprehension!
          </p>
          <div className="flex items-center gap-2">
            <div className="flex-1 h-2 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-muted-foreground transition-all duration-300"
                style={{ width: `${movie.percent_known}%` }}
              />
            </div>
            <span className="text-xs font-mono font-semibold text-muted-foreground">
              {Math.floor(movie.percent_known)}%
            </span>
          </div>
        </div>
      </div>
    </Card>
  );
}
