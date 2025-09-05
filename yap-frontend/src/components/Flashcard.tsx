import { type AudioRequest, type CardContent, type Language } from '../../../yap-frontend-rs/pkg'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { MoreVertical } from "lucide-react"
import { Button } from "@/components/ui/button"
import { motion, useMotionValue, useTransform, useAnimation, type PanInfo } from "framer-motion"
import { AnimatedCard } from "./AnimatedCard"
import { useEffect, useState, memo } from "react"
import "./Flashcard.css"
import { AudioButton } from "./AudioButton"
import { ReportIssueModal } from "./challenges/ReportIssueModal"
import { CantListenButton } from "./CantListenButton"
import { AudioVisualizer } from "./AudioVisualizer"
import { CardsRemaining } from "./CardsRemaining"
import { toast } from 'sonner'

interface FlashcardProps {
  audioRequest: AudioRequest
  content: CardContent<string>
  showAnswer: boolean
  onToggle: () => void
  dueCount: number
  totalCount: number
  onRating?: (rating: 'again' | 'hard' | 'good' | 'easy') => void
  accessToken: string | undefined
  onCantListen?: () => void
  isNew: boolean
  targetLanguage: Language
  listeningPrefix?: string
}

const CardFront = ({ content, listeningPrefix }: { content: CardContent<string>, listeningPrefix?: string }) => {
  if ("Listening" in content) {
    const prefix = listeningPrefix || "Le mot est"
    return (
      <h2 className="text-3xl font-semibold flex items-center gap-3 flex-col">
        {prefix}...
        <AudioVisualizer />
      </h2>
    )
  }
  else if ("Heteronym" in content) {
    return (<h2 className="text-3xl font-semibold">{content.Heteronym[0].word}</h2>)
  }
  else {
    return (<h2 className="text-3xl font-semibold">{content.Multiword[0]}</h2>)
  }
}

const CardFrontSubtitle = ({ content }: { content: CardContent<string> }) => {
  if ("Listening" in content) {
    return <span className="text-sm text-muted-foreground"> Fill in the blank!</span>
  }

  const partOfSpeech = "Heteronym" in content ?
    (content.Heteronym[0].pos == "ADJ" ? "Adjective" :
      content.Heteronym[0].pos == "ADP" ? "Adposition" :
        content.Heteronym[0].pos == "ADV" ? "Adverb" :
          content.Heteronym[0].pos == "AUX" ? "Auxiliary" :
            content.Heteronym[0].pos == "CCONJ" ? "Conjunction" :
              content.Heteronym[0].pos == "DET" ? "Determiner" :
                content.Heteronym[0].pos == "INTJ" ? "Interjection" :
                  content.Heteronym[0].pos == "NOUN" ? "Noun" :
                    content.Heteronym[0].pos == "NUM" ? "Number" :
                      content.Heteronym[0].pos == "PART" ? "Particle" :
                        content.Heteronym[0].pos == "PRON" ? "Pronoun" :
                          content.Heteronym[0].pos == "PROPN" ? "Proper Noun" :
                            content.Heteronym[0].pos == "PUNCT" ? "Punctuation" :
                              content.Heteronym[0].pos == "SCONJ" ? "Subordinating Conjunction" :
                                content.Heteronym[0].pos == "SYM" ? "Symbol" :
                                  content.Heteronym[0].pos == "VERB" ? "Verb" :
                                    content.Heteronym[0].pos == "X" ? "Unknown" : "Unknown")
    : "Multiword";
  return <span className="text-sm text-muted-foreground">({partOfSpeech})</span>
}

const CardBack = ({ content }: { content: CardContent<string> }) => {
  if ("Listening" in content) {
    const possible_words: [boolean, string][] = content.Listening.possible_words;

    if (possible_words.length === 1) {
      return (
        <div className="text-3xl font-medium">{possible_words[0][1]}</div>
      )
    }

    return (
      <div className="space-y-4">
        <div className="text-sm text-muted-foreground">It could have been any of these words:</div>
        <div className="grid grid-cols-2 gap-2">
          {possible_words.map(([isKnown, word], index) => (
            <div
              key={index}
              className={`text-left p-2 rounded-md ${isKnown
                ? 'bg-green-500/10 border border-green-500/20'
                : 'bg-muted/30 border border-muted/20'
                }`}
            >
              <span className="text-lg">{word}</span>
              {isKnown && <span className="text-sm text-green-600 ml-2">(known)</span>}
            </div>
          ))}
        </div>
      </div>
    )
  }
  else if ("Heteronym" in content) {
    return content.Heteronym[1].map((def, index) => (
      <div key={index} className="text-left bg-muted/30 rounded-lg p-4 space-y-2">
        <div className="flex items-baseline gap-2">
          <span className="text-xl font-medium">{def.native}</span>
        </div>

        {def.example_sentence_target_language && (
          <div className="space-y-1 text-sm">
            <p className="text-muted-foreground italic">"{def.example_sentence_target_language}"</p>
            <p className="text-muted-foreground">"{def.example_sentence_native_language}"</p>
          </div>
        )}
      </div>
    ))
  }
  else {
    return (
      <div className="text-left bg-muted/30 rounded-lg p-4 space-y-2">
        <div className="flex items-baseline gap-2">
          <span className="text-xl font-medium">{content.Multiword[1].meaning}</span>
        </div>

        {content.Multiword[1].example_sentence_target_language && (
          <div className="space-y-1 text-sm">
            <p className="text-muted-foreground italic">"{content.Multiword[1].example_sentence_target_language}"</p>
            <p className="text-muted-foreground">"{content.Multiword[1].example_sentence_native_language}"</p>
          </div>
        )}
      </div>
    )
  }
}

export const Flashcard = memo(function Flashcard({ audioRequest, content, showAnswer, onToggle, dueCount, totalCount, onRating, accessToken, onCantListen, isNew, targetLanguage, listeningPrefix }: FlashcardProps) {
  const x = useMotionValue(0)
  const controls = useAnimation()
  const [isDragging, setIsDragging] = useState(false)
  const [showReportModal, setShowReportModal] = useState(false)

  const leftLabel = isNew ? "Didn't know" : "Forgot"
  const rightLabel = isNew ? "Already knew" : "Good"

  const requireShowAnswer = totalCount < 30
  const canGrade = showAnswer || !requireShowAnswer

  const rotate = useTransform(x, [-200, 200], [-30, 30])
  const opacity = useTransform(x, [-200, -100, 0, 100, 200], [0, 1, 1, 1, 0])

  // Color overlay for visual feedback
  const leftOverlayOpacity = useTransform(x, [-200, 0], [1, 0])
  const rightOverlayOpacity = useTransform(x, [0, 200], [0, 1])

  const handleDragEnd = async (_event: MouseEvent | TouchEvent | PointerEvent, info: PanInfo) => {
    setIsDragging(false)
    const threshold = 100

    if (!canGrade) {
      controls.start({ x: 0, transition: { type: "spring", stiffness: 300, damping: 20 } })
      return
    }

    if (info.offset.x > threshold && info.velocity.x > 0) {
      // Swiped right - Good
      await controls.start({ x: 300, opacity: 0, transition: { duration: 0.2 } })
      if (onRating) {
        window.scrollTo({ top: 0, behavior: 'smooth' })
        onRating('good')
      }
    } else if (info.offset.x < -threshold && info.velocity.x < 0) {
      // Swiped left - Again
      await controls.start({ x: -300, opacity: 0, transition: { duration: 0.2 } })
      if (onRating) {
        window.scrollTo({ top: 0, behavior: 'smooth' })
        onRating('again')
      }
    } else {
      // Not enough swipe - snap back
      controls.start({ x: 0, transition: { type: "spring", stiffness: 300, damping: 20 } })
    }
  }

  // Reset position and animate in
  useEffect(() => {
    // Reset to initial state instantly, then animate in
    controls.set({ x: 0, opacity: 0, scale: 0.95 })
    controls.start({
      x: 0,
      opacity: 1,
      scale: 1,
      transition: {
        duration: 0.3,
        ease: "easeOut"
      }
    })
  }, [controls])


  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      if (e.key === 'Enter') {
        e.preventDefault();
        toast('Use the arrow keys');
        return;
      }

      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
      }

      // Show answer: Space / ↓ / j (when answer is hidden)
      if (!showAnswer && (e.key === ' ' || e.key === 'ArrowDown' || e.key === 'j')) {
        e.preventDefault();
        onToggle();
      }
      // Hide answer: ↑ / k
      else if (showAnswer && (e.key === 'ArrowUp' || e.key === 'k')) {
        e.preventDefault();
        onToggle();
      }
      // Mark as "good" when answer is shown: Space
      else if (canGrade && e.key === 'ArrowRight' && !e.shiftKey) {
        e.preventDefault();
        if (onRating) {
          window.scrollTo({ top: 0, behavior: 'smooth' });
          onRating('good');
        }
      }
      // Mark as "again": f or ←
      else if (canGrade && (e.key === 'f' || e.key === 'ArrowLeft')) {
        e.preventDefault();
        if (onRating) {
          window.scrollTo({ top: 0, behavior: 'smooth' });
          onRating('again');
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showAnswer, canGrade, onToggle, onRating]);

  return (
    <div className="flex flex-col flex-1 justify-between">
      <AnimatedCard
        className="relative w-full flex-1"
        drag="x"
        dragConstraints={{ left: 0, right: 0 }}
        onDragStart={() => setIsDragging(true)}
        onDragEnd={handleDragEnd}
        animate={controls}
        style={{ x, rotate, opacity }}
      >
        <div
          className={`bg-card text-card-foreground rounded-lg pt-3 pb-3 pl-3 pr-3 cursor-pointer transition-all hover:shadow-lg border flex flex-col relative overflow-hidden flashcard h-full ${!showAnswer ? "spin-on-hover" : ""}`}
          onClick={() => {
            if (!isDragging) {
              onToggle()
            }
          }}
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

          <div className="text-center relative z-10">
            <div className="mb-4 justify-center gap-2 flex flex-col items-center w-full">
              <div className="flex items-center justify-between w-full" onClick={(e) => e.stopPropagation()}>
                <AudioButton
                  audioRequest={audioRequest}
                  accessToken={accessToken}
                  autoPlay={true}
                />

                <CardFront content={content} listeningPrefix={listeningPrefix} />

                {onRating ? (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button variant="ghost" size="icon" className="h-10 w-10">
                        <MoreVertical className="h-6 w-6 size--xl" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem onClick={() => onRating('hard')}>
                        Hard
                      </DropdownMenuItem>
                      <DropdownMenuItem onClick={() => onRating('easy')}>
                        Easy
                      </DropdownMenuItem>
                      <DropdownMenuItem onClick={() => setShowReportModal(true)}>
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

            <hr className="my-4" />

            {showAnswer ? (
              <motion.div
                className="space-y-6"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.2 }}
              >
                <CardBack content={content} />
              </motion.div>)
              : (
                <div>
                  <div className={`text-muted-foreground ${requireShowAnswer ? 'font-bold' : ''}`}>
                    Show Answer
                  </div>
                  <div className="text-muted-foreground">
                    ↓
                  </div>
                </div>)
            }
          </div>
        </div>

        <CardsRemaining
          dueCount={dueCount}
          totalCount={totalCount}
          className="mt-4"
        />
      </AnimatedCard>

      {onRating && (
        <div className="mt-4 flex flex-col gap-2">
          {onCantListen && "Listening" in content && (
            <CantListenButton onClick={onCantListen} />
          )}
          <div className="grid grid-cols-2 gap-2">
            <Button
              onClick={() => {
                if (!canGrade) return
                window.scrollTo({ top: 0, behavior: 'smooth' })
                onRating('again')
              }}
              variant="destructive"
              size="lg"
              className="h-14"
              disabled={!canGrade}
            >
              {leftLabel}
            </Button>
            <Button
              onClick={() => {
                if (!canGrade) return
                window.scrollTo({ top: 0, behavior: 'smooth' })
                onRating('good')
              }}
              variant="default"
              size="lg"
              className="h-14"
              disabled={!canGrade}
            >
              {rightLabel}
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
  )
})
