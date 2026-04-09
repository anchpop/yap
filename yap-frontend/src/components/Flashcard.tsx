import {
  type AudioRequest,
  type CardContent,
  type DictionaryEntry,
  type Language,
  type Literal,
  type PhrasebookDefinitionEntry,
  type Rating,
  type TargetToNativeWord,
  get_word_prefix,
} from "../../../yap-frontend-rs/pkg";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { MoreVertical, ArrowLeft, ArrowRight, ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  motion,
  useMotionValue,
  useTransform,
  useAnimation as animationControls,
  type PanInfo,
} from "framer-motion";
import { useCallback, useEffect, useState } from "react";
import "./Flashcard.css";
import { AudioButton } from "./AudioButton";
import { ReportIssueModal } from "./challenges/ReportIssueModal";
import { CantListenButton } from "./CantListenButton";
import { AudioErrorBanner } from "./AudioErrorBanner";
import { toast } from "sonner";
import { match } from "ts-pattern";
import { formatMorphology } from "@/utils/formatMorphology";
import { useBackground } from "./BackgroundShader";
import { PlayfulArrow } from "./PlayfulArrow";
import { cn } from "@/lib/utils";
import { highlightTermInSentence } from "@/utils/highlightTermInSentence";
import { TargetLanguageText } from "./TargetLanguageText";

// GramDefinition is missing from the .d.ts due to a type generator bug
type GramDefinition =
  | { Dictionary: DictionaryEntry }
  | { Phrasebook: PhrasebookDefinitionEntry };

function gramDisplayText(gram: Literal<string>[]): string {
  return gram
    .map((l) => l.word.text + l.whitespace)
    .join("")
    .trim();
}

interface FlashcardProps {
  audioRequest: AudioRequest | undefined;
  content: CardContent;
  totalCount: number;
  onRating?: (rating: Rating) => void;
  accessToken: string | undefined;
  onCantListen?: () => void;
  isNew: boolean;
  targetLanguage: Language;
  nativeLanguage: Language;
  listeningPrefix?: string;
  autoplayed: boolean;
  setAutoplayed: () => void;
  timesTypeSeen: number;
}

const CardFront = ({
  content,
  listeningPrefix,
  targetLanguage,
}: {
  content: CardContent;
  listeningPrefix?: string;
  targetLanguage: Language;
}) => {
  return match(content)
    .with({ type: "Listening" }, () => {
      const prefix = listeningPrefix || "Le mot est";
      return (
        <h2 className="text-3xl font-semibold flex items-center gap-3 flex-wrap justify-center text-center">
          <span>
            <TargetLanguageText language={targetLanguage}>
              {prefix}
            </TargetLanguageText>{" "}
            _____.{" "}
          </span>
        </h2>
      );
    })
    .with({ type: "Gram" }, (content) => {
      const definition = content.definition as GramDefinition;
      const text = gramDisplayText(content.gram);

      if ("Dictionary" in definition) {
        const dict = definition.Dictionary;
        // Try to get word prefix from morphology + first heteronym in gram
        const firstHeteronym = content.gram
          .map((l) => l.word.word_type)
          .find((wt) => wt.type === "Heteronym");
        const wordPrefix =
          firstHeteronym &&
          firstHeteronym.type === "Heteronym" &&
          dict.morphology.length > 0
            ? get_word_prefix(
                dict.morphology[0],
                firstHeteronym.word,
                firstHeteronym.pos,
                targetLanguage,
              )
            : undefined;
        return (
          <h2 className="text-3xl font-semibold">
            <TargetLanguageText language={targetLanguage}>
              {wordPrefix && (
                <span className="text-muted-foreground/60">
                  {wordPrefix.prefix}
                  {wordPrefix.separator}
                </span>
              )}
              {text}
            </TargetLanguageText>
          </h2>
        );
      } else {
        return (
          <h2 className="text-3xl font-semibold">
            <TargetLanguageText language={targetLanguage}>
              {text}
            </TargetLanguageText>
          </h2>
        );
      }
    })
    .exhaustive();
};

const CardFrontSubtitle = ({ content }: { content: CardContent }) => {
  return match(content)
    .with({ type: "Listening" }, () => (
      <span className="text-sm text-muted-foreground"> Fill in the blank!</span>
    ))
    .with({ type: "Gram" }, (content) => {
      const definition = content.definition as GramDefinition;
      if ("Dictionary" in definition) {
        const firstHeteronym = content.gram
          .map((l) => l.word.word_type)
          .find((wt) => wt.type === "Heteronym");
        const partOfSpeech =
          firstHeteronym && firstHeteronym.type === "Heteronym"
            ? match(firstHeteronym.pos)
                .with("ADJ", () => "Adjective")
                .with("ADP", () => "Adposition")
                .with("ADV", () => "Adverb")
                .with("AUX", () => "Auxiliary")
                .with("CCONJ", () => "Conjunction")
                .with("DET", () => "Determiner")
                .with("INTJ", () => "Interjection")
                .with("NOUN", () => "Noun")
                .with("NUM", () => "Number")
                .with("PART", () => "Particle")
                .with("PRON", () => "Pronoun")
                .with("SCONJ", () => "Subordinating Conjunction")
                .with("SYM", () => "Symbol")
                .with("VERB", () => "Verb")
                .exhaustive()
            : null;
        return partOfSpeech ? (
          <span className="text-sm text-muted-foreground">
            ({partOfSpeech})
          </span>
        ) : null;
      } else {
        return (
          <span className="text-sm text-muted-foreground">(Multiword)</span>
        );
      }
    })
    .exhaustive();
};

const CardBack = ({
  content,
  targetLanguage,
  accessToken,
}: {
  content: CardContent;
  targetLanguage: Language;
  accessToken: string | undefined;
}) => {
  return match(content)
    .with({ type: "Listening" }, (content) => {
      const possibleGrams = content.possible_grams;

      const definitionsGloss = (
        definitions: GramDefinition[],
      ): string | null => {
        const parts = definitions.flatMap((definition) => {
          if ("Dictionary" in definition) {
            return definition.Dictionary.definitions
              .map((d: TargetToNativeWord) => d.native)
              .filter(Boolean);
          }
          return definition.Phrasebook.meaning
            ? [definition.Phrasebook.meaning]
            : [];
        });
        return parts.length > 0 ? parts.join(", ") : null;
      };

      if (possibleGrams.length === 1) {
        const [, gram, definitions] = possibleGrams[0];
        const gloss = definitionsGloss(definitions as GramDefinition[]);
        return (
          <div className="space-y-2">
            <div className="text-3xl font-medium">
              <TargetLanguageText language={targetLanguage}>
                {gramDisplayText(gram)}
              </TargetLanguageText>
            </div>
            {gloss && (
              <div className="text-lg text-muted-foreground">{gloss}</div>
            )}
          </div>
        );
      }

      return (
        <div className="space-y-4">
          <div className="text-sm text-muted-foreground">
            It could have been any of these words:
          </div>
          <div className="grid grid-cols-2 gap-2">
            {possibleGrams.map(([isKnown, gram, definitions], index: number) => {
              const gloss = definitionsGloss(definitions as GramDefinition[]);
              return (
                <div
                  key={index}
                  className={`text-left p-2 rounded-md ${
                    isKnown
                      ? "bg-green-500/10 border border-green-500/20"
                      : "bg-muted/30 border border-muted/20"
                  }`}
                >
                  <div>
                    <span className="text-lg">
                      <TargetLanguageText language={targetLanguage}>
                        {gramDisplayText(gram)}
                      </TargetLanguageText>
                    </span>
                    {isKnown && (
                      <span className="text-sm text-green-600 ml-2">
                        (known)
                      </span>
                    )}
                  </div>
                  {gloss && (
                    <div className="text-sm text-muted-foreground">
                      {gloss}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      );
    })
    .with({ type: "Gram" }, (content) => {
      const definition = content.definition as GramDefinition;
      const term = gramDisplayText(content.gram);

      if ("Dictionary" in definition) {
        const dict = definition.Dictionary;
        const morphologyText =
          dict.morphology.length > 0
            ? formatMorphology(dict.morphology[0])
            : null;

        return (
          <>
            {dict.definitions.map((def: TargetToNativeWord, index: number) => (
              <div
                key={index}
                className="text-left border border-card/50 bg-card/30 rounded-lg p-4 space-y-2"
              >
                <div className="flex items-baseline justify-between gap-2">
                  <span className="text-xl font-medium">{def.native}</span>
                  {morphologyText && (
                    <span className="text-sm text-muted-foreground italic">
                      {morphologyText}
                    </span>
                  )}
                </div>

                {def.example_sentence_target_language && (
                  <div className="space-y-1 text-sm">
                    <div className="flex items-start gap-2">
                      <div onClick={(e) => e.stopPropagation()}>
                        <AudioButton
                          audioRequest={{
                            request: {
                              text: def.example_sentence_target_language,
                              language: targetLanguage,
                            },
                            provider: "ElevenLabs",
                          }}
                          accessToken={accessToken}
                          className="h-8 w-8"
                          size="icon"
                        />
                      </div>
                      <div>
                        <p className="text-muted-foreground italic flex-1">
                          <TargetLanguageText language={targetLanguage}>
                            "
                            {highlightTermInSentence(
                              def.example_sentence_target_language,
                              term,
                            )}
                            "
                          </TargetLanguageText>
                        </p>
                        <p className="text-muted-foreground">
                          "{def.example_sentence_native_language}"
                        </p>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </>
        );
      } else {
        const pb = definition.Phrasebook;
        return (
          <div className="text-left bg-muted/30 rounded-lg p-4 space-y-2">
            <div className="flex items-baseline gap-2">
              <span className="text-xl font-medium">{pb.meaning}</span>
            </div>

            {pb.target_language_example && (
              <div className="text-sm">
                <div className="flex items-start gap-2">
                  <div>
                    <p className="text-muted-foreground italic">
                      <TargetLanguageText language={targetLanguage}>
                        "
                        {highlightTermInSentence(
                          pb.target_language_example,
                          term,
                        )}
                        "
                      </TargetLanguageText>
                    </p>
                    <p className="text-muted-foreground">
                      "{pb.native_language_example}"
                    </p>
                  </div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <AudioButton
                      audioRequest={{
                        request: {
                          text: pb.target_language_example,
                          language: targetLanguage,
                        },
                        provider: "ElevenLabs",
                      }}
                      accessToken={accessToken}
                      className="h-8 w-8"
                      size="icon"
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
        );
      }
    })
    .exhaustive();
};

export const Flashcard = function Flashcard({
  audioRequest,
  content,
  totalCount,
  onRating,
  accessToken,
  onCantListen,
  isNew,
  targetLanguage,
  nativeLanguage,
  listeningPrefix,
  autoplayed,
  setAutoplayed,
  timesTypeSeen,
}: FlashcardProps) {
  const x = useMotionValue(0);
  const controls = animationControls();
  const [isDragging, setIsDragging] = useState(false);
  const [showReportModal, setShowReportModal] = useState(false);
  const [showAnswer, setShowAnswer] = useState(false);
  const [hasBeenOpened, setHasBeenOpened] = useState(false);
  const [audioError, setAudioError] = useState(false);
  const { bumpBackground } = useBackground();

  const toggleAnswer = useCallback(
    () => setShowAnswer(!showAnswer),
    [showAnswer],
  );

  const leftLabel = isNew ? "Didn't know" : "Forgot";
  const rightLabel = isNew ? "Already knew" : "Remembered";

  const requireShowAnswer = totalCount < 50 || timesTypeSeen < 10;
  const canGrade = hasBeenOpened || showAnswer || !requireShowAnswer;

  const showTutorial = timesTypeSeen < 2;

  const tutorialText = match(content)
    .with({ type: "Gram" }, (c) => (
      <>
        Guess what "
        <TargetLanguageText language={targetLanguage}>
          {gramDisplayText(c.gram)}
        </TargetLanguageText>
        " means…
      </>
    ))
    .with({ type: "Listening" }, () => (
      <>Guess what {targetLanguage} word is missing</>
    ))
    .exhaustive();

  const showAnswerText = match(content)
    .with({ type: "Gram" }, () => `Show ${nativeLanguage}`)
    .with({ type: "Listening" }, () => "Show missing word")
    .exhaustive();

  const rotate = useTransform(x, [-200, 200], [-30, 30]);

  // Color overlay for visual feedback
  const leftOverlayOpacity = useTransform(x, [-200, 0], [1, 0]);
  const rightOverlayOpacity = useTransform(x, [0, 200], [0, 1]);

  const handleDragEnd = async (
    _event: MouseEvent | TouchEvent | PointerEvent,
    info: PanInfo,
  ) => {
    setIsDragging(false);
    const threshold = 100;

    if (!canGrade) {
      controls.start({
        x: 0,
        transition: { type: "spring", stiffness: 300, damping: 20 },
      });
      return;
    }

    if (info.offset.x > threshold && info.velocity.x > 0) {
      // Swiped right - "remembered"
      await controls.start({
        x: 300,
        transition: { duration: 0.2 },
      });
      if (onRating) {
        bumpBackground(30.0);
        window.scrollTo({ top: 0, behavior: "smooth" });
        onRating("remembered");
      }
    } else if (info.offset.x < -threshold && info.velocity.x < 0) {
      // Swiped left - Again
      await controls.start({
        x: -300,
        transition: { duration: 0.2 },
      });
      if (onRating) {
        bumpBackground(30.0);
        window.scrollTo({ top: 0, behavior: "smooth" });
        onRating("again");
      }
    } else {
      // Not enough swipe - snap back
      controls.start({
        x: 0,
        transition: { type: "spring", stiffness: 300, damping: 20 },
      });
    }
  };

  // Track if card has been opened
  useEffect(() => {
    if (showAnswer && !hasBeenOpened) {
      setHasBeenOpened(true);
    }
  }, [showAnswer, hasBeenOpened]);

  // Reset position and animate in
  useEffect(() => {
    // Reset to initial state instantly, then animate in
    controls.set({ x: 0, scale: 0.95 });
    controls.start({
      x: 0,
      scale: 1,
      transition: {
        duration: 0.3,
        ease: "easeOut",
      },
    });
  }, [controls]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      if (e.key === "Enter") {
        e.preventDefault();
        toast("Use the arrow keys");
        return;
      }

      if (["1", "2", "3", "4"].includes(e.key)) {
        e.preventDefault();
        toast("Use the arrow keys");
        return;
      }

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
      }

      // Show answer: Space / ↓ / j (when answer is hidden)
      if (
        !showAnswer &&
        (e.key === " " || e.key === "ArrowDown" || e.key === "j")
      ) {
        e.preventDefault();
        toggleAnswer();
      }
      // Hide answer: ↑ / k
      else if (showAnswer && (e.key === "ArrowUp" || e.key === "k")) {
        e.preventDefault();
        toggleAnswer();
      }
      // Mark as remembered: →
      else if (canGrade && e.key === "ArrowRight" && !e.shiftKey) {
        e.preventDefault();
        if (onRating) {
          bumpBackground(30.0);
          window.scrollTo({ top: 0, behavior: "smooth" });
          onRating("remembered");
        }
      }
      // Mark as "again": ←
      else if (canGrade && e.key === "ArrowLeft") {
        e.preventDefault();
        if (onRating) {
          bumpBackground(30.0);
          window.scrollTo({ top: 0, behavior: "smooth" });
          onRating("again");
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [showAnswer, canGrade, toggleAnswer, onRating, isNew, bumpBackground]);

  const copyWord = () => {
    const word = match(content)
      .with({ type: "Gram" }, (c) => gramDisplayText(c.gram))
      .with({ type: "Listening" }, (c) =>
        c.possible_grams.length > 0
          ? gramDisplayText(c.possible_grams[0][1])
          : undefined,
      )
      .exhaustive();

    if (word) {
      navigator.clipboard
        .writeText(word)
        .then(() => toast("Copied to clipboard"))
        .catch(() => toast("Failed to copy"));
    } else {
      toast("No word to copy");
    }
  };

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div className="flex flex-col gap-2">
        {/* Tutorial text above card */}
        {showTutorial && (
          <div
            className={cn(
              "grid transition-all duration-300",
              showAnswer
                ? "grid-rows-[0fr] opacity-0"
                : "grid-rows-[1fr] opacity-100",
            )}
          >
            <div className="overflow-hidden">
              <div className="text-center mt-4 text-2xl font-semibold text-muted-foreground animate-fade-in flex flex-row justify-center items-start gap-1">
                <PlayfulArrow direction="down" flipStart size={70} />
                <div>{tutorialText}</div>
                <PlayfulArrow direction="down" size={70} />
              </div>
            </div>
          </div>
        )}

        <motion.div
          className="relative w-full"
          drag="x"
          dragConstraints={{ left: 0, right: 0 }}
          onDragStart={() => setIsDragging(true)}
          onDragEnd={handleDragEnd}
          animate={controls}
          style={{ x, rotate }}
        >
          <Card
            className={`pt-3 pb-3 pl-3 pr-3 cursor-pointer transition-all hover:shadow-lg overflow-hidden flashcard h-full gap-0 ${
              !showAnswer ? "spin-on-hover" : ""
            }`}
            onClick={() => {
              if (!isDragging) {
                toggleAnswer();
              }
            }}
            animate
          >
            {/* Swipe feedback overlays */}
            <motion.div
              className="absolute inset-0 bg-red-500/20 pointer-events-none"
              style={{ opacity: leftOverlayOpacity }}
            />
            <motion.div
              className="absolute inset-0 bg-green-500/20 pointer-events-none"
              style={{ opacity: rightOverlayOpacity }}
            />

            {/* Swipe indicators */}
            <motion.div
              className="absolute top-8 left-8 text-red-500 font-bold text-2xl rotate-[-30deg] pointer-events-none"
              style={{ opacity: leftOverlayOpacity }}
            >
              {leftLabel.toUpperCase()}
            </motion.div>
            <motion.div
              className="absolute top-8 right-8 text-green-500 font-bold text-2xl rotate-[30deg] pointer-events-none"
              style={{ opacity: rightOverlayOpacity }}
            >
              {rightLabel.toUpperCase()}
            </motion.div>

            <div className="text-center relative z-10 flex flex-col gap-6">
              <div className="justify-center gap-2 flex flex-col items-center w-full">
                <div
                  className="flex items-center justify-between w-full"
                  onClick={(e) => e.stopPropagation()}
                >
                  {audioRequest ? (
                    <AudioButton
                      audioRequest={audioRequest}
                      accessToken={accessToken}
                      autoPlay={true}
                      autoplayed={autoplayed}
                      setAutoplayed={setAutoplayed}
                      onError={() => setAudioError(true)}
                      onSuccess={() => setAudioError(false)}
                    />
                  ) : (
                    <div className="w-10" /> /* Spacer to keep content centered */
                  )}

                  <CardFront
                    content={content}
                    listeningPrefix={listeningPrefix}
                    targetLanguage={targetLanguage}
                  />

                  {onRating ? (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-10 w-10"
                        >
                          <MoreVertical className="h-6 w-6 size--xl" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          onClick={() => {
                            bumpBackground(30.0);
                            onRating("easy");
                          }}
                        >
                          Easy
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          onClick={() => {
                            bumpBackground(30.0);
                            onRating("good");
                          }}
                        >
                          Good
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          onClick={() => {
                            bumpBackground(30.0);
                            onRating("hard");
                          }}
                        >
                          Hard
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={copyWord}>
                          Copy word
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          onClick={() => setShowReportModal(true)}
                        >
                          Report an Issue
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  ) : (
                    <div className="w-8" /> /* Spacer to keep word centered */
                  )}
                </div>
                <CardFrontSubtitle content={content} />
              </div>

              <hr className="" />

              {showAnswer ? (
                <div className="space-y-6 animate-feedback-in">
                  <CardBack
                    content={content}
                    targetLanguage={targetLanguage}
                    accessToken={accessToken}
                  />
                </div>
              ) : (
                <div className="flex flex-col items-center gap-2">
                  <div
                    className={` ${
                      requireShowAnswer ? "font-bold" : "text-muted-foreground"
                    }`}
                  >
                    {showAnswerText}
                  </div>
                  <kbd className="h-6 w-6 text-xs font-semibold border rounded bg-muted/20 border flex items-center justify-center hide-kbd-border-mobile">
                    <ArrowDown className="h-3 w-3 text-muted-foreground" />
                  </kbd>
                </div>
              )}
            </div>
          </Card>
        </motion.div>

        {/* Tutorial text below card */}
        {showTutorial && !showAnswer && (
          <div className="text-center mt-2 text-2xl font-semibold text-muted-foreground flex flex-row justify-center items-end animate-fade-in-delayed">
            <PlayfulArrow direction="up" size={70} />
            <span>Then, tap to see if you're right!</span>
            <PlayfulArrow direction="up" flipStart size={70} />
          </div>
        )}
      </div>

      <div className="flex flex-col sticky bottom-0">
        {/* Tutorial text above buttons */}
        {showTutorial && showAnswer && (
          <div className="text-center mt-4 text-2xl font-semibold text-muted-foreground flex flex-row justify-center items-start animate-fade-in-delayed">
            <PlayfulArrow direction="down" flipStart size={96} />
            <span>Were you right?</span>
            <PlayfulArrow direction="down" size={96} />
          </div>
        )}

        {onRating && (
          <div
            className={`mt-4 flex flex-col gap-2 transition-opacity duration-300`}
          >
            {!showAnswer && (
              <>
                {audioError && onCantListen && content.type === "Listening" && (
                  <AudioErrorBanner onSkip={onCantListen} />
                )}
                {onCantListen && content.type === "Listening" && (
                  <CantListenButton onClick={onCantListen} />
                )}
              </>
            )}
            <div className={!canGrade ? "hidden" : "quick-fade-in"}>
              <div className="grid grid-cols-2">
                <Button
                  onClick={() => {
                    if (!canGrade) return;
                    bumpBackground(30.0);
                    window.scrollTo({ top: 0, behavior: "smooth" });
                    onRating("again");
                  }}
                  variant="destructive"
                  size="lg"
                  className="h-14 text-lg rounded-r-none group"
                  disabled={!canGrade}
                >
                  <span className="relative flex items-center justify-center">
                    <kbd className="absolute right-full mr-2 h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center hide-kbd-mobile opacity-0 group-hover:opacity-100 transition-opacity">
                      <ArrowLeft className="h-3 w-3" />
                    </kbd>
                    {leftLabel}
                  </span>
                </Button>
                <Button
                  onClick={() => {
                    if (!canGrade) return;
                    bumpBackground(30.0);
                    window.scrollTo({ top: 0, behavior: "smooth" });
                    onRating("remembered");
                  }}
                  variant="default"
                  size="lg"
                  className="h-14 text-lg rounded-l-none group"
                  disabled={!canGrade}
                >
                  <span className="relative flex items-center justify-center">
                    {rightLabel}
                    <kbd className="absolute left-full ml-2 h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center hide-kbd-mobile opacity-0 group-hover:opacity-100 transition-opacity">
                      <ArrowRight className="h-3 w-3" />
                    </kbd>
                  </span>
                </Button>
              </div>
            </div>
          </div>
        )}
      </div>

      <ReportIssueModal
        context={`${JSON.stringify(content)}`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  );
};
