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
import { MoviePosterGrid } from "./MoviePosterGrid";
import {
  type TranslateComprehensibleSentence,
  type Literal,
  type ProperNounDefinition,
  type LiteralGrades,
  type Gram,
  type DictionaryEntry,
  type PhrasebookDefinitionEntry,
  type TargetToNativeWord,
  autograde_translation,
  find_closest_translation,
  gram_to_display_string,
  get_app_version,
  type Language,
  type Course,
  type Deck,
  type Heteronym,
} from "../../../../yap-frontend-rs/pkg/yap_frontend_rs";

// GramDefinition is missing from the .d.ts due to a type generator bug
type GramDefinition =
  | { Dictionary: DictionaryEntry }
  | { Phrasebook: PhrasebookDefinitionEntry };
import { Badge } from "@/components/ui/badge";
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
import { cn, languageToIso6391 } from "@/lib/utils";
import { Textarea } from "../ui/textarea";
import { TargetLanguageText } from "../TargetLanguageText";

interface SentenceChallengeProps {
  sentence: TranslateComprehensibleSentence;
  onComplete: (
    grade:
      | {
          literalGrades: LiteralGrades;
          phrasesRemembered: Gram<string>[];
          phrasesForgot: Gram<string>[];
        }
      | { perfect: string | null },
    heteronymsTapped: Heteronym<string>[],
    submission: string,
    completedAtMs: number,
  ) => void;
  accessToken: string | undefined;
  targetLanguage: Language;
  nativeLanguage: Language;
  autoplayed: boolean;
  setAutoplayed: () => void;
  deck: Deck;
  totalReviewsCompleted: bigint;
}

interface ChallengeSentenceProps {
  literals: Literal<string>[];
  onWordTap: (index: number) => void;
  literalGrades?: LiteralGrades;
  isPerfect?: boolean;
  tappedWords: Set<number>;
  literalGramIndices: number[];
  tappedGramGroups: Set<number>;
  targetLanguage: Language;
}

type GradeItem =
  | {
      kind: "phrase";
      gram: Gram<string>;
      display: string;
      status: boolean | null;
    }
  | {
      kind: "literal";
      literalIndex: number;
      display: string;
      status: boolean | null;
    };

interface SwipeablePhraseProps {
  item: GradeItem;
  onSwipe: (item: GradeItem, remembered: boolean) => void;
  isSelected?: boolean;
  targetLanguage: Language;
}

export interface SwipeableWordHandle {
  handleButtonClick: (remembered: boolean) => void;
}

const SwipeablePhrase = forwardRef<SwipeableWordHandle, SwipeablePhraseProps>(
  ({ item, onSwipe, isSelected = false, targetLanguage }, ref) => {
    const status = item.status;
    const x = useMotionValue(0);
    const controls = animationControls();
    const { bumpBackground } = useBackground();

    const background = useTransform(
      x,
      [-150, 0, 150],
      ["rgba(239, 68, 68, 0.2)", "rgba(0, 0, 0, 0)", "rgba(34, 197, 94, 0.2)"],
    );

    const handleDragEnd = async (
      _event: MouseEvent | TouchEvent | PointerEvent,
      info: PanInfo,
    ) => {
      const velocityThreshold = 5;
      const positionThreshold = 50;

      if (
        info.velocity.x < -velocityThreshold ||
        (info.velocity.x > velocityThreshold &&
          info.offset.x < -positionThreshold)
      ) {
        await controls.start({ x: -60 });
        onSwipe(item, false);
      } else if (
        info.velocity.x > velocityThreshold ||
        (info.velocity.x < -velocityThreshold &&
          info.offset.x > positionThreshold)
      ) {
        await controls.start({ x: 60 });
        onSwipe(item, true);
      }
    };

    const handleButtonClick = useCallback(
      async (remembered: boolean) => {
        bumpBackground(30.0);
        if (remembered) {
          await controls.start({ x: 60 });
          onSwipe(item, true);
        } else {
          await controls.start({ x: -60 });
          onSwipe(item, false);
        }
      },
      [controls, onSwipe, item, bumpBackground],
    );

    useImperativeHandle(
      ref,
      () => ({
        handleButtonClick,
      }),
      [handleButtonClick],
    );

    useEffect(() => {
      if (status === true) {
        controls.start({ x: 60 });
      } else if (status === false) {
        controls.start({ x: -60 });
      } else {
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
        <button
          onClick={() => handleButtonClick(false)}
          className="p-2 rounded-full hover:bg-green-500/10 transition-colors"
          aria-label="Mark as Forgot"
        >
          <X className="w-5 h-5 text-red-500" />
        </button>

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
              <TargetLanguageText language={targetLanguage}>
                {item.display}
              </TargetLanguageText>
            </p>
          </motion.div>
        </div>

        <button
          onClick={() => handleButtonClick(true)}
          className="p-2 rounded-full hover:bg-red-500/10 transition-colors"
          aria-label="Mark as remembered"
        >
          <Check className="w-5 h-5 text-green-500" />
        </button>
      </motion.div>
    );
  },
);

SwipeablePhrase.displayName = "SwipeablePhrase";

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

interface PhraseStatusesProps {
  selectedPhraseIndex: number;
  setSelectedPhraseIndex: (index: number) => void;
  gradeItems: GradeItem[];
  phraseRefs: React.RefObject<Map<number, SwipeableWordHandle>>;
  handleGradeSwipe: (item: GradeItem, remembered: boolean) => void;
  openByDefault: boolean;
  targetLanguage: Language;
}

function PhraseStatuses({
  selectedPhraseIndex,
  setSelectedPhraseIndex,
  gradeItems,
  phraseRefs,
  handleGradeSwipe,
  openByDefault,
  targetLanguage,
}: PhraseStatusesProps) {
  const [isAnswerOpen, setIsAnswerOpen] = useState(openByDefault);

  useEffect(() => {
    if (isAnswerOpen && selectedPhraseIndex === -1 && gradeItems.length > 0) {
      setSelectedPhraseIndex(0);
    }
  }, [
    isAnswerOpen,
    selectedPhraseIndex,
    gradeItems.length,
    setSelectedPhraseIndex,
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
            Mark as remembered (✓) or forgot (✗).
          </p>
        </div>

        <div className="space-y-2">
          {gradeItems.map((item, index: number) => (
            <div key={index}>
              <SwipeablePhrase
                ref={(el) => {
                  if (el) {
                    phraseRefs.current.set(index, el);
                  } else {
                    phraseRefs.current.delete(index);
                  }
                }}
                item={item}
                onSwipe={handleGradeSwipe}
                isSelected={selectedPhraseIndex === index}
                targetLanguage={targetLanguage}
              />
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function ProperNounDefinitionCard({
  words,
  type_singular,
  type_plural,
  targetLanguage,
}: {
  words: string[];
  type_singular: string;
  type_plural: string;
  targetLanguage: Language;
}) {
  return (
    <div className="p-3 border border-card/50 bg-card/30 rounded-md">
      <span className="font-semibold">
        {words.map((word, index) => (
          <span key={word}>
            <TargetLanguageText language={targetLanguage}>
              {word}
            </TargetLanguageText>
            <span className="text-muted-foreground">
              {index < words.length - 1
                ? index === words.length - 2
                  ? " and "
                  : ", "
                : ""}
            </span>
          </span>
        ))}
      </span>
      <span className="text-muted-foreground">
        : {words.length === 1 ? type_singular : type_plural}
      </span>
    </div>
  );
}

export function ProperNounDefinitions({
  definitions,
  targetLanguage,
}: {
  definitions: [string, ProperNounDefinition][];
  targetLanguage: Language;
}) {
  if (!definitions || definitions.length === 0) {
    return null;
  }

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
          targetLanguage={targetLanguage}
        />
      )}
      {placeNames.length > 0 && (
        <ProperNounDefinitionCard
          words={placeNames}
          type_singular="place"
          type_plural="places"
          targetLanguage={targetLanguage}
        />
      )}
      {organizationNames.length > 0 && (
        <ProperNounDefinitionCard
          words={organizationNames}
          type_singular="organization"
          type_plural="organizations"
          targetLanguage={targetLanguage}
        />
      )}
      {transliterationAndDescription.map(
        ([noun, transliteration, description]) => (
          <ProperNounDefinitionCard
            key={noun}
            words={[noun]}
            type_singular={`${transliteration} (${description})`}
            type_plural=""
            targetLanguage={targetLanguage}
          />
        ),
      )}
      {transliterations.map(([noun, transliteration]) => (
        <ProperNounDefinitionCard
          key={noun}
          words={[noun]}
          type_singular={transliteration}
          type_plural=""
          targetLanguage={targetLanguage}
        />
      ))}
      {descriptionOnly.map(([noun, description]) => (
        <ProperNounDefinitionCard
          key={noun}
          words={[noun]}
          type_singular={description}
          type_plural=""
          targetLanguage={targetLanguage}
        />
      ))}
    </div>
  );
}

function GramDefinitionDisplay({
  definition,
  targetLanguage,
}: {
  definition: GramDefinition;
  targetLanguage: Language;
}) {
  if ("Dictionary" in definition) {
    const dict = definition.Dictionary;
    return (
      <div className="p-3 border border-card/50 bg-card/30 rounded-md space-y-2">
        <p className="text-sm font-semibold">
          <TargetLanguageText language={targetLanguage}>
            {dict.target_language_word}
          </TargetLanguageText>
          :
        </p>
        {dict.definitions.map((def: TargetToNativeWord, i: number) => (
          <div key={i}>
            <p className="text-sm">
              {def.native}
              {def.note && (
                <span className="text-xs text-muted-foreground">
                  {" "}
                  ({def.note})
                </span>
              )}
            </p>
            {def.example_sentence_target_language && (
              <div className="text-xs text-muted-foreground mt-1">
                <p className="italic">
                  <TargetLanguageText language={targetLanguage}>
                    "{def.example_sentence_target_language}"
                  </TargetLanguageText>
                </p>
                <p>"{def.example_sentence_native_language}"</p>
              </div>
            )}
          </div>
        ))}
      </div>
    );
  } else {
    const pb = definition.Phrasebook;
    return (
      <div className="p-3 border border-card/50 bg-card/30 rounded-md">
        <p className="text-sm font-semibold">
          <TargetLanguageText language={targetLanguage}>
            {pb.target_language_multi_word_term}
          </TargetLanguageText>
          :
        </p>
        <p className="text-sm">{pb.meaning}</p>
        {pb.target_language_example && (
          <div className="text-xs text-muted-foreground mt-1">
            <p className="italic">
              <TargetLanguageText language={targetLanguage}>
                "{pb.target_language_example}"
              </TargetLanguageText>
            </p>
            <p>"{pb.native_language_example}"</p>
          </div>
        )}
      </div>
    );
  }
}

function ChallengeSentence({
  literals,
  literalGrades,
  isPerfect,
  onWordTap,
  tappedWords,
  literalGramIndices,
  tappedGramGroups,
  targetLanguage,
}: ChallengeSentenceProps) {
  const getLiteralColorClass = (literal: Literal<string>, i: number) => {
    if (isPerfect) {
      return "text-green-600 dark:text-green-400";
    }

    const isHeteronym =
      (literal.word.word_type as { type?: string })?.type === "Heteronym";

    // Highlight all literals in a tapped gram group
    const gramGroup = literalGramIndices[i];
    if (gramGroup !== undefined && tappedGramGroups.has(gramGroup)) {
      return "text-yellow-500 dark:text-yellow-400";
    }

    // Also highlight individually tapped words (backwards compat)
    if (isHeteronym && tappedWords.has(i)) {
      return "text-yellow-500 dark:text-yellow-400";
    }

    if (!literalGrades || !isHeteronym) {
      return "";
    }

    const grade = literalGrades[i];
    if (grade === "Remembered") return "text-green-600 dark:text-green-400";
    if (grade === "Forgot") return "text-red-600 dark:text-red-400";

    return "";
  };

  return (
    <h2 className="text-2xl font-semibold">
      {literals.map((literal: Literal<string>, i: number) => {
        const colorClass = getLiteralColorClass(literal, i);
        const isHeteronym =
          (literal.word.word_type as { type?: string })?.type === "Heteronym";

        return (
          <span key={i}>
            <span
              className={cn(
                colorClass,
                isHeteronym
                  ? "cursor-pointer underline-offset-3 underline decoration-dotted hover:decoration-solid hover:decoration-3 transition-transform hover:scale-105 inline-block"
                  : "",
              )}
              onClick={() => {
                if (isHeteronym) {
                  onWordTap(i);
                }
              }}
            >
              <TargetLanguageText language={targetLanguage}>
                {literal.word.text}
              </TargetLanguageText>
            </span>
            {literal.whitespace}
          </span>
        );
      })}
    </h2>
  );
}

export function TranslationChallenge({
  sentence,
  onComplete,
  accessToken,
  targetLanguage,
  nativeLanguage,
  autoplayed,
  setAutoplayed,
  deck,
  totalReviewsCompleted,
}: SentenceChallengeProps) {
  "use memo";
  const [userTranslation, setUserTranslation] = useState("");

  const movieData = useMemo(() => {
    if (!sentence.movie_titles || sentence.movie_titles.length === 0) {
      return [];
    }
    const movieIds = sentence.movie_titles.map(([id]: [string, string]) => id);
    return getMovieMetadata(deck, movieIds);
  }, [sentence.movie_titles, deck]);
  const [correctTranslation, setCorrectTranslation] = useState(
    sentence.native_translations[0],
  );
  const [selectedPhraseIndex, setSelectedPhraseIndex] = useState<number>(-1);
  const [showReportModal, setShowReportModal] = useState(false);
  const [tappedWords, setTappedWords] = useState<Set<number>>(new Set());
  const STORAGE_KEY = "yap-pending-translation-grade";

  type GradeState =
    | {
        graded:
          | {
              literalGrades: LiteralGrades;
              phrasesRemembered: Gram<string>[];
              phrasesForgot: Gram<string>[];
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
    | null;

  // Try to restore a saved grade from localStorage
  const restored = useMemo(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return null;
      const saved = JSON.parse(raw);
      if (
        saved.version !== get_app_version() ||
        saved.totalReviewsCompleted !== Number(totalReviewsCompleted) ||
        JSON.stringify(saved.challenge) !== JSON.stringify(sentence)
      ) {
        localStorage.removeItem(STORAGE_KEY);
        return null;
      }
      return saved as {
        grade: GradeState;
        userTranslation: string;
        tappedWords: number[];
        completedAtMs: number;
        correctTranslation: string;
      };
    } catch {
      localStorage.removeItem(STORAGE_KEY);
      return null;
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const [grade, setGrade] = useState<GradeState>(restored?.grade ?? null);
  const completedAtMsRef = useRef<number | undefined>(restored?.completedAtMs);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const phraseRefs = useRef<Map<number, SwipeableWordHandle>>(new Map());
  const { bumpBackground } = useBackground();

  // Restore saved state
  useEffect(() => {
    if (restored) {
      setUserTranslation(restored.userTranslation);
      setTappedWords(new Set(restored.tappedWords));
      setCorrectTranslation(restored.correctTranslation);
    }
  }, [restored]);

  // Save grade to localStorage when grading completes so it survives navigation
  useEffect(() => {
    if (grade && "graded" in grade) {
      const timestamp = completedAtMsRef.current ?? Date.now();
      completedAtMsRef.current = timestamp;
      try {
        localStorage.setItem(
          STORAGE_KEY,
          JSON.stringify({
            version: get_app_version(),
            challenge: sentence,
            totalReviewsCompleted: Number(totalReviewsCompleted),
            grade,
            userTranslation,
            tappedWords: [...tappedWords],
            completedAtMs: timestamp,
            correctTranslation,
          }),
        );
      } catch {
        // localStorage full or unavailable — not critical
      }
    }
  }, [grade, sentence, userTranslation, tappedWords, correctTranslation]);

  const literalGramIndices: number[] = sentence.literal_gram_indices;
  const gramDefinitions = sentence.gram_definitions_for_lookup as (
    | GramDefinition
    | undefined
  )[];

  const handleWordTap = (index: number) => {
    if (!grade) {
      setTappedWords((prev) => new Set(prev).add(index));
    }
  };

  // Compute which gram groups have been tapped (for highlighting all words in the gram)
  const tappedGramGroups = useMemo(() => {
    const groups = new Set<number>();
    for (const i of tappedWords) {
      const group = literalGramIndices[i];
      if (group !== undefined) {
        groups.add(group);
      }
    }
    return groups;
  }, [tappedWords, literalGramIndices]);

  const gramEq = (a: Gram<string>, b: Gram<string>) =>
    JSON.stringify(a) === JSON.stringify(b);

  // Definitions to show for tapped gram groups, forgot literals, and forgot phrases
  const tappedDefinitions = useMemo(() => {
    const defs: GramDefinition[] = [];
    const seen = new Set<number>();

    const addGroup = (group: number) => {
      if (seen.has(group)) return;
      seen.add(group);
      const def = gramDefinitions[group];
      if (def) defs.push(def);
    };

    for (const group of tappedGramGroups) {
      addGroup(group);
    }

    if (grade && "graded" in grade && "literalGrades" in grade.graded) {
      // Show definitions for literals graded as "Forgot"
      grade.graded.literalGrades.forEach((literalGrade, i) => {
        if (literalGrade === "Forgot") {
          const group = literalGramIndices[i];
          if (group !== undefined) addGroup(group);
        }
      });

      // Show definitions for phrases graded as "Forgot"
      // Phrases are multiword terms separate from the sentence's gram sequence,
      // so their definitions are in phrase_definitions (parallel to unique_target_language_phrases),
      // not in gram_definitions_for_lookup.
      const phraseDefinitions = sentence.phrase_definitions as (
        | GramDefinition
        | undefined
      )[];
      for (const phrase of grade.graded.phrasesForgot) {
        const phraseIdx = sentence.unique_target_language_phrases.findIndex(
          (p) => gramEq(p, phrase),
        );
        if (phraseIdx !== -1) {
          const def = phraseDefinitions[phraseIdx];
          if (def) defs.push(def);
        }
      }
    }

    return defs;
  }, [
    tappedGramGroups,
    gramDefinitions,
    grade,
    literalGramIndices,
    sentence.phrase_definitions,
    sentence.unique_target_language_phrases,
  ]);

  // Build grade items for manual grading UI (literals first, then phrases)
  const gradeItems: GradeItem[] = useMemo(() => {
    if (!grade || !("graded" in grade) || "perfect" in grade.graded) return [];
    const items: GradeItem[] = [];

    // Add each gradable heteronym literal
    if ("literalGrades" in grade.graded) {
      const hasAutogradingError =
        "autogradingError" in grade.graded && grade.graded.autogradingError;
      sentence.target_language_literals.forEach((literal, i) => {
        if ((literal.word.word_type as { type?: string })?.type !== "Heteronym")
          return;
        const literalGrade =
          grade.graded && "literalGrades" in grade.graded
            ? grade.graded.literalGrades[i]
            : undefined;
        // When autograde failed, undefined heteronym grades need manual grading;
        // when autograde succeeded, undefined means ungradable.
        if (literalGrade === undefined && !hasAutogradingError) return;
        items.push({
          kind: "literal",
          literalIndex: i,
          display: literal.word.text,
          status:
            literalGrade === "Remembered"
              ? true
              : literalGrade === "Forgot"
                ? false
                : null,
        });
      });
    }

    // Add multiword phrases
    if ("phrasesRemembered" in grade.graded) {
      for (const gram of sentence.unique_target_language_phrases) {
        const display = gram_to_display_string(gram, targetLanguage);
        const remembered = grade.graded.phrasesRemembered.some((p) =>
          gramEq(p, gram),
        );
        const forgot = grade.graded.phrasesForgot.some((p) => gramEq(p, gram));
        items.push({
          kind: "phrase",
          gram,
          display,
          status: remembered ? true : forgot ? false : null,
        });
      }
    }

    return items;
  }, [
    grade,
    sentence.target_language_literals,
    sentence.unique_target_language_phrases,
    targetLanguage,
  ]);

  const canContinue =
    grade &&
    "graded" in grade &&
    ("perfect" in grade.graded ||
      gradeItems.some((item) => item.status !== null));

  useEffect(() => {
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 100);
    return () => clearTimeout(timer);
  }, [sentence.target_language]);

  const handleCheckAnswer = useCallback(async () => {
    if (userTranslation.trim()) {
      completedAtMsRef.current = Date.now();
      bumpBackground(30.0);
      const closest =
        find_closest_translation(
          userTranslation,
          sentence.native_translations,
          nativeLanguage,
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
          sentence.target_language_literals,
          sentence.unique_target_language_phrases,
          accessToken,
          course,
          sentence.gram_definitions_for_lookup,
          new Uint32Array(sentence.literal_gram_indices),
          sentence.phrase_definitions,
          sentence.primary_expression,
        );

        const encouragement = response.encouragement;
        const explanation = response.explanation;

        playSoundEffect("aiDoneGrading");

        if (response.autograding_error) {
          // Heuristic fallback was used — show grades for manual review
          setGrade({
            graded: {
              literalGrades: response.literal_grades,
              phrasesRemembered: response.phrases_remembered,
              phrasesForgot: response.phrases_forgot,
              encouragement,
              explanation,
              autogradingError: response.autograding_error,
            },
          });
        } else if (
          response.phrases_forgot.length === 0 &&
          !response.literal_grades.some((g) => g === "Forgot")
        ) {
          setGrade({ graded: { perfect: null, encouragement, explanation } });
          playSoundEffect("perfect");
        } else {
          setGrade({
            graded: {
              literalGrades: response.literal_grades,
              phrasesRemembered: response.phrases_remembered,
              phrasesForgot: response.phrases_forgot,
              encouragement,
              explanation,
            },
          });
        }
      } catch (error) {
        console.error("Autograde failed:", error);
        playSoundEffect("aiDoneGrading");
        setGrade({
          graded: {
            literalGrades: sentence.target_language_literals.map(
              () => undefined,
            ),
            phrasesRemembered: [],
            phrasesForgot: [],
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
    const result = new Array<Heteronym<string>>();
    for (const index of tappedWords.values()) {
      const literal = sentence.target_language_literals[index];
      if (literal.word.word_type.type === "Heteronym") {
        const heteronym = literal.word.word_type as Heteronym<string>;
        result.push(heteronym);
      }
    }
    return result;
  }, [sentence, tappedWords]);

  const handleContinue = useCallback(() => {
    if (canContinue) {
      if (grade && "graded" in grade) {
        localStorage.removeItem(STORAGE_KEY);
        bumpBackground(30.0);
        window.scrollTo({ top: 0, behavior: "smooth" });
        const completedAtMs = completedAtMsRef.current!;
        if ("perfect" in grade.graded) {
          onComplete(
            { perfect: grade.graded.perfect },
            heteronymsTapped,
            userTranslation,
            completedAtMs,
          );
        } else {
          onComplete(
            {
              literalGrades: grade.graded.literalGrades,
              phrasesRemembered: grade.graded.phrasesRemembered,
              phrasesForgot: grade.graded.phrasesForgot,
            },
            heteronymsTapped,
            userTranslation,
            completedAtMs,
          );
        }
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

  const handleGradeSwipe = useCallback(
    (item: GradeItem, remembered: boolean) => {
      setGrade((prevGrade) => {
        if (!prevGrade || !("graded" in prevGrade)) return prevGrade;

        if (item.kind === "phrase" && "phrasesRemembered" in prevGrade.graded) {
          const newRemembered = prevGrade.graded.phrasesRemembered.filter(
            (p) => !gramEq(p, item.gram),
          );
          const newForgot = prevGrade.graded.phrasesForgot.filter(
            (p) => !gramEq(p, item.gram),
          );
          if (remembered) newRemembered.push(item.gram);
          else newForgot.push(item.gram);
          return {
            graded: {
              ...prevGrade.graded,
              phrasesRemembered: newRemembered,
              phrasesForgot: newForgot,
            },
          };
        }

        if (item.kind === "literal" && "literalGrades" in prevGrade.graded) {
          const newLiteralGrades = [...prevGrade.graded.literalGrades];
          newLiteralGrades[item.literalIndex] = remembered
            ? "Remembered"
            : "Forgot";
          return {
            graded: {
              ...prevGrade.graded,
              literalGrades: newLiteralGrades,
            },
          };
        }

        return prevGrade;
      });
    },
    [],
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

      const hasGradeItems =
        grade &&
        "graded" in grade &&
        "literalGrades" in grade.graded &&
        gradeItems.length > 0;

      if (hasGradeItems) {
        const itemCount = gradeItems.length;

        switch (e.key) {
          case "ArrowUp":
            e.preventDefault();
            setSelectedPhraseIndex((prev) => {
              if (prev <= 0) return itemCount - 1;
              return prev - 1;
            });
            break;

          case "ArrowDown":
            e.preventDefault();
            setSelectedPhraseIndex((prev) => {
              if (prev >= itemCount - 1) return 0;
              return prev + 1;
            });
            break;

          case "ArrowLeft":
            e.preventDefault();
            if (selectedPhraseIndex >= 0 && selectedPhraseIndex < itemCount) {
              const ref = phraseRefs.current.get(selectedPhraseIndex);
              ref?.handleButtonClick(false);
            }
            break;

          case "ArrowRight":
            e.preventDefault();
            if (selectedPhraseIndex >= 0 && selectedPhraseIndex < itemCount) {
              const ref = phraseRefs.current.get(selectedPhraseIndex);
              ref?.handleButtonClick(true);
            }
            break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    handleContinue,
    selectedPhraseIndex,
    handleGradeSwipe,
    canContinue,
    userTranslation,
    handleCheckAnswer,
    grade,
    gradeItems.length,
  ]);

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div>
        <Card animate className="pt-3 pb-3 pl-3 pr-3 relative gap-2">
          {sentence.second_chance && (
            <Badge className="absolute -top-2 -left-2 -rotate-12 z-10 shadow-sm text-sm">
              Second Chance!
            </Badge>
          )}
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
                    literalGrades={
                      grade &&
                      "graded" in grade &&
                      "literalGrades" in grade.graded
                        ? grade.graded.literalGrades
                        : undefined
                    }
                    isPerfect={
                      (grade &&
                        "graded" in grade &&
                        "perfect" in grade.graded) ??
                      undefined
                    }
                    tappedWords={tappedWords}
                    literalGramIndices={literalGramIndices}
                    tappedGramGroups={tappedGramGroups}
                    targetLanguage={targetLanguage}
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
                  lang={languageToIso6391(nativeLanguage)}
                  placeholder="Translation..."
                  value={userTranslation}
                  onChange={(e) => setUserTranslation(e.target.value)}
                  className="text-lg min-h-0"
                  rows={1}
                />

                <ProperNounDefinitions
                  definitions={sentence.proper_noun_definitions}
                  targetLanguage={targetLanguage}
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

                    <PhraseStatuses
                      gradeItems={gradeItems}
                      phraseRefs={phraseRefs}
                      handleGradeSwipe={handleGradeSwipe}
                      selectedPhraseIndex={selectedPhraseIndex}
                      setSelectedPhraseIndex={setSelectedPhraseIndex}
                      openByDefault={
                        "autogradingError" in grade.graded &&
                        grade.graded.autogradingError !== undefined
                      }
                      targetLanguage={targetLanguage}
                    />
                  </>
                )}
              </div>
            )}
          </div>
          {tappedDefinitions.length > 0 && (
            <div className="space-y-2">
              {tappedDefinitions.map((def, i) => (
                <GramDefinitionDisplay
                  key={i}
                  definition={def}
                  targetLanguage={targetLanguage}
                />
              ))}
            </div>
          )}
        </Card>

        {/* Movie posters - hidden after grading */}
        {grade === null && (
          <MoviePosterGrid movieData={movieData} deck={deck} />
        )}
      </div>

      <div className="sticky bottom-0">
        {grade === null ? (
          <Button
            onClick={handleCheckAnswer}
            className="w-full mt-4 h-14 text-lg"
            size="lg"
            disabled={!userTranslation.trim()}
          >
            Check Answer
          </Button>
        ) : (
          <Button
            onClick={handleContinue}
            className="w-full h-14 text-lg"
            size="lg"
            disabled={!canContinue}
          >
            {"grading" in grade ? (
              "AI is grading..."
            ) : (
              <span className="relative flex items-center justify-center">
                {"perfect" in grade.graded ? "Nailed it!" : "Continue"}
                <span className="absolute left-full ml-2 text-sm text-muted-foreground hide-keyboard-hint-mobile">
                  (⏎)
                </span>
              </span>
            )}
          </Button>
        )}
      </div>

      <ReportIssueModal
        context={`Sentence challenge: ${JSON.stringify(sentence)}"`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  );
}
