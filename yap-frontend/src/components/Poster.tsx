import { useRef, useState, useEffect } from "react";
import { getPosterDataUrl } from "@/lib/poster-utils";
import type { Deck } from "../../../yap-frontend-rs/pkg";

interface PosterProps {
  movieId: string;
  deck: Deck;
  alt: string | undefined;
}

export function Poster({ movieId, deck, alt }: PosterProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [posterDataUrl, setPosterDataUrl] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    // Reset state when movieId/deck changes
    setLoaded(false);
    setPosterDataUrl(null);
  }, [movieId, deck]);

  useEffect(() => {
    if (loaded) return;

    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          const bytes = deck.get_movie_poster(movieId);
          setPosterDataUrl(getPosterDataUrl(bytes));
          setLoaded(true);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [movieId, deck, loaded]);

  if (!loaded) {
    return <div ref={ref} className="w-full h-full" />;
  }

  if (!posterDataUrl) {
    return (
      <div className="w-full h-full flex items-center justify-center text-4xl">
        🎬
      </div>
    );
  }

  return (
    <img src={posterDataUrl} alt={alt} className="w-full h-full object-cover" />
  );
}
