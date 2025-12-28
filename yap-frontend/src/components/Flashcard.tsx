import {
  type AudioRequest,
  type CardContent,
  type Language,
  type Rating,
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
import { useEffect, useState } from "react";
import "./Flashcard.css";
import { AudioButton } from "./AudioButton";
import { ReportIssueModal } from "./challenges/ReportIssueModal";
import { CantListenButton } from "./CantListenButton";
import { CantSpeakButton } from "./CantSpeakButton";
import { toast } from "sonner";
import { match } from "ts-pattern";
import { formatMorphology } from "@/utils/formatMorphology";
import { useBackground } from "./BackgroundShader";

interface FlashcardProps {
  audioRequest: AudioRequest | undefined;
  content: CardContent<string>;
  showAnswer: boolean;
  onToggle: () => void;
  totalCount: number;
  onRating?: (rating: Rating) => void;
  accessToken: string | undefined;
  onCantListen?: () => void;
  onCantSpeak?: () => void;
  isNew: boolean;
  targetLanguage: Language;
  listeningPrefix?: string;
  autoplayed: boolean;
  setAutoplayed: () => void;
}

const CardFront = ({
  content,
  listeningPrefix,
  targetLanguage,
}: {
  content: CardContent<string>;
  listeningPrefix?: string;
  targetLanguage: Language;
}) => {
  if ("Listening" in content) {
    const prefix = listeningPrefix || "Le mot est";
    return (
      <h2 className="text-3xl font-semibold flex items-center gap-3 flex-wrap justify-center text-center">
        <span>{prefix} _____. </span>
      </h2>
    );
  } else if ("Heteronym" in content) {
    const wordPrefix = get_word_prefix(
      content.Heteronym.morphology,
      content.Heteronym.heteronym.word,
      content.Heteronym.heteronym.pos,
      targetLanguage
    );
    return (
      <h2 className="text-3xl font-semibold">
        {wordPrefix && (
          <span className="text-muted-foreground/60">
            {wordPrefix.prefix}
            {wordPrefix.separator}
          </span>
        )}
        {content.Heteronym.heteronym.word}
      </h2>
    );
  } else if ("Multiword" in content) {
    return <h2 className="text-3xl font-semibold">{content.Multiword[0]}</h2>;
  } else if ("LetterPronunciation" in content) {
    const guide = content.LetterPronunciation.guide;
    const pattern = content.LetterPronunciation.pattern;

    // Add visual indicators for position
    const displayPattern = match(guide.position)
      .with("Beginning", () => `${pattern}___`)
      .with("End", () => `___${pattern}`)
      .with("Anywhere", () => pattern)
      .exhaustive();

    return <h2 className="text-4xl font-bold">🗣️ "{displayPattern}"</h2>;
  } else {
    return <h2 className="text-3xl font-semibold">Unknown card type</h2>;
  }
};

const CardFrontSubtitle = ({ content }: { content: CardContent<string> }) => {
  if ("Listening" in content) {
    return (
      <span className="text-sm text-muted-foreground"> Fill in the blank!</span>
    );
  }

  if ("LetterPronunciation" in content) {
    const guide = content.LetterPronunciation.guide;
    const positionText = match(guide.position)
      .with("Beginning", () => "Appears at the beginning of words")
      .with("End", () => "Appears at the end of words")
      .with("Anywhere", () => null)
      .exhaustive();

    return (
      <div className="flex flex-col gap-1 items-center">
        <span className="text-sm text-muted-foreground">Say it out loud!</span>
        {positionText && (
          <span className="text-xs text-muted-foreground/80">
            {positionText}
          </span>
        )}
      </div>
    );
  }

  const partOfSpeech =
    "Heteronym" in content
      ? content.Heteronym.heteronym.pos == "ADJ"
        ? "Adjective"
        : content.Heteronym.heteronym.pos == "ADP"
        ? "Adposition"
        : content.Heteronym.heteronym.pos == "ADV"
        ? "Adverb"
        : content.Heteronym.heteronym.pos == "AUX"
        ? "Auxiliary"
        : content.Heteronym.heteronym.pos == "CCONJ"
        ? "Conjunction"
        : content.Heteronym.heteronym.pos == "DET"
        ? "Determiner"
        : content.Heteronym.heteronym.pos == "INTJ"
        ? "Interjection"
        : content.Heteronym.heteronym.pos == "NOUN"
        ? "Noun"
        : content.Heteronym.heteronym.pos == "NUM"
        ? "Number"
        : content.Heteronym.heteronym.pos == "PART"
        ? "Particle"
        : content.Heteronym.heteronym.pos == "PRON"
        ? "Pronoun"
        : content.Heteronym.heteronym.pos == "SCONJ"
        ? "Subordinating Conjunction"
        : content.Heteronym.heteronym.pos == "SYM"
        ? "Symbol"
        : content.Heteronym.heteronym.pos == "VERB"
        ? "Verb"
        : content.Heteronym.heteronym.pos == "X"
        ? "Unknown"
        : "Unknown"
      : "Multiword";
  return (
    <span className="text-sm text-muted-foreground">({partOfSpeech})</span>
  );
};

const CardBack = ({
  content,
  targetLanguage,
  accessToken,
}: {
  content: CardContent<string>;
  targetLanguage: Language;
  accessToken: string | undefined;
}) => {
  if ("Listening" in content) {
    const possible_words: [boolean, string][] =
      content.Listening.possible_words;

    if (possible_words.length === 1) {
      return <div className="text-3xl font-medium">{possible_words[0][1]}</div>;
    }

    return (
      <div className="space-y-4">
        <div className="text-sm text-muted-foreground">
          It could have been any of these words:
        </div>
        <div className="grid grid-cols-2 gap-2">
          {possible_words.map(([isKnown, word], index) => (
            <div
              key={index}
              className={`text-left p-2 rounded-md ${
                isKnown
                  ? "bg-green-500/10 border border-green-500/20"
                  : "bg-muted/30 border border-muted/20"
              }`}
            >
              <span className="text-lg">{word}</span>
              {isKnown && (
                <span className="text-sm text-green-600 ml-2">(known)</span>
              )}
            </div>
          ))}
        </div>
      </div>
    );
  } else if ("Heteronym" in content) {
    const morphologyText = formatMorphology(content.Heteronym.morphology);
    return content.Heteronym.definitions.map((def, index) => (
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
                  "{def.example_sentence_target_language}"
                </p>
                <p className="text-muted-foreground">
                  "{def.example_sentence_native_language}"
                </p>
              </div>
            </div>
          </div>
        )}
      </div>
    ));
  } else if ("LetterPronunciation" in content) {
    const guide = content.LetterPronunciation.guide;
    const pattern = content.LetterPronunciation.pattern;

    // Get the appropriate connector phrase based on the target language
    const connector = match(targetLanguage)
      .with("French", () => "comme dans")
      .with("Spanish", () => "como en")
      .with("Korean", () => "처럼") // cheoreom (like/as in)
      .with("English", () => "as in")
      .with("German", () => "wie in")
      .with("Chinese", () => "如") // rú (like/as)
      .with("Japanese", () => "のように") // no you ni (like/as in)
      .with("Russian", () => "как в") // kak v (as in)
      .with("Portuguese", () => "como em")
      .with("Italian", () => "come in")
      .exhaustive();

    return (
      <div className="space-y-4">
        <div className="text-left bg-muted/30 rounded-lg p-4 space-y-4">
          {guide.example_words && guide.example_words.length > 0 && (
            <div className="space-y-3">
              <div className="text-sm text-muted-foreground">Examples:</div>
              <div className="grid gap-3">
                {guide.example_words.slice(0, 3).map((example, index) => {
                  // Find and highlight the pattern in the word based on position
                  const lowerPattern = pattern.toLowerCase();
                  const lowerWord = example.target.toLowerCase();

                  let patternIndex = -1;
                  const matchLength = pattern.length;

                  if (guide.position === "Beginning") {
                    // Only highlight if pattern is at the beginning
                    if (lowerWord.startsWith(lowerPattern)) {
                      patternIndex = 0;
                    }
                  } else if (guide.position === "End") {
                    // Only highlight if pattern is at the end
                    if (lowerWord.endsWith(lowerPattern)) {
                      patternIndex = example.target.length - pattern.length;
                    }
                  } else {
                    // Highlight anywhere in the word
                    patternIndex = lowerWord.indexOf(lowerPattern);
                  }

                  let highlightedWord;
                  if (patternIndex !== -1) {
                    const before = example.target.slice(0, patternIndex);
                    const matched = example.target.slice(
                      patternIndex,
                      patternIndex + matchLength
                    );
                    const after = example.target.slice(
                      patternIndex + matchLength
                    );
                    highlightedWord = (
                      <>
                        {before}
                        <span className="bg-yellow-500/30 rounded px-0.5">
                          {matched}
                        </span>
                        {after}
                      </>
                    );
                  } else {
                    highlightedWord = example.target;
                  }

                  return (
                    <div
                      key={index}
                      className="bg-background/50 rounded p-3 flex items-center justify-between"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <div className="flex-1">
                        <div className="text-base">
                          <span className="font-medium">{pattern}</span>
                          <span className="text-muted-foreground mx-2">
                            {connector}
                          </span>
                          <span className="font-semibold">
                            {highlightedWord}
                          </span>
                        </div>
                        {example.cultural_context && (
                          <div className="text-xs text-muted-foreground mt-1">
                            {example.cultural_context}
                          </div>
                        )}
                      </div>
                      <AudioButton
                        audioRequest={{
                          request: {
                            text: `"${pattern}" ${connector} "${example.target}"`,
                            language: targetLanguage,
                          },
                          provider: "Google",
                        }}
                        accessToken={accessToken}
                        autoPlay={false}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {guide.description && (
            <div className="pt-3 border-t border-muted/20">
              <div className="text-sm text-muted-foreground">
                {guide.description}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  } else if ("Multiword" in content) {
    return (
      <div className="text-left bg-muted/30 rounded-lg p-4 space-y-2">
        <div className="flex items-baseline gap-2">
          <span className="text-xl font-medium">
            {content.Multiword[1].meaning}
          </span>
        </div>

        {content.Multiword[1].example_sentence_target_language && (
          <div className="space-y-1 text-sm">
            <div className="flex items-start gap-2">
              <p className="text-muted-foreground italic flex-1">
                "{content.Multiword[1].example_sentence_target_language}"
              </p>
              <div onClick={(e) => e.stopPropagation()}>
                <AudioButton
                  audioRequest={{
                    request: {
                      text: content.Multiword[1]
                        .example_sentence_target_language,
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
            <p className="text-muted-foreground">
              "{content.Multiword[1].example_sentence_native_language}"
            </p>
          </div>
        )}
      </div>
    );
  } else {
    return <div>Unknown card type</div>;
  }
};

export const Flashcard = function Flashcard({
  audioRequest,
  content,
  showAnswer,
  onToggle,
  totalCount,
  onRating,
  accessToken,
  onCantListen,
  onCantSpeak,
  isNew,
  targetLanguage,
  listeningPrefix,
  autoplayed,
  setAutoplayed,
}: FlashcardProps) {
  const x = useMotionValue(0);
  const controls = animationControls();
  const [isDragging, setIsDragging] = useState(false);
  const [showReportModal, setShowReportModal] = useState(false);
  const [hasBeenOpened, setHasBeenOpened] = useState(false);
  const { bumpBackground } = useBackground();

  const leftLabel = isNew ? "Didn't know" : "Forgot";
  const rightLabel = isNew ? "Already knew" : "Remembered";

  const requireShowAnswer = totalCount < 50;
  const canGrade = hasBeenOpened || showAnswer || !requireShowAnswer;

  const rotate = useTransform(x, [-200, 200], [-30, 30]);

  // Color overlay for visual feedback
  const leftOverlayOpacity = useTransform(x, [-200, 0], [1, 0]);
  const rightOverlayOpacity = useTransform(x, [0, 200], [0, 1]);

  const handleDragEnd = async (
    _event: MouseEvent | TouchEvent | PointerEvent,
    info: PanInfo
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
        onToggle();
      }
      // Hide answer: ↑ / k
      else if (showAnswer && (e.key === "ArrowUp" || e.key === "k")) {
        e.preventDefault();
        onToggle();
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
  }, [showAnswer, canGrade, onToggle, onRating, isNew]);

  const copyWord = () => {
    let word: string | undefined;
    if ("Heteronym" in content) {
      word = content.Heteronym.heteronym.word;
    } else if ("Multiword" in content) {
      word = content.Multiword[0];
    } else if ("Listening" in content) {
      const possible = content.Listening.possible_words;
      if (possible.length > 0) {
        word = possible[0][1];
      }
    } else if ("LetterPronunciation" in content) {
      word = content.LetterPronunciation.pattern;
    }

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
      <motion.div
        className="relative w-full flex-1"
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
              onToggle();
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
                {!("LetterPronunciation" in content) && audioRequest ? (
                  <AudioButton
                    audioRequest={audioRequest}
                    accessToken={accessToken}
                    autoPlay={true}
                    autoplayed={autoplayed}
                    setAutoplayed={setAutoplayed}
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
                      <Button variant="ghost" size="icon" className="h-10 w-10">
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
                  Show Answer
                </div>
                <kbd className="h-6 w-6 text-xs font-semibold border rounded bg-muted/20 border flex items-center justify-center hide-kbd-border-mobile">
                  <ArrowDown className="h-3 w-3 text-muted-foreground" />
                </kbd>
              </div>
            )}
          </div>
        </Card>
      </motion.div>

      {onRating && (
        <div className={`mt-4 flex flex-col gap-2 transition-opacity duration-300 ${!canGrade ? "opacity-0" : "opacity-100"}`}>
          {onCantListen && "Listening" in content && (
            <CantListenButton onClick={onCantListen} />
          )}
          {onCantSpeak && "LetterPronunciation" in content && (
            <CantSpeakButton onClick={onCantSpeak} />
          )}
          <div className="grid grid-cols-2 gap-2">
            <Button
              onClick={() => {
                if (!canGrade) return;
                bumpBackground(30.0);
                window.scrollTo({ top: 0, behavior: "smooth" });
                onRating("again");
              }}
              variant="destructive"
              size="lg"
              className="h-14 group"
              disabled={!canGrade}
            >
              <span className="flex items-center gap-2">
                <kbd className="h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center hide-kbd-mobile opacity-0 group-hover:opacity-100 transition-opacity">
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
              className="h-14 group"
              disabled={!canGrade}
            >
              <span className="flex items-center gap-2">
                {rightLabel}
                <kbd className="h-6 w-6 text-xs font-semibold border rounded bg-background/20 border-background/40 flex items-center justify-center hide-kbd-mobile opacity-0 group-hover:opacity-100 transition-opacity">
                  <ArrowRight className="h-3 w-3" />
                </kbd>
              </span>
            </Button>
          </div>
        </div>
      )}

      <ReportIssueModal
        context={`${JSON.stringify(content)}`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  );
};
