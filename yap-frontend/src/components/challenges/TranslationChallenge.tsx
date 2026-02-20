import {
  useState,
  useEffect,
  useRef,
  useCallback,
  forwardRef,
  useImperativeHandle,
  useMemo,
} from "react";
import { getMovieMetadata } from "@/lib/movie-cache";
import {
  type TranslateComprehensibleSentence,
  type Lexeme,
  type Literal,
  type TargetToNativeWord,
  type ProperNounDefinition,
  autograde_translation,
  find_closest_translation,
  type Language,
  type Course,
  type Deck,
  type Heteronym,
} from "../../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  motion,
  useMotionValue,
  useTransform,
  useAnimation as animationControls,
  type PanInfo,
} from "framer-motion";
import { Check, X, MoreVertical } from "lucide-react";
import { AudioButton } from "../AudioButton";
import { ReportIssueModal } from "./ReportIssueModal";
import { FeedbackDisplay } from "@/components/FeedbackDisplay";
import { playSoundEffect } from "@/lib/sound-effects";
import { useBackground } from "../BackgroundShader";
import { MoviePosterCard } from "./MoviePosterCard";
import { cn } from "@/lib/utils";
import { Textarea } from "../ui/textarea";

interface SentenceChallengeProps {
  sentence: TranslateComprehensibleSentence<string>;
  onComplete: (
    grade:
      | { wordStatuses: [Lexeme<string>, boolean | null][] }
      | { perfect: string | null },
    heteronymsTapped: Heteronym<string>[],
    submission: string
  ) => void;
  unique_target_language_lexeme_definitions: [
    Lexeme<string>,
    TargetToNativeWord[]
  ][];
  accessToken: string | undefined;
  targetLanguage: Language;
  nativeLanguage: Language;
  autoplayed: boolean;
  setAutoplayed: () => void;
  deck: Deck;
}

interface ChallengeSentenceProps {
  literals: Literal<string>[];
  onWordTap: (index: number) => void;
  wordStatuses?: [Lexeme<string>, boolean | null][];
  isPerfect?: boolean;
  tappedWords: Set<number>;
  uniqueTargetLanguageLexemes: Lexeme<string>[];
}

interface SwipeableWordProps {
  lexeme: Lexeme<string>;
  aliased: boolean;
  onSwipe: (lexeme: Lexeme<string>, remembered: boolean) => void;
  isSelected?: boolean;
  status?: boolean | null; // true = remembered, false = forgot, null = not graded
}

export interface SwipeableWordHandle {
  handleButtonClick: (remembered: boolean) => void;
}

const SwipeableWord = forwardRef<SwipeableWordHandle, SwipeableWordProps>(
  ({ lexeme, aliased, onSwipe, isSelected = false, status = null }, ref) => {
    const x = useMotionValue(0);
    const controls = animationControls();
    const { bumpBackground } = useBackground();

    const background = useTransform(
      x,
      [-150, 0, 150],
      ["rgba(239, 68, 68, 0.2)", "rgba(0, 0, 0, 0)", "rgba(34, 197, 94, 0.2)"]
    );

    const handleDragEnd = async (
      _event: MouseEvent | TouchEvent | PointerEvent,
      info: PanInfo
    ) => {
      const velocityThreshold = 5;
      const positionThreshold = 50;

      if (
        info.velocity.x < -velocityThreshold ||
        (info.velocity.x > velocityThreshold &&
          info.offset.x < -positionThreshold)
      ) {
        // Swiped left - forgot
        await controls.start({ x: -60 });
        onSwipe(lexeme, false);
      } else if (
        info.velocity.x > velocityThreshold ||
        (info.velocity.x < -velocityThreshold &&
          info.offset.x > positionThreshold)
      ) {
        // Swiped right - remembered
        await controls.start({ x: 60 });
        onSwipe(lexeme, true);
      }
    };

    const handleButtonClick = useCallback(
      async (remembered: boolean) => {
        bumpBackground(30.0);
        if (remembered) {
          await controls.start({ x: 60 });
          onSwipe(lexeme, true);
        } else {
          await controls.start({ x: -60 });
          onSwipe(lexeme, false);
        }
      },
      [controls, onSwipe, lexeme, bumpBackground]
    );

    useImperativeHandle(
      ref,
      () => ({
        handleButtonClick,
      }),
      [handleButtonClick]
    );

    // Set initial position based on status
    useEffect(() => {
      if (status === true) {
        // Remembered - move right
        controls.start({ x: 60 });
      } else if (status === false) {
        // Forgot - move left
        controls.start({ x: -60 });
      } else {
        // Not graded - center
        controls.start({ x: 0 });
      }
    }, [status, controls]);

    return (
      <motion.div
        className="relative flex items-center gap-3"
        initial={{ opacity: 0, scale: 0.8 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.2 }}
        data-word-index
      >
        {/* Left button - Forgot */}
        <button
          onClick={() => handleButtonClick(false)}
          className="p-2 rounded-full hover:bg-green-500/10 transition-colors"
          aria-label="Mark as Forgot"
        >
          <X className="w-5 h-5 text-red-500" />
        </button>

        {/* Swipeable word container */}
        <div className="flex-1 relative overflow-hidden">
          <motion.div
            drag="x"
            dragConstraints={{ left: -60, right: 60 }}
            onDragEnd={handleDragEnd}
            style={{ x, background }}
            animate={controls}
            className={`relative px-6 py-3 bg-card border rounded-lg cursor-grab active:cursor-grabbing select-none ${
              isSelected ? "ring-2 ring-primary" : ""
            }`}
          >
            <p className="text-lg font-medium text-center">
              {lexeme.type === "Heteronym" ? lexeme.heteronym.word : lexeme.phrase}
              <span className="text-sm text-muted-foreground">
                {aliased
                  ? lexeme.type === "Heteronym"
                    ? ` (${lexeme.heteronym.pos})`
                    : ""
                  : ""}
              </span>
            </p>
          </motion.div>
        </div>

        {/* Right button - Remembered */}
        <button
          onClick={() => handleButtonClick(true)}
          className="p-2 rounded-full hover:bg-red-500/10 transition-colors"
          aria-label="Mark as remembered"
        >
          <Check className="w-5 h-5 text-green-500" />
        </button>
      </motion.div>
    );
  }
);

SwipeableWord.displayName = "SwipeableWord";

function YourTranslation({ userTranslation }: { userTranslation: string }) {
  return (
    <div className="rounded-lg p-4 border">
      <p className="text-sm font-medium mb-1">Your translation:</p>
      <p className="text-lg font-medium">{userTranslation}</p>
    </div>
  );
}

function CorrectTranslation({ sentence }: { sentence: string }) {
  return (
    <div className="bg-green-500/10 rounded-lg p-4 border border-green-500/20">
      <p className="text-sm font-medium text-green-600 dark:text-green-400 mb-1">
        Correct translation:
      </p>
      <p className="text-lg font-medium">{sentence}</p>
    </div>
  );
}

function FeedbackSkeleton() {
  return (
    <div className="space-y-4 mt-4 animate-feedback-in">
      <div className="space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    </div>
  );
}

function AutogradeError() {
  return (
    <div
      className={`rounded-lg p-4 border bg-yellow-500/10 border-yellow-500/20`}
    >
      <p
        className={`text-sm font-medium mb-1 text-yellow-600 dark:text-yellow-400`}
      >
        Your submission could not be graded automatically. Please grade the
        words manually below.
      </p>
    </div>
  );
}

interface WordStatusesProps {
  selectedWordIndex: number;
  sentence: TranslateComprehensibleSentence<string>;
  setSelectedWordIndex: (index: number) => void;
  wordStatuses: [Lexeme<string>, boolean | null][];
  wordRefs: React.RefObject<Map<number, SwipeableWordHandle>>;
  handleWordSwipe: (lexeme: Lexeme<string>, remembered: boolean) => void;
  definitions: [Lexeme<string>, TargetToNativeWord[]][];
}

function WordDefinition({
  lexeme,
  definitions,
}: {
  lexeme: Lexeme<string>;
  definitions: TargetToNativeWord[];
}) {
  if (!definitions || definitions.length === 0) {
    return null;
  }

  const lexemeText =
    lexeme.type === "Heteronym" ? lexeme.heteronym.word : lexeme.phrase;

  return (
    <div className="p-3 border border-card/50 bg-card/30 rounded-md">
      <p className="text-sm font-semibold">{lexemeText}:</p>
      <ul className="list-disc list-inside text-sm">
        {definitions.map((def, i) => (
          <li key={i}>
            {def.native}
            {def.note && (
              <span className="text-xs text-muted-foreground">
                {" "}
                ({def.note})
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ProperNounDefinitionCard({
  words,
  type_singular,
  type_plural,
}: {
  words: string[];
  type_singular: string;
  type_plural: string;
}) {
  return (
    <div className="p-3 border border-card/50 bg-card/30 rounded-md">
      <span className="font-semibold">
        {words.map((word, index) => (
          <>
            {word}
            <span className="text-muted-foreground">
              {index < words.length - 1
                ? index === words.length - 2
                  ? " and "
                  : ", "
                : ""}
            </span>
          </>
        ))}
      </span>
      <span className="text-muted-foreground">
        : {words.length === 1 ? type_singular : type_plural}
      </span>
    </div>
  );
}

function ProperNounDefinitions({
  definitions,
}: {
  definitions: [string, ProperNounDefinition][];
}) {
  if (!definitions || definitions.length === 0) {
    return null;
  }

  // Group proper nouns by type
  const personNames: string[] = [];
  const placeNames: string[] = [];
  const organizationNames: string[] = [];
  const transliterations: Array<[string, string]> = [];
  const transliterationAndDescription: Array<[string, string, string]> = [];
  const descriptionOnly: Array<[string, string]> = [];

  definitions.forEach(([noun, def]) => {
    if (def.learner_native_language_translation === noun) {
      if (def.description) {
        descriptionOnly.push([noun, def.description]);
      } else if (def.is_person_name) {
        personNames.push(def.learner_native_language_translation);
      } else if (def.is_place_name) {
        placeNames.push(def.learner_native_language_translation);
      } else if (def.is_organization_name) {
        organizationNames.push(def.learner_native_language_translation);
      }
    } else {
      if (def.description) {
        transliterationAndDescription.push([
          noun,
          def.learner_native_language_translation,
          def.description,
        ]);
      } else {
        transliterations.push([noun, def.learner_native_language_translation]);
      }
    }
  });

  return (
    <div className="text-sm space-y-1">
      {personNames.length > 0 && (
        <ProperNounDefinitionCard
          words={personNames}
          type_singular="person"
          type_plural="people"
        />
      )}
      {placeNames.length > 0 && (
        <ProperNounDefinitionCard
          words={placeNames}
          type_singular="place"
          type_plural="places"
        />
      )}
      {organizationNames.length > 0 && (
        <ProperNounDefinitionCard
          words={organizationNames}
          type_singular="organization"
          type_plural="organizations"
        />
      )}
      {transliterationAndDescription.map(
        ([noun, transliteration, description]) => (
          <ProperNounDefinitionCard
            words={[noun]}
            type_singular={`${transliteration} (${description})`}
            type_plural=""
          />
        )
      )}
      {transliterations.map(([noun, transliteration]) => (
        <ProperNounDefinitionCard
          words={[noun]}
          type_singular={transliteration}
          type_plural=""
        />
      ))}
      {descriptionOnly.map(([noun, description]) => (
        <ProperNounDefinitionCard
          words={[noun]}
          type_singular={description}
          type_plural=""
        />
      ))}
    </div>
  );
}

function WordStatuses({
  selectedWordIndex,
  sentence,
  setSelectedWordIndex,
  wordStatuses,
  wordRefs,
  handleWordSwipe,
}: WordStatusesProps) {
  const [isAnswerOpen, setIsAnswerOpen] = useState(false);

  // Initialize selection when collapsible opens
  useEffect(() => {
    if (
      isAnswerOpen &&
      selectedWordIndex === -1 &&
      sentence.unique_target_language_lexemes.length > 0
    ) {
      setSelectedWordIndex(0);
    }
  }, [
    isAnswerOpen,
    selectedWordIndex,
    sentence.unique_target_language_lexemes.length,
    setSelectedWordIndex,
  ]);

  return (
    <Collapsible open={isAnswerOpen} onOpenChange={setIsAnswerOpen}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between p-0">
          <span className="text-sm font-medium">Grade Words</span>
          <span className="text-xs text-muted-foreground">
            {isAnswerOpen ? "Hide" : "Show"}
          </span>
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="text-center space-y-1">
          <p className="text-sm font-medium text-muted-foreground pb-2">
            Mark as remembered (✓) or forgot (✗). Tap a word to see its
            definition.
          </p>
        </div>

        <div className="space-y-2">
          {wordStatuses.map(([lexeme, status], index) => (
            <div key={index}>
              <SwipeableWord
                ref={(el) => {
                  if (el) {
                    wordRefs.current.set(index, el);
                  } else {
                    wordRefs.current.delete(index);
                  }
                }}
                lexeme={lexeme}
                aliased={false} // TODO: need to fix this
                onSwipe={handleWordSwipe}
                isSelected={selectedWordIndex === index}
                status={status}
              />
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function ChallengeSentence({
  literals,
  wordStatuses,
  isPerfect,
  onWordTap,
  tappedWords,
}: ChallengeSentenceProps) {
  // Helper function to get the color class for a literal
  const getLiteralColorClass = (literal: Literal<string>, i: number) => {
    // If perfect, all literals are green
    if (isPerfect) {
      return "text-green-600 dark:text-green-400";
    }

    const isHeteronym = (literal.word.word_type as { type?: string })?.type === "Heteronym";

    if (isHeteronym && tappedWords.has(i)) {
      return "text-yellow-500 dark:text-yellow-400"; // Color for tapped words
    }

    // If no word statuses, use default color
    if (!wordStatuses || !isHeteronym) {
      return "";
    }

    // Find if this literal belongs to any of the graded lexemes
    const lexeme = literal.word.word_type as unknown as Lexeme<string>;
    const statusEntry = wordStatuses.find(
      ([l]) => JSON.stringify(l) === JSON.stringify(lexeme)
    );
    if (statusEntry) {
      const status = statusEntry[1];
      if (status === true) return "text-green-600 dark:text-green-400";
      if (status === false) return "text-red-600 dark:text-red-400";
    }

    return "";
  };

  return (
    <h2 className="text-2xl font-semibold">
      {literals.map((literal, i) => {
        const colorClass = getLiteralColorClass(literal, i);
        const isHeteronym = (literal.word.word_type as { type?: string })?.type === "Heteronym";

        return (
          <>
            <span
              key={i}
              className={cn(
                colorClass,
                isHeteronym
                  ? "cursor-pointer underline-offset-3 underline decoration-dotted hover:decoration-solid hover:decoration-3 transition-transform hover:scale-105 inline-block"
                  : ""
              )}
              onClick={() => {
                if (isHeteronym) {
                  onWordTap(i);
                }
              }}
            >
              {literal.word.text}
            </span>
            {literal.whitespace}
          </>
        );
      })}
    </h2>
  );
}

export function TranslationChallenge({
  sentence,
  onComplete,
  unique_target_language_lexeme_definitions,
  accessToken,
  targetLanguage,
  nativeLanguage,
  autoplayed,
  setAutoplayed,
  deck,
}: SentenceChallengeProps) {
  "use memo";
  const [userTranslation, setUserTranslation] = useState("");

  const movieData = useMemo(() => {
    if (!sentence.movie_titles || sentence.movie_titles.length === 0) {
      return [];
    }
    const movieIds = sentence.movie_titles.map(([id]) => id);
    return getMovieMetadata(deck, movieIds);
  }, [sentence.movie_titles, deck]);
  const [correctTranslation, setCorrectTranslation] = useState(
    sentence.native_translations[0]
  );
  const [selectedWordIndex, setSelectedWordIndex] = useState<number>(-1);
  const [showReportModal, setShowReportModal] = useState(false);
  const [tappedWords, setTappedWords] = useState<Set<number>>(new Set());
  const [grade, setGrade] = useState<
    | {
        graded:
          | {
              wordStatuses: [Lexeme<string>, boolean | null][];
              encouragement?: string;
              explanation?: string;
              autogradingError?: string;
            }
          | {
              perfect: string | null;
              encouragement?: string;
              explanation?: string;
            };
      }
    | { grading: null }
    | null
  >(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const wordRefs = useRef<Map<number, SwipeableWordHandle>>(new Map());
  const { bumpBackground } = useBackground();

  // No need for useEffect to reset state - the component gets a new key when the challenge changes,
  // causing React to unmount and remount it with fresh state

  const handleWordTap = (index: number) => {
    if (!grade) {
      setTappedWords((prev) => new Set(prev).add(index));
    }
  };

  const canContinue =
    grade &&
    "graded" in grade &&
    ("perfect" in grade.graded ||
      grade.graded.wordStatuses.every(
        ([lexeme, status]) =>
          JSON.stringify(lexeme) !==
            JSON.stringify(sentence.primary_expression) || status !== null
      ));

  // Focus when input should be visible (component mount or when returning to input)
  useEffect(() => {
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 100);
    return () => clearTimeout(timer);
  }, [sentence.target_language]);

  const handleCheckAnswer = useCallback(async () => {
    if (userTranslation.trim()) {
      bumpBackground(30.0);
      // Use Rust function to find closest match with normalization and Levenshtein distance
      const closest =
        find_closest_translation(
          userTranslation,
          sentence.native_translations,
          nativeLanguage
        ) ?? sentence.native_translations[0];
      setCorrectTranslation(closest);
      setGrade({ grading: null });

      try {
        const course: Course = {
          targetLanguage: targetLanguage,
          nativeLanguage: nativeLanguage,
        };

        const response = await autograde_translation(
          sentence.target_language,
          userTranslation,
          sentence.native_translations,
          sentence.primary_expression,
          sentence.unique_target_language_lexemes,
          accessToken,
          course
        );

        const encouragement = response.encouragement;
        const explanation = response.explanation;
        let finalWordStatuses: [Lexeme<string>, boolean | null][] = [];

        playSoundEffect("aiDoneGrading");

        if (response.expressions_forgot.length === 0) {
          setGrade({ graded: { perfect: null, encouragement, explanation } });
          playSoundEffect("perfect");
        } else {
          finalWordStatuses = sentence.unique_target_language_lexemes.map(
            (lexeme) => {
              const lexemeStr = JSON.stringify(lexeme);
              if (
                response.expressions_remembered.some(
                  (rememberedLexeme) =>
                    JSON.stringify(rememberedLexeme) === lexemeStr
                )
              ) {
                return [lexeme, true];
              } else if (
                response.expressions_forgot.some(
                  (forgotLexeme) => JSON.stringify(forgotLexeme) === lexemeStr
                )
              ) {
                return [lexeme, false];
              }
              return [lexeme, null];
            }
          );
          setGrade({
            graded: {
              wordStatuses: finalWordStatuses,
              encouragement,
              explanation,
            },
          });
        }
      } catch (error) {
        console.error("Autograde failed:", error);
        const fallbackStatuses: [Lexeme<string>, boolean | null][] =
          sentence.unique_target_language_lexemes.map((lexeme) => {
            return [lexeme, null];
          });
        playSoundEffect("aiDoneGrading");
        setGrade({
          graded: {
            wordStatuses: fallbackStatuses,
            encouragement: undefined,
            explanation: undefined,
            autogradingError:
              error instanceof Error
                ? error.message
                : "Failed to grade automatically",
          },
        });
      }
    }
  }, [
    sentence,
    userTranslation,
    accessToken,
    targetLanguage,
    nativeLanguage,
    bumpBackground,
  ]);

  const heteronymsTapped = useMemo(() => {
    const heteronymsTapped = new Array<Heteronym<string>>();
    for (const index of tappedWords.values()) {
      const literal = sentence.target_language_literals[index];
      if (literal.word.word_type.type === "Heteronym") {
        const heteronym = literal.word.word_type as Heteronym<string>;
        heteronymsTapped.push(heteronym);
      }
    }
    return heteronymsTapped;
  }, [sentence, tappedWords]);

  const wordsToGradeManually = new Array<[Lexeme<string>, boolean | null]>();
  if (grade && "graded" in grade && "wordStatuses" in grade.graded) {
    for (const [lexeme, status] of grade.graded.wordStatuses) {
      if (
        !heteronymsTapped.some((l) => JSON.stringify(l) === JSON.stringify(lexeme))
      ) {
        wordsToGradeManually.push([lexeme, status]);
      }
    }
  }
  const definitionsToShow: Lexeme<string>[] = heteronymsTapped.map((h) => ({type: "Heteronym", heteronym: h}));
  if (grade && "graded" in grade && "wordStatuses" in grade.graded) {
    for (const [lexeme, status] of grade.graded.wordStatuses) {
      if (status === false) {
        if (
          !definitionsToShow.some(
            (l) => JSON.stringify(l) === JSON.stringify(lexeme)
          )
        ) {
          definitionsToShow.push(lexeme);
        }
      }
    }
  }

  const handleContinue = useCallback(() => {
    if (canContinue) {
      if (grade && "graded" in grade) {
        // Scroll to top when continuing
        bumpBackground(30.0);
        window.scrollTo({ top: 0, behavior: "smooth" });
        onComplete(grade.graded, heteronymsTapped, userTranslation);
      }
    }
  }, [
    canContinue,
    onComplete,
    grade,
    userTranslation,
    heteronymsTapped,
    bumpBackground,
  ]);

  const handleWordSwipe = useCallback(
    (lexeme: Lexeme<string>, remembered: boolean) => {
      setGrade((prevGrade) => {
        if (
          prevGrade &&
          "graded" in prevGrade &&
          "wordStatuses" in prevGrade.graded
        ) {
          const newStatuses = [...prevGrade.graded.wordStatuses];
          const index = newStatuses.findIndex(
            ([lexeme_other]) => lexeme_other === lexeme
          );
          if (index !== -1) {
            newStatuses[index] = [lexeme, remembered];
          }
          return {
            graded: {
              wordStatuses: newStatuses,
              encouragement: prevGrade.graded.encouragement,
              explanation: prevGrade.graded.explanation,
            },
          };
        }
        return prevGrade;
      });
    },
    []
  );

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" && grade === null) {
        e.preventDefault();
        if (userTranslation.trim()) {
          handleCheckAnswer();
        }
      } else if (e.key === "Enter" && grade && "graded" in grade) {
        e.preventDefault();
        handleContinue();
      } else if (
        e.key === "ArrowRight" &&
        grade &&
        "graded" in grade &&
        canContinue
      ) {
        e.preventDefault();
        handleContinue();
        return;
      }

      const wordStatuses =
        grade &&
        "graded" in grade &&
        "wordStatuses" in grade.graded &&
        grade.graded.wordStatuses;

      // Only handle arrow keys when words are shown and not all correct
      if (wordStatuses && wordStatuses.length > 0) {
        const lexemeCount = sentence.unique_target_language_lexemes.length;

        switch (e.key) {
          case "ArrowUp":
            e.preventDefault();
            setSelectedWordIndex((prev) => {
              if (prev <= 0) return lexemeCount - 1;
              return prev - 1;
            });
            break;

          case "ArrowDown":
            e.preventDefault();
            setSelectedWordIndex((prev) => {
              if (prev >= lexemeCount - 1) return 0;
              return prev + 1;
            });
            break;

          case "ArrowLeft":
            e.preventDefault();
            if (selectedWordIndex >= 0 && selectedWordIndex < lexemeCount) {
              const wordRef = wordRefs.current.get(selectedWordIndex);
              wordRef?.handleButtonClick(false);
            }
            break;

          case "ArrowRight":
            e.preventDefault();
            if (selectedWordIndex >= 0 && selectedWordIndex < lexemeCount) {
              const wordRef = wordRefs.current.get(selectedWordIndex);
              wordRef?.handleButtonClick(true);
            }
            break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    handleContinue,
    sentence.unique_target_language_lexemes,
    selectedWordIndex,
    handleWordSwipe,
    canContinue,
    userTranslation,
    handleCheckAnswer,
    grade,
  ]);

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div>
        <Card animate className="pt-3 pb-3 pl-3 pr-3 relative gap-2">
          <div className="space-y-6">
            <div className="text-center">
              <div className="flex items-center justify-between w-full">
                <AudioButton
                  audioRequest={sentence.audio}
                  accessToken={accessToken}
                  autoPlay={true}
                  autoplayed={autoplayed}
                  setAutoplayed={setAutoplayed}
                />

                <div className="flex flex-col items-center gap-1">
                  <ChallengeSentence
                    literals={sentence.target_language_literals}
                    onWordTap={handleWordTap}
                    wordStatuses={
                      grade &&
                      "graded" in grade &&
                      "wordStatuses" in grade.graded
                        ? grade.graded.wordStatuses
                        : undefined
                    }
                    isPerfect={
                      (grade &&
                        "graded" in grade &&
                        "perfect" in grade.graded) ??
                      undefined
                    }
                    tappedWords={tappedWords}
                    uniqueTargetLanguageLexemes={
                      sentence.unique_target_language_lexemes
                    }
                  />
                </div>

                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="ghost" size="icon" className="h-8 w-8">
                      <MoreVertical className="h-6 w-6 size--xl" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => setShowReportModal(true)}>
                      Report an Issue
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>

            {grade === null ? (
              <>
                <Textarea
                  ref={inputRef}
                  placeholder="Translation..."
                  value={userTranslation}
                  onChange={(e) => setUserTranslation(e.target.value)}
                  className="text-lg min-h-0"
                  rows={1}
                />

                <ProperNounDefinitions
                  definitions={sentence.proper_noun_definitions}
                />
              </>
            ) : (
              <div className="space-y-4 mt-4 animate-feedback-in">
                {"grading" in grade ? (
                  <>
                    <div className="space-y-2">
                      <YourTranslation userTranslation={userTranslation} />
                      <CorrectTranslation sentence={correctTranslation} />
                      <FeedbackSkeleton />
                    </div>
                  </>
                ) : "perfect" in grade.graded ? (
                  <>
                    <div className="space-y-2">
                      <CorrectTranslation sentence={correctTranslation} />

                      <FeedbackDisplay
                        encouragement={grade.graded.encouragement}
                        explanation={grade.graded.explanation}
                        perfect
                      />
                    </div>
                  </>
                ) : (
                  <>
                    <div className="space-y-2">
                      <YourTranslation userTranslation={userTranslation} />
                      <CorrectTranslation sentence={correctTranslation} />
                    </div>

                    {"autogradingError" in grade.graded &&
                      grade.graded.autogradingError && <AutogradeError />}

                    <FeedbackDisplay
                      encouragement={grade.graded.encouragement}
                      explanation={grade.graded.explanation}
                    />

                    <WordStatuses
                      wordStatuses={wordsToGradeManually}
                      wordRefs={wordRefs}
                      handleWordSwipe={handleWordSwipe}
                      selectedWordIndex={selectedWordIndex}
                      sentence={sentence}
                      setSelectedWordIndex={setSelectedWordIndex}
                      definitions={
                        sentence.unique_target_language_lexeme_definitions
                      }
                    />
                  </>
                )}
              </div>
            )}
          </div>
          <div className="space-y-2">
            {definitionsToShow.map((lexeme, i) => (
              <WordDefinition
                key={i}
                lexeme={lexeme}
                definitions={
                  unique_target_language_lexeme_definitions.find(
                    ([l]) => JSON.stringify(l) === JSON.stringify(lexeme)
                  )?.[1] || []
                }
              />
            ))}
          </div>
        </Card>

        {/* Movie posters */}
        {movieData.length > 0 && (
          <div className="mt-3 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
            {movieData.map((movie) => (
              <MoviePosterCard
                key={movie.id}
                id={movie.id}
                title={movie.title}
                year={movie.year}
                posterBytes={movie.poster_bytes}
              />
            ))}
          </div>
        )}
      </div>

      {grade === null ? (
        <Button
          onClick={handleCheckAnswer}
          className="w-full mt-4 h-14"
          size="lg"
          disabled={!userTranslation.trim()}
        >
          Check Answer
        </Button>
      ) : (
        <Button
          onClick={handleContinue}
          className="w-full h-14"
          size="lg"
          disabled={!canContinue}
        >
          {"grading" in grade ? (
            "AI is grading..."
          ) : (
            <>
              {"perfect" in grade.graded ? "Nailed it!" : "Continue"}
              <span className="ml-2 text-sm text-muted-foreground hide-keyboard-hint-mobile">
                (⏎)
              </span>
            </>
          )}
        </Button>
      )}

      <ReportIssueModal
        context={`Sentence challenge: ${JSON.stringify(sentence)}"`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  );
}
