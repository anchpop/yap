import { useState, useCallback, useMemo } from "react";
import type { GoalSelection } from "../../../yap-frontend-rs/pkg";

export type Goal =
  | { type: "essential" }
  | { type: "movie"; movieId: string }
  | { type: "pimsleur"; level: number; lesson: number };

export function goalSelectionToGoal(gs: GoalSelection | undefined): Goal {
  if (gs && gs.type === "Movie") {
    return { type: "movie", movieId: gs.id };
  }
  if (gs && gs.type === "PimsleurLesson") {
    return { type: "pimsleur", level: gs.level, lesson: gs.lesson };
  }
  return { type: "essential" };
}

export function goalToGoalSelection(goal: Goal): GoalSelection | undefined {
  switch (goal.type) {
    case "movie":
      return { type: "Movie" as const, id: goal.movieId };
    case "pimsleur":
      return { type: "PimsleurLesson" as const, level: goal.level, lesson: goal.lesson };
    case "essential":
      return undefined;
  }
}

export function useGoal(deckGoal: GoalSelection | undefined) {
  const [goalOverride, setGoalOverride] = useState<Goal | null>(null);

  const baseGoal = useMemo(() => goalSelectionToGoal(deckGoal), [deckGoal]);

  const goal = goalOverride ?? baseGoal;

  const setGoal = useCallback((g: Goal) => {
    setGoalOverride(g);
  }, []);

  return { goal, setGoal };
}
