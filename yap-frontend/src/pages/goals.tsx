import { useNavigate } from "react-router-dom";
import { useOutletContext } from "react-router-dom";
import type { AppContextType } from "@/App";
import { TopPageLayout } from "@/components/TopPageLayout";
import { Card } from "@/components/ui/card";
import { useDeck } from "@/App";
import { languageToIso6391 } from "@/lib/utils";
import { getMovieMetadata } from "@/lib/movie-cache";
import { getPosterDataUrl } from "@/lib/poster-utils";
import { goalSelectionToGoal, goalToGoalSelection, type Goal } from "@/hooks/useGoal";
import { useWeapon } from "@/weapon";
import { match, P } from "ts-pattern";
import { Check, ChevronDown, Headphones, Sparkles } from "lucide-react";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";

const INITIAL_MOVIES_SHOWN = 5;
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";

export function GoalsPage() {
  const { userInfo } = useOutletContext<AppContextType>();
  const deckState = useDeck();
  const navigate = useNavigate();
  const weapon = useWeapon();
  const goal = deckState?.type === "deck" && deckState.deck
    ? goalSelectionToGoal(deckState.deck.get_goal())
    : ({ type: "essential" } as Goal);
  const [pimsleurAcknowledged, setPimsleurAcknowledged] = useState(
    () => localStorage.getItem("yap-pimsleur-acknowledged") === "true"
  );
  const [showAllMovies, setShowAllMovies] = useState(false);

  useEffect(() => {
    if (deckState?.type === "noLanguageSelected") {
      navigate("/", { replace: true });
    }
  }, [deckState, navigate]);

  return match(deckState)
    .with(
      { type: "deck", deck: P.not(P.nullish) },
      ({ deck, targetLanguage }) => {
        const setGoalAndNavigate = (g: Goal) => {
          const event = deck.set_goal(goalToGoalSelection(g));
          weapon.add_deck_event(event);
          window.scrollTo(0, 0);
          navigate("/learn");
        };
        const targetLanguageIso = languageToIso6391(targetLanguage);

        const movieStats = deck.get_movie_stats();
        const movieIds = movieStats.map((s) => s.id);
        const metadata = getMovieMetadata(deck, movieIds);
        const metadataMap = new Map(metadata.map((m) => [m.id, m]));
        const moviesWithMetadata = movieStats
          .map((stat) => ({
            ...stat,
            ...(metadataMap.get(stat.id) || {}),
          }))
          .filter((m) => m.original_language === targetLanguageIso);

        const pimsleurStats = deck.get_pimsleur_stats();

        const tierInfo = deck.get_current_tier();
        const overallPercentKnown = deck.get_percent_of_words_known() * 100;

        return (
          <TopPageLayout
            userInfo={userInfo}
            headerProps={{
              backButton: {
                label: "Goal",
                onBack: () => navigate("/learn"),
              },
              title: "Choose a Goal",
            }}
          >
            <div className="space-y-4 max-w-lg mx-auto w-full">
              <p className="text-sm text-muted-foreground text-center">
                Pick a goal to focus your learning on. Your cards will be
                tailored to help you reach it.
              </p>

              {/* Frequent French */}
              <GoalCard
                selected={goal.type === "essential"}
                onClick={() => setGoalAndNavigate({ type: "essential" })}
                title={`Frequent ${targetLanguage}`}
                description={`Currently on: ${tierInfo.name} ${targetLanguage} (tier ${tierInfo.tier})`}
                percentKnown={overallPercentKnown}
                posterUrl="/essential-course.webp"
              />

              {/* Movie goals */}
              <h3 className="text-lg font-semibold mt-6">Movies</h3>
              <p className="text-sm text-muted-foreground">
                Focus on vocabulary from a specific movie. You can usually
                watch comfortably once you know 95% of the words.
              </p>

              {(() => {
                const visibleMovies = showAllMovies
                  ? moviesWithMetadata
                  : moviesWithMetadata.slice(0, INITIAL_MOVIES_SHOWN);
                const hiddenCount = moviesWithMetadata.length - INITIAL_MOVIES_SHOWN;

                return (
                  <>
                    <div className="space-y-2">
                      {visibleMovies.map((movie) => {
                        const posterDataUrl = getPosterDataUrl(movie.poster_bytes);
                        const isSelected =
                          goal.type === "movie" && goal.movieId === movie.id;

                        return (
                          <GoalCard
                            key={movie.id}
                            selected={isSelected}
                            onClick={() => setGoalAndNavigate({ type: "movie", movieId: movie.id })}
                            title={movie.title ?? movie.id}
                            description={movie.year ? `${movie.year}` : undefined}
                            percentKnown={movie.percent_known}
                            done={movie.all_available_learned}
                            posterUrl={posterDataUrl}
                          />
                        );
                      })}
                    </div>
                    {!showAllMovies && hiddenCount > 0 && (
                      <button
                        onClick={() => setShowAllMovies(true)}
                        className="w-full py-3 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors duration-200 font-medium rounded-md border border-border"
                      >
                        Show {hiddenCount} more {hiddenCount === 1 ? "movie" : "movies"}
                      </button>
                    )}
                  </>
                );
              })()}

              {/* Pimsleur goals */}
              {pimsleurStats.length > 0 && (
                <>
                  <h3 className="text-lg font-semibold mt-6">Pimsleur Lessons</h3>
                  {!pimsleurAcknowledged ? (
                    <div className="flex flex-col items-center gap-3 py-4 text-center">
                      <p className="text-sm text-muted-foreground">
                        Yap has word lists for Pimsleur, but is not affiliated with Pimsleur in any way.
                      </p>
                      <Button
                        variant="default"
                        onClick={() => {
                          localStorage.setItem("yap-pimsleur-acknowledged", "true");
                          setPimsleurAcknowledged(true);
                        }}
                      >
                        I understand
                      </Button>
                    </div>
                  ) : (
                    <>
                      <p className="text-sm text-muted-foreground">
                        Focus on vocabulary from a specific Pimsleur lesson.
                      </p>

                      {(() => {
                        const levels = [...new Set(pimsleurStats.map(l => l.level))].sort((a, b) => a - b);
                        return levels.map((level) => {
                          const units = pimsleurStats.filter(l => l.level === level);
                          return (
                            <Collapsible key={level}>
                              <CollapsibleTrigger className="flex items-center gap-2 mt-4 w-full group">
                                <h4 className="text-sm font-semibold">Level {level}</h4>
                                <ChevronDown className="h-3 w-3 text-muted-foreground transition-transform group-data-[state=open]:rotate-180" />
                              </CollapsibleTrigger>
                              <CollapsibleContent>
                                <div className="space-y-2 mt-2">
                                  {units.map((lesson) => {
                                    const isSelected =
                                      goal.type === "pimsleur" &&
                                      goal.level === lesson.level &&
                                      goal.lesson === lesson.lesson;

                                    return (
                                      <GoalCard
                                        key={`pimsleur-${lesson.level}-${lesson.lesson}`}
                                        selected={isSelected}
                                        onClick={() => setGoalAndNavigate({
                                          type: "pimsleur",
                                          level: lesson.level,
                                          lesson: lesson.lesson,
                                        })}
                                        title={`Lesson ${lesson.lesson}`}
                                        percentKnown={lesson.percent_known}
                                        done={lesson.all_available_learned}
                                        icon={<Headphones className="h-5 w-5 text-muted-foreground" />}
                                      />
                                    );
                                  })}
                                </div>
                              </CollapsibleContent>
                                </Collapsible>
                              );
                            });
                          })()}
                        </>
                      )}
                </>
              )}
            </div>
          </TopPageLayout>
        );
      }
    )
    .otherwise(() => (
      <TopPageLayout userInfo={userInfo}>
        <div className="flex-1 flex items-center justify-center">
          <p className="text-muted-foreground animate-fade-in-delayed">
            Loading...
          </p>
        </div>
      </TopPageLayout>
    ));
}

function GoalCard({
  selected,
  onClick,
  title,
  description,
  percentKnown,
  done,
  posterUrl,
  icon,
}: {
  selected: boolean;
  onClick: () => void;
  title: string;
  description?: string;
  percentKnown: number;
  done?: boolean;
  posterUrl?: string | null;
  icon?: React.ReactNode;
}) {
  return (
    <Card
      animate
      className={`p-0 overflow-hidden cursor-pointer transition-all hover:scale-[1.01] ${
        selected
          ? "ring-2 ring-primary"
          : "hover:ring-1 hover:ring-muted-foreground/30"
      }`}
      onClick={onClick}
    >
      <div className="flex items-center gap-3 p-3">
        {posterUrl && (
          <img
            src={posterUrl}
            alt={title}
            className="w-10 h-15 object-cover rounded-md flex-shrink-0 opacity-90 saturate-70 dark:opacity-70 dark:saturate-80"
          />
        )}
        {!posterUrl && (
          <div className="w-10 h-10 rounded-md bg-muted flex items-center justify-center flex-shrink-0">
            {icon ?? <Sparkles className="h-5 w-5 text-muted-foreground" />}
          </div>
        )}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h4 className="font-semibold text-sm truncate">{title}</h4>
            {selected && (
              <Check className="h-4 w-4 text-primary flex-shrink-0" />
            )}
          </div>
          {description && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
          <div className="flex items-center gap-2 mt-1">
            <div className="flex-1 h-1.5 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-muted-foreground transition-all duration-300"
                style={{ width: `${percentKnown}%` }}
              />
            </div>
            <span className="text-xs font-mono text-muted-foreground">
              {done ? "Done!" : `${Math.floor(percentKnown)}%`}
            </span>
          </div>
        </div>
      </div>
    </Card>
  );
}
