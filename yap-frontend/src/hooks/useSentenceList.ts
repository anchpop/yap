import { useState, useCallback, useMemo } from "react";
import type { SentenceListSelection } from "../../../yap-frontend-rs/pkg";

export type SentenceList =
  | { type: "essential" }
  | { type: "movie"; movieId: string }
  | { type: "pimsleur"; level: number; lesson: number };

export function sentenceListSelectionToSentenceList(sl: SentenceListSelection | undefined): SentenceList {
  if (sl && sl.type === "Movie") {
    return { type: "movie", movieId: sl.id };
  }
  if (sl && sl.type === "PimsleurLesson") {
    return { type: "pimsleur", level: sl.level, lesson: sl.lesson };
  }
  return { type: "essential" };
}

export function sentenceListToSelection(sentenceList: SentenceList): SentenceListSelection | undefined {
  switch (sentenceList.type) {
    case "movie":
      return { type: "Movie" as const, id: sentenceList.movieId };
    case "pimsleur":
      return {
        type: "PimsleurLesson" as const,
        level: sentenceList.level,
        lesson: sentenceList.lesson,
      };
    case "essential":
      return undefined;
  }
}

export function useSentenceList(deckSentenceList: SentenceListSelection | undefined) {
  const [sentenceListOverride, setSentenceListOverride] = useState<SentenceList | null>(null);

  const baseSentenceList = useMemo(() => sentenceListSelectionToSentenceList(deckSentenceList), [deckSentenceList]);

  const sentenceList = sentenceListOverride ?? baseSentenceList;

  const setSentenceList = useCallback((sl: SentenceList) => {
    setSentenceListOverride(sl);
  }, []);

  return { sentenceList, setSentenceList };
}
