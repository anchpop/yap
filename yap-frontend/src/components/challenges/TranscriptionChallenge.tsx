import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { getMovieMetadata } from "@/lib/movie-cache";
import { reportAutogradeFailure } from "@/instrument";
import { MoviePosterGrid } from "./MoviePosterGrid";
import {
  autograde_transcription,
  get_app_version,
  type TranscribeComprehensibleSentence,
  type PartGraded,
  type PartSubmitted,
  type WordGrade,
  type Language,
  type Course,
  type Deck,
  type DictionaryEntry,
  type PhrasebookDefinitionEntry,
} from "../../../../yap-frontend-rs/pkg/yap_frontend_rs";

// GramDefinition is missing from the .d.ts due to a type generator bug
type GramDefinition =
  | { Dictionary: DictionaryEntry }
  | { Phrasebook: PhrasebookDefinitionEntry };
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { languageToLangAttr } from "@/lib/utils";
import { Card } from "@/components/ui/card";
import { AudioButton } from "../AudioButton";
import { playSoundEffect } from "@/lib/sound-effects";
import { CantListenButton } from "../CantListenButton";
import { AudioErrorBanner } from "../AudioErrorBanner";
import { FeedbackDisplay } from "@/components/FeedbackDisplay";
import { AccentedCharacterKeyboard } from "../AccentedCharacterKeyboard";
import { MobileKeyboardTip } from "../MobileKeyboardTip";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { useBackground } from "../background-context";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { MoreVertical, X } from "lucide-react";
import { ReportIssueModal } from "./ReportIssueModal";
import { Skeleton } from "@/components/ui/skeleton";
import { InlineTextarea } from "../ui/textarea";
import {
  ProperNounDefinitions,
  GramDefinitionDisplay,
} from "./TranslationChallenge";
import { TargetLanguageText } from "../TargetLanguageText";
import { type BreakdownRow } from "../MorphemeBreakdown";

interface TranscriptionChallengeProps {
  challenge: TranscribeComprehensibleSentence;
  onComplete: (grade: PartGraded[], completedAtMs: number) => void;
  totalCount: number;
  accessToken: string | undefined;
  onCantListen?: () => void;
  targetLanguage: Language;
  nativeLanguage: Language;
  autoplayed: boolean;
  setAutoplayed: () => void;
  deck: Deck;
  totalReviewsCompleted: bigint;
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

function FeedbackSkeleton() {
  return (
    <div className="space-y-4 animate-feedback-in">
      <div className="space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    </div>
  );
}

type GradingState =
  | null // Not started
  | { grading: null } // Grading in progress
  | {
      graded: {
        results: PartGraded[];
        encouragement: string | undefined;
        explanation: string | undefined;
        compare: string[];
        autograding_error?: string;
      };
    };

export function TranscriptionChallenge({
  challenge,
  onComplete,
  totalCount,
  accessToken,
  onCantListen,
  targetLanguage,
  nativeLanguage,
  autoplayed,
  setAutoplayed,
  deck,
  totalReviewsCompleted,
}: TranscriptionChallengeProps) {
  const STORAGE_KEY = "yap-pending-transcription-grade";

  // Try to restore a saved grade from localStorage
  const restored = useMemo(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return null;
      const saved = JSON.parse(raw);
      if (
        saved.version !== get_app_version() ||
        saved.totalReviewsCompleted !== Number(totalReviewsCompleted) ||
        JSON.stringify(saved.challenge) !== JSON.stringify(challenge)
      ) {
        localStorage.removeItem(STORAGE_KEY);
        return null;
      }
      return saved as {
        gradingState: GradingState;
        userInputs: [number, string][];
        completedAtMs: number;
      };
    } catch {
      localStorage.removeItem(STORAGE_KEY);
      return null;
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const [userInputs, setUserInputs] = useState<Map<number, string>>(
    restored ? new Map(restored.userInputs) : new Map(),
  );
  const [audioError, setAudioError] = useState(false);

  const movieData = useMemo(() => {
    if (!challenge.movie_titles || challenge.movie_titles.length === 0) {
      return [];
    }
    const movieIds = challenge.movie_titles.map(([id]) => id);
    return getMovieMetadata(deck, movieIds);
  }, [challenge.movie_titles, deck]);
  const [gradingState, setGradingState] = useState<GradingState>(restored?.gradingState ?? null);
  const gradingGenerationRef = useRef(0);
  const completedAtMsRef = useRef<number | undefined>(restored?.completedAtMs);
  const [showReportModal, setShowReportModal] = useState(false);
  const [isTranslationRevealed, setIsTranslationRevealed] = useState(false);
  const [focusedInputIndex, setFocusedInputIndex] = useState<number | null>(
    null,
  );
  const [shiftHeld, setShiftHeld] = useState(false);
  const inputRefs = useRef<(HTMLTextAreaElement | null)[]>([]);
  const { bumpBackground } = useBackground();

  // Find indices of words that should be blanks
  const blankIndices: number[] = useMemo(() => {
    const blankIndices: number[] = [];
    challenge.parts.forEach((item, index) => {
      if (item.type === "AskedToTranscribe") {
        blankIndices.push(index);
      }
    });
    return blankIndices;
  }, [challenge]);

  // Definitions for literals graded as wrong (anything beyond Perfect / CorrectWithTypo).
  // Deduplicated per gram group, preserving order.
  const wrongGramEntries = useMemo(() => {
    if (!gradingState || !("graded" in gradingState)) return [];
    const gramDefinitions = challenge.gram_definitions_for_lookup as (
      | GramDefinition
      | undefined
    )[];
    const gramBreakdowns = challenge.gram_breakdowns_for_lookup as (
      | BreakdownRow[]
      | null
      | undefined
    )[];
    const partGramIndices = challenge.part_gram_indices as number[][];
    const entries: {
      definition: GramDefinition;
      breakdown: BreakdownRow[] | null | undefined;
    }[] = [];
    const seen = new Set<number>();
    gradingState.graded.results.forEach((result, partIdx) => {
      if (result.type !== "AskedToTranscribe") return;
      result.parts.forEach((p, litIdx) => {
        const t = p.grade.type;
        if (t === "Perfect" || t === "CorrectWithTypo") return;
        const gramIdx = partGramIndices[partIdx]?.[litIdx];
        if (gramIdx === undefined || seen.has(gramIdx)) return;
        seen.add(gramIdx);
        const def = gramDefinitions[gramIdx];
        if (def)
          entries.push({ definition: def, breakdown: gramBreakdowns[gramIdx] });
      });
    });
    return entries;
  }, [gradingState, challenge]);

  // Save grade to localStorage when grading completes so it survives navigation
  useEffect(() => {
    if (gradingState && "graded" in gradingState) {
      const timestamp = completedAtMsRef.current ?? Date.now();
      completedAtMsRef.current = timestamp;
      try {
        localStorage.setItem(
          STORAGE_KEY,
          JSON.stringify({
            version: get_app_version(),
            challenge,
            totalReviewsCompleted: Number(totalReviewsCompleted),
            gradingState,
            userInputs: [...userInputs.entries()],
            completedAtMs: timestamp,
          }),
        );
      } catch {
        // localStorage full or unavailable — not critical
      }
    }
  }, [gradingState, challenge, userInputs]);

  // Focus first input on mount and reset translation reveal
  useEffect(() => {
    const firstBlankIndex = blankIndices[0];
    if (firstBlankIndex !== undefined) {
      setTimeout(() => {
        inputRefs.current[firstBlankIndex]?.focus();
      }, 100);
    }
    // Reset translation reveal state for new challenge
    setIsTranslationRevealed(false);
  }, [blankIndices]);

  // Track shift key state for uppercase accent keyboard
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "Shift") setShiftHeld(true);
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Shift") setShiftHeld(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, []);

  // Determine if accent keyboard should show uppercase
  const accentUppercase = useMemo(() => {
    if (shiftHeld) return true;
    // Uppercase when cursor is at position 0 of the first blank and it's the first part
    const firstBlank = blankIndices[0];
    if (firstBlank === undefined) return false;
    const activeIndex = focusedInputIndex ?? firstBlank;
    if (activeIndex !== firstBlank || firstBlank !== 0) return false;
    const value = userInputs.get(activeIndex) || "";
    const input = inputRefs.current[activeIndex];
    const cursorPos = input?.selectionStart ?? 0;
    return cursorPos === 0 && value === "";
  }, [shiftHeld, focusedInputIndex, blankIndices, userInputs]);

  const handleInputChange = (index: number, value: string) => {
    const newInputs = new Map(userInputs);
    newInputs.set(index, value);
    setUserInputs(newInputs);
  };

  const handleCharacterInsert = (char: string) => {
    // Use the last focused input index, or the first blank if none was focused
    const targetIndex =
      focusedInputIndex !== null ? focusedInputIndex : blankIndices[0];

    if (targetIndex !== undefined) {
      const currentValue = userInputs.get(targetIndex) || "";
      const input = inputRefs.current[targetIndex];

      if (input) {
        // Focus the input first to get correct selection
        input.focus();

        const start = input.selectionStart || currentValue.length;
        const end = input.selectionEnd || currentValue.length;
        const newValue =
          currentValue.substring(0, start) + char + currentValue.substring(end);

        handleInputChange(targetIndex, newValue);

        setTimeout(() => {
          if (input) {
            const newPosition = start + char.length;
            input.setSelectionRange(newPosition, newPosition);
            input.focus();
            setFocusedInputIndex(targetIndex);
          }
        }, 0);
      }
    }
  };

  const allBlanksFilledOut = blankIndices.every(
    (index) =>
      userInputs.get(index)?.trim() !== undefined &&
      userInputs.get(index)?.trim() !== "",
  );

  const handleSubmit = useCallback(async () => {
    if (gradingState !== null) return;

    completedAtMsRef.current = Date.now();
    bumpBackground(30.0);
    const generation = ++gradingGenerationRef.current;
    setGradingState({ grading: null });

    const request: PartSubmitted[] = challenge.parts.map((part, index) => {
      if (part.type === "AskedToTranscribe") {
        const submission = (userInputs.get(index) ?? "").trim();

        return {
          type: "AskedToTranscribe" as const,
          parts: part.parts,
          submission,
        };
      } else {
        return {
          type: "Provided" as const,
          part: part.part,
        };
      }
    });

    const course: Course = {
      targetLanguage: targetLanguage,
      nativeLanguage: nativeLanguage,
    };

    const graded = await autograde_transcription(request, accessToken, course);
    if (generation !== gradingGenerationRef.current) return;

    if (graded.autograding_error) {
      reportAutogradeFailure("transcription", graded.autograding_error);
    }

    const isAllCorrect = graded.results.every(
      (result) =>
        result.type === "Provided" ||
        result.parts.every((part) => part.grade.type === "Perfect"),
    );

    setGradingState({
      graded,
    });

    playSoundEffect("aiDoneGrading");

    if (isAllCorrect) {
      playSoundEffect("perfect");
    }
  }, [
    gradingState,
    challenge.parts,
    userInputs,
    accessToken,
    targetLanguage,
    nativeLanguage,
    bumpBackground,
  ]);

  const handleTranscriptionContinue = useCallback(() => {
    if (gradingState && "graded" in gradingState) {
      localStorage.removeItem(STORAGE_KEY);
      bumpBackground(30.0);
      onComplete(gradingState.graded.results, completedAtMsRef.current!);
    }
  }, [gradingState, onComplete, bumpBackground]);

  // Global keyboard handler for Enter key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const activeElement = document.activeElement;
      const isInputFocused =
        activeElement?.tagName === "INPUT" ||
        activeElement?.tagName === "TEXTAREA";

      if (e.key === "Enter") {
        if (isInputFocused) {
          e.preventDefault();

          // Find which input is focused
          const currentIndex = inputRefs.current.findIndex(
            (ref) => ref === activeElement,
          );
          if (currentIndex === -1) return;

          // Find next blank
          const currentBlankPosition = blankIndices.findIndex(
            (index) => index === currentIndex,
          );
          const nextBlankIndex = blankIndices[currentBlankPosition + 1];

          if (nextBlankIndex !== undefined) {
            // Focus next input
            inputRefs.current[nextBlankIndex]?.focus();
          } else if (gradingState === null && allBlanksFilledOut) {
            handleSubmit();
          }
        } else if (gradingState && "graded" in gradingState) {
          e.preventDefault();
          handleTranscriptionContinue();
        }
      } else if (
        e.key === "ArrowRight" &&
        gradingState &&
        "graded" in gradingState &&
        !isInputFocused
      ) {
        e.preventDefault();
        handleTranscriptionContinue();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    gradingState,
    handleTranscriptionContinue,
    blankIndices,
    allBlanksFilledOut,
    handleSubmit,
  ]);

  const isAllCorrect =
    gradingState &&
    "graded" in gradingState &&
    gradingState.graded.results.every(
      (result) =>
        result.type === "Provided" ||
        result.parts.every((part) => part.grade.type === "Perfect"),
    );

  const renderSentenceWithBlanks = () => {
    const askedToTranscribeParts = challenge.parts.filter(
      (part) => part.type === "AskedToTranscribe",
    );
    const isSinglePartTranscription =
      askedToTranscribeParts.length === 1 &&
      challenge.parts.every(
        (part) =>
          part.type === "AskedToTranscribe" ||
          (part.type === "Provided" &&
            part.part.word.word_type?.type !== "Heteronym"),
      );

    return challenge.parts.map((item, index) => {
      if (item.type === "AskedToTranscribe") {
        if (item.parts.length === 0) {
          throw new Error("AskedToTranscribe part has no parts");
        }
        const end_whitespace = item.parts[item.parts.length - 1].whitespace;

        return (
          <span key={index}>
            <InlineTextarea
              ref={(el) => {
                inputRefs.current[index] = el;
              }}
              value={userInputs.get(index) || ""}
              onChange={(e) => handleInputChange(index, e.target.value)}
              onFocus={() => setFocusedInputIndex(index)}
              onBlur={() => {
                // Keep track of last focused input but allow blur
                // The accent keyboard will refocus when clicked
              }}
              disabled={gradingState !== null}
              lang={languageToLangAttr(targetLanguage)}
              autoCorrect="off"
              autoCapitalize={index === 0 ? "sentences" : "off"}
              spellCheck={false}
              className={`inline-block ${
                isSinglePartTranscription ? "min-w-64" : "min-w-32"
              } mx-1 text-center resize-none text-l font-semibold ${getInputClassName(
                index,
              )} border-0 border-b-3 border-dotted`}
              placeholder="Write what you hear"
            />
            <span>{end_whitespace}</span>
          </span>
        );
      } else {
        return (
          <span key={index}>
            <TargetLanguageText language={targetLanguage}>
              {item.part.word.text}
            </TargetLanguageText>
            {item.part.whitespace}
          </span>
        );
      }
    });
  };

  const getInputClassName = (index: number) => {
    if (gradingState && "graded" in gradingState) {
      const result = gradingState.graded.results[index];

      if (result && result.type === "AskedToTranscribe") {
        const allPerfect = result.parts.every(
          (part) => part.grade.type === "Perfect",
        );
        const hasMissed = result.parts.some(
          (part) => part.grade.type === "Missed",
        );
        const hasIncorrect = result.parts.some(
          (part) => part.grade.type === "Incorrect",
        );
        const hasPhoneticallySimilar = result.parts.some(
          (part) =>
            part.grade.type === "PhoneticallySimilarButContextuallyIncorrect",
        );
        const hasPhoneticallyIdentical = result.parts.some(
          (part) =>
            part.grade.type === "PhoneticallyIdenticalButContextuallyIncorrect",
        );

        if (allPerfect) {
          return "border-green-500 bg-green-50 dark:bg-green-950";
        } else if (hasPhoneticallyIdentical) {
          return "border-yellow-500 bg-yellow-50 dark:bg-yellow-950";
        } else if (hasPhoneticallySimilar) {
          return "border-orange-500 bg-orange-50 dark:bg-orange-950";
        } else if (hasIncorrect || hasMissed) {
          return "border-red-500 bg-red-50 dark:bg-red-950";
        }
      }
    }
    return "border-muted-foreground/30";
  };

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div className="flex flex-col gap-2">
        <Card animate className="pt-3 pb-3 pl-3 pr-3 relative gap-0">
          {challenge.second_chance && (
            <Badge className="absolute -top-2 -left-2 -rotate-12 z-10 shadow-sm text-sm">
              Second Chance!
            </Badge>
          )}
          {/* Dropdown menu for options */}
          <div className="absolute top-2 right-2">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <MoreVertical className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={() => setShowReportModal(true)}>
                  Report an Issue
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          <div className="space-y-6">
            {/* Audio + sentence with blanks, grouped so they read as one unit */}
            <div>
              <div className="flex flex-col items-center gap-2">
                <AudioButton
                  audioRequest={challenge.audio}
                  accessToken={accessToken}
                  autoPlay={true}
                  autoplayed={autoplayed}
                  setAutoplayed={setAutoplayed}
                  playPreAudio={true}
                  onError={() => setAudioError(true)}
                  onSuccess={() => setAudioError(false)}
                  visualizer
                />
                <p className="text-sm text-muted-foreground">
                  Listen and fill in the blanks
                </p>
              </div>
              <div className="text-center pt-4">
                <div className="text-2xl font-semibold leading-relaxed">
                  {renderSentenceWithBlanks()}
                </div>
              </div>
            </div>

            {gradingState === null && (
              <ProperNounDefinitions
                definitions={challenge.proper_noun_definitions}
                targetLanguage={targetLanguage}
              />
            )}

            {/* Result feedback */}
            {gradingState && (
              <div className="space-y-2 animate-feedback-in">
                {/* Show correct answer immediately when grading starts */}
                <div className="rounded-lg p-4 border bg-green-500/10 border-green-500/20">
                  <p className="text-sm font-medium mb-1 text-green-600 dark:text-green-400">
                    Correct answer:
                  </p>
                  <p className="text-lg font-medium">
                    <TargetLanguageText language={targetLanguage}>
                      {challenge.target_language}
                    </TargetLanguageText>
                  </p>
                </div>

                {/* Show skeleton while grading */}
                {"grading" in gradingState && <FeedbackSkeleton />}

                {/* Only show these when grading is complete */}
                {"graded" in gradingState && (
                  <>
                    {"autograding_error" in gradingState.graded &&
                      gradingState.graded.autograding_error && (
                        <AutogradeError />
                      )}

                    <WordGrades
                      wordGrades={gradingState.graded.results}
                      setGrade={(results) => {
                        setGradingState({
                          ...gradingState,
                          graded: { ...gradingState.graded, results: results },
                        });
                      }}
                      open_by_default={
                        "autograding_error" in gradingState.graded &&
                        gradingState.graded.autograding_error !== undefined
                      }
                      targetLanguage={targetLanguage}
                    />

                    <FeedbackDisplay
                      encouragement={gradingState.graded.encouragement}
                      explanation={gradingState.graded.explanation}
                      perfect={isAllCorrect ?? undefined}
                      targetLanguage={targetLanguage}
                    />

                    {Array.isArray(gradingState.graded.compare) &&
                      gradingState.graded.compare.length > 0 &&
                      (() => {
                        const words = gradingState.graded.compare;

                        const ttsText = words.map((w) => `${w};`).join(" ");

                        return (
                          <div className="rounded-lg p-4 border">
                            <div className="flex flex-row items-center gap-3">
                              <p className="text-sm font-medium">Listen:</p>
                              <AudioButton
                                audioRequest={{
                                  request: {
                                    text: ttsText,
                                    language: targetLanguage,
                                    speed: 0.8,
                                  },
                                  provider: "Google",
                                }}
                                accessToken={accessToken}
                                size="icon"
                                variant="ghost"
                                temp
                              />
                              <div className="flex flex-row flex-wrap justify-around items-center gap-3">
                                {words.map((item, idx) => (
                                  <span key={idx} className="font-medium">
                                    <TargetLanguageText
                                      language={targetLanguage}
                                    >
                                      {item}
                                    </TargetLanguageText>
                                    {idx < words.length - 1 && ","}
                                  </span>
                                ))}
                              </div>
                            </div>
                          </div>
                        );
                      })()}

                    <div
                      className="rounded-lg p-4 border cursor-pointer select-none"
                      onClick={() =>
                        setIsTranslationRevealed(!isTranslationRevealed)
                      }
                    >
                      <p className="text-sm font-medium mb-1">
                        English translation (click to reveal):
                      </p>
                      <p
                        className={`text-lg font-medium transition-all duration-100 ${
                          isTranslationRevealed ? "" : "blur-sm"
                        }`}
                      >
                        {challenge.native_language}
                      </p>
                    </div>

                    {wrongGramEntries.length > 0 && (
                      <div className="space-y-2">
                        {wrongGramEntries.map((entry, i) => (
                          <GramDefinitionDisplay
                            key={i}
                            definition={entry.definition}
                            breakdown={entry.breakdown}
                            targetLanguage={targetLanguage}
                          />
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>
            )}
          </div>
        </Card>

        {audioError && onCantListen && gradingState === null && (
          <AudioErrorBanner onSkip={onCantListen} />
        )}

        {/* Accented character keyboard - show when not graded, language supports it, and not on small screens */}
        {gradingState === null &&
          (targetLanguage === "French" ||
            targetLanguage === "Spanish" ||
            targetLanguage === "German") && (
            <AccentedCharacterKeyboard
              onCharacterInsert={handleCharacterInsert}
              language={targetLanguage}
              uppercase={accentUppercase}
              className="hidden md:flex mt-3 p-3 border rounded-lg bg-muted/30"
            />
          )}

        {/* Mobile keyboard tip - show on small screens when conditions are met */}
        {gradingState === null && totalCount < 60 && (
          <MobileKeyboardTip language={targetLanguage} />
        )}

        {/* Movie posters - hidden after grading */}
        {gradingState === null && (
          <MoviePosterGrid movieData={movieData} deck={deck} />
        )}
      </div>

      <div className="mt-4 flex flex-col gap-2 sticky bottom-0">
        {onCantListen && gradingState === null && (
          <CantListenButton onClick={onCantListen} />
        )}

        <div>
          {gradingState !== null && "grading" in gradingState ? (
            <div className="flex gap-2">
              <Button
                className="flex-1 h-14 text-lg"
                size="lg"
                disabled
              >
                AI is grading...
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-14 w-14"
                onClick={() => {
                  gradingGenerationRef.current++;
                  setGradingState(null);
                }}
              >
                <X className="h-5 w-5" />
              </Button>
            </div>
          ) : (
            <Button
              onClick={
                gradingState && "graded" in gradingState
                  ? handleTranscriptionContinue
                  : handleSubmit
              }
              disabled={
                (gradingState === null && !allBlanksFilledOut) ||
                (gradingState !== null && "error" in gradingState)
              }
              className="w-full h-14 text-lg"
              size="lg"
            >
              {gradingState === null ? (
                <span className="relative flex items-center justify-center">
                  Check Answer
                  <span className="absolute left-full ml-2 text-sm text-muted-foreground hide-keyboard-hint-mobile">
                    (⏎)
                  </span>
                </span>
              ) : "error" in gradingState ? (
                "Error"
              ) : (
                <span className="relative flex items-center justify-center">
                  {isAllCorrect ? "Nailed it!" : "Continue"}
                  <span className="absolute left-full ml-2 text-sm text-muted-foreground hide-keyboard-hint-mobile">
                    (⏎)
                  </span>
                </span>
              )}
            </Button>
          )}
        </div>
      </div>

      <ReportIssueModal
        context={`Transcription challenge: ${JSON.stringify(challenge)}`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  );
}

interface WordGradesProps {
  wordGrades: PartGraded[];
  setGrade: (results: PartGraded[]) => void;
  open_by_default: boolean;
  targetLanguage: Language;
}

function WordGrades({
  wordGrades,
  setGrade,
  open_by_default,
  targetLanguage,
}: WordGradesProps) {
  const [isOpen, setIsOpen] = useState(open_by_default);

  const gradeOptions = [
    { value: "Perfect", label: "Perfect" },
    { value: "CorrectWithTypo", label: "Correct with Typo" },
    {
      value: "PhoneticallyIdenticalButContextuallyIncorrect",
      label: "Phonetically Identical",
    },
    {
      value: "PhoneticallySimilarButContextuallyIncorrect",
      label: "Phonetically Similar",
    },
    { value: "Incorrect", label: "Incorrect" },
    { value: "Missed", label: "Missed" },
  ];

  const getGradeKey = (grade: WordGrade): string => {
    return grade.type;
  };

  const handleGradeChange = (
    partIndex: number,
    wordIndex: number,
    newGradeKey: string,
  ) => {
    const updatedGrades = [...wordGrades];
    const part = updatedGrades[partIndex];

    if (part.type === "AskedToTranscribe") {
      const newGrade: WordGrade = { type: newGradeKey } as WordGrade;
      part.parts[wordIndex].grade = newGrade;
    }

    setGrade(updatedGrades);
  };

  const transcribedParts = wordGrades.filter(
    (part) => part.type === "AskedToTranscribe",
  );

  if (transcribedParts.length === 0) {
    return null;
  }

  console.log(wordGrades);

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between p-0">
          <span className="text-sm font-medium">Grade Words Manually</span>
          <span className="text-xs text-muted-foreground">
            {isOpen ? "Hide" : "Show"}
          </span>
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="mt-3 space-y-3">
          {wordGrades.map((part, partIndex) => {
            if (part.type === "AskedToTranscribe") {
              return (
                <div key={partIndex} className="space-y-2">
                  <div className="text-sm text-muted-foreground">
                    Your answer: "{part.submission}"
                  </div>
                  <div className="grid gap-2">
                    {part.parts.map((wordPart, wordIndex) => (
                      <div
                        key={wordIndex}
                        className="flex items-center gap-3 p-2 rounded-lg bg-muted/30"
                      >
                        <div className="flex-1">
                          <span className="font-medium">
                            <TargetLanguageText language={targetLanguage}>
                              {wordPart.heard.word.text}
                            </TargetLanguageText>
                          </span>
                        </div>
                        <Select
                          value={getGradeKey(wordPart.grade)}
                          onValueChange={(value: string) =>
                            handleGradeChange(partIndex, wordIndex, value)
                          }
                        >
                          <SelectTrigger className="w-[200px]">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            {gradeOptions.map((option) => (
                              <SelectItem
                                key={option.value}
                                value={option.value}
                              >
                                {option.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                    ))}
                  </div>
                </div>
              );
            }
            return null;
          })}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
