import { useState, useEffect, useRef, useCallback, forwardRef, useImperativeHandle } from 'react'
import { type TranslateComprehensibleSentence, type Lexeme, type Literal, type TargetToNativeWord, autograde_translation, type Heteronym, type Language } from '../../../../yap-frontend-rs/pkg/yap_frontend_rs'
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { motion, useMotionValue, useTransform, useAnimation, type PanInfo } from "framer-motion"
import { Check, X, MoreVertical } from "lucide-react"
import { AudioButton } from "../AudioButton"
import { ReportIssueModal } from "./ReportIssueModal"
import Markdown from 'react-markdown'
import { playSoundEffect } from '@/lib/sound-effects'
import { CardsRemaining } from "../CardsRemaining"
import { AnimatedCard } from "../AnimatedCard"

interface SentenceChallengeProps {
  sentence: TranslateComprehensibleSentence<string>
  onComplete: (grade: { wordStatuses: [Lexeme<string>, boolean | null][] } | { perfect: string | null }, submission: string) => void
  dueCount: number
  totalCount: number
  unique_target_language_lexeme_definitions: [Lexeme<string>, TargetToNativeWord[]][]
  accessToken: string | undefined
  targetLanguage: Language
}

interface ChallengeSentenceProps {
  literals: Literal<string>[]
  onWordTap: (heteronym: Heteronym<string>) => void
  wordStatuses?: [Lexeme<string>, boolean | null][]
  isPerfect?: boolean
  tappedWords: Set<string> // JSON stringified Lexemes
  uniqueTargetLanguageLexemes: Lexeme<string>[]
}

interface SwipeableWordProps {
  lexeme: Lexeme<string>
  aliased: boolean
  onSwipe: (lexeme: Lexeme<string>, remembered: boolean) => void
  isSelected?: boolean
  status?: boolean | null // true = remembered, false = forgot, null = not graded
}

export interface SwipeableWordHandle {
  handleButtonClick: (remembered: boolean) => void
}

const SwipeableWord = forwardRef<SwipeableWordHandle, SwipeableWordProps>(
  ({ lexeme, aliased, onSwipe, isSelected = false, status = null }, ref) => {
    const x = useMotionValue(0)
    const controls = useAnimation()

    const background = useTransform(
      x,
      [-150, 0, 150],
      ['rgba(239, 68, 68, 0.2)', 'rgba(0, 0, 0, 0)', 'rgba(34, 197, 94, 0.2)',]
    )

    const handleDragEnd = async (_event: MouseEvent | TouchEvent | PointerEvent, info: PanInfo) => {
      const velocityThreshold = 5
      const positionThreshold = 50

      if (info.velocity.x < -velocityThreshold || (info.velocity.x > velocityThreshold && info.offset.x < -positionThreshold)) {
        // Swiped left - forgot
        await controls.start({ x: -60 })
        onSwipe(lexeme, false)
      } else if (info.velocity.x > velocityThreshold || (info.velocity.x < -velocityThreshold && info.offset.x > positionThreshold)) {
        // Swiped right - remembered
        await controls.start({ x: 60 })
        onSwipe(lexeme, true)
      }
    }

    const handleButtonClick = useCallback(async (remembered: boolean) => {
      if (remembered) {
        await controls.start({ x: 60 })
        onSwipe(lexeme, true)
      } else {
        await controls.start({ x: -60 })
        onSwipe(lexeme, false)
      }
    }, [controls, onSwipe, lexeme])

    useImperativeHandle(ref, () => ({
      handleButtonClick
    }), [handleButtonClick])

    // Set initial position based on status
    useEffect(() => {
      if (status === true) {
        // Remembered - move right
        controls.start({ x: 60 })
      } else if (status === false) {
        // Forgot - move left  
        controls.start({ x: -60 })
      } else {
        // Not graded - center
        controls.start({ x: 0 })
      }
    }, [status, controls])

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
            className={`relative px-6 py-3 bg-card border rounded-lg cursor-grab active:cursor-grabbing select-none ${isSelected ? 'ring-2 ring-primary' : ''
              }`}
          >
            <p className="text-lg font-medium text-center">
              {"Heteronym" in lexeme ? lexeme.Heteronym.word : lexeme.Multiword}
              <span className="text-sm text-muted-foreground">
                {aliased ? "Heteronym" in lexeme ? ` (${lexeme.Heteronym.pos})` : "" : ""}
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
    )
  })

SwipeableWord.displayName = 'SwipeableWord'

function YourTranslation({ userTranslation }: { userTranslation: string }) {
  return (
    <div className="rounded-lg p-4 border">
      <p className="text-sm font-medium mb-1">Your translation:</p>
      <p className="text-lg font-medium">{userTranslation}</p>
    </div>
  )
}

function CorrectTranslation({ sentence }: { sentence: string }) {
  return (
    <div className="bg-green-500/10 rounded-lg p-4 border border-green-500/20">
      <p className="text-sm font-medium text-green-600 dark:text-green-400 mb-1">Correct translation:</p>
      <p className="text-lg font-medium">{sentence}</p>
    </div>
  )
}

function FeedbackSkeleton() {
  return (
    <motion.div
      className="space-y-4 mt-4"
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
    >
      <div className="space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    </motion.div>
  )
}

function Feedback({ feedback }: { feedback: string }) {
  const processedFeedback = feedback
    .replace(/• /g, '- ')

  return (
    <div className={`rounded-lg p-4 border bg-blue-500/10 border-blue-500/20`}>
      <p className={`text-sm font-medium mb-1 text-blue-600 dark:text-blue-400`}>
        Feedback:
      </p>
      <Markdown>{processedFeedback}</Markdown>
    </div>
  )
}

function AutogradeError() {
  return (
    <div className={`rounded-lg p-4 border bg-yellow-500/10 border-yellow-500/20`}>
      <p className={`text-sm font-medium mb-1 text-yellow-600 dark:text-yellow-400`}>
        Your submission could not be graded automatically. Please grade the words manually below.
      </p>
    </div>
  )
}

interface WordStatusesProps {
  selectedWordIndex: number;
  sentence: TranslateComprehensibleSentence<string>;
  setSelectedWordIndex: (index: number) => void;
  wordStatuses: [Lexeme<string>, boolean | null][];
  wordRefs: React.RefObject<(SwipeableWordHandle | null)[]>;
  handleWordSwipe: (lexeme: Lexeme<string>, remembered: boolean) => void;
  definitions: [Lexeme<string>, TargetToNativeWord[]][];
}

function WordDefinition({ lexeme, definitions }: { lexeme: Lexeme<string>, definitions: TargetToNativeWord[] }) {
  if (!definitions || definitions.length === 0) {
    return null;
  }

  const lexemeText = "Heteronym" in lexeme ? lexeme.Heteronym.word : lexeme.Multiword;

  return (
    <div className="mt-2 p-3 bg-secondary rounded-md">
      <p className="text-sm font-semibold">{lexemeText}:</p>
      <ul className="list-disc list-inside text-sm">
        {definitions.map((def, i) => (
          <li key={i}>
            {def.native}
            {def.note && <span className="text-xs text-muted-foreground"> ({def.note})</span>}
          </li>
        ))}
      </ul>
    </div>
  );
}


function WordDefinitions({ wordStatuses, definitions }: { wordStatuses: [Lexeme<string>, boolean | null][], definitions: [Lexeme<string>, TargetToNativeWord[]][] }) {
  const getDefinitionsForLexeme = (lexeme: Lexeme<string>): TargetToNativeWord[] => {
    const found = definitions.find(([l]) => JSON.stringify(l) === JSON.stringify(lexeme));
    return found ? found[1] : [];
  };

  return (
    <div>
      {wordStatuses.map(([lexeme, status], index) => (
        status === false && (
          <WordDefinition key={index} lexeme={lexeme} definitions={getDefinitionsForLexeme(lexeme)} />
        )
      ))}
    </div>
  )
}

function WordStatuses({
  selectedWordIndex,
  sentence,
  setSelectedWordIndex,
  wordStatuses,
  wordRefs,
  handleWordSwipe,
}: WordStatusesProps) {
  const [isAnswerOpen, setIsAnswerOpen] = useState(false)

  return <Collapsible open={isAnswerOpen} onOpenChange={setIsAnswerOpen}>
    <CollapsibleTrigger asChild>
      <Button variant="ghost" className="w-full justify-between p-0">
        <span className="text-sm font-medium">
          Grade Words
        </span>
        <span className="text-xs text-muted-foreground">
          {isAnswerOpen ? "Hide" : "Show"}
        </span>
      </Button>
    </CollapsibleTrigger>
    <CollapsibleContent>
      {/* Initialize selection when words are shown */}
      {selectedWordIndex === -1 && sentence.unique_target_language_lexemes.length > 0 && (() => {
        setSelectedWordIndex(0)
        return null
      })()}
      <div className="text-center space-y-1">
        <p className="text-sm font-medium text-muted-foreground pb-2">
          Mark as remembered (✓) or forgot (✗). Tap a word to see its definition.
        </p>
      </div>

      <div className="space-y-2">
        {wordStatuses.map(([lexeme, status], index) => (
          <div key={index}>
            <SwipeableWord
              ref={(el) => { wordRefs.current[index] = el }}
              lexeme={lexeme}
              aliased={false} // TODO: need to fix this
              onSwipe={handleWordSwipe}
              isSelected={selectedWordIndex === index}
              status={status} />
          </div>
        ))}
      </div>
    </CollapsibleContent>
  </Collapsible>
}



function ChallengeSentence({ literals, wordStatuses, isPerfect, onWordTap, tappedWords }: ChallengeSentenceProps) {
  // Helper function to get the color class for a literal
  const getLiteralColorClass = (literal: Literal<string>) => {
    // If perfect, all literals are green
    if (isPerfect) {
      return 'text-green-600 dark:text-green-400'
    }

    const heteronym = "heteronym" in literal && literal.heteronym !== undefined ? literal.heteronym : undefined;

    if (heteronym && tappedWords.has(JSON.stringify(heteronym))) {
      return 'text-yellow-500 dark:text-yellow-400' // Color for tapped words
    }

    // If no word statuses, use default color
    if (!wordStatuses || !heteronym) {
      return ''
    }

    // Find if this literal belongs to any of the graded lexemes
    const lexeme: Lexeme<string> = { Heteronym: heteronym };
    const statusEntry = wordStatuses.find(([l]) => JSON.stringify(l) === JSON.stringify(lexeme))
    if (statusEntry) {
      const status = statusEntry[1]
      if (status === true) return 'text-green-600 dark:text-green-400'
      if (status === false) return 'text-red-600 dark:text-red-400'
    }

    return ''
  }

  return (
    <h2 className="text-2xl font-semibold">
      {literals.map((literal, i) => {
        const colorClass = getLiteralColorClass(literal);
        const heteronym = "heteronym" in literal && literal.heteronym !== undefined ? literal.heteronym : undefined;

        return (
          <span
            key={i}
            className={`${colorClass} ${heteronym ? 'cursor-pointer hover:underline' : ''}`}
            onClick={() => {
              if (heteronym) {
                onWordTap(heteronym);
              }
            }}
          >
            {literal.text}
            {literal.whitespace}
          </span>
        );
      })
      }
    </h2>
  );
}

export function TranslationChallenge({ sentence, onComplete, dueCount, totalCount, unique_target_language_lexeme_definitions, accessToken, targetLanguage }: SentenceChallengeProps) {
  const [userTranslation, setUserTranslation] = useState('')
  const [correctTranslation, setCorrectTranslation] = useState(sentence.native_translations[0])
  const [selectedWordIndex, setSelectedWordIndex] = useState<number>(-1)
  const [showReportModal, setShowReportModal] = useState(false)
  const [tappedWords, setTappedWords] = useState<Set<string>>(new Set())
  const [showDefinitionFor, setShowDefinitionFor] = useState<Lexeme<string> | null>(null);
  const [grade, setGrade] = useState<
    {
      graded: {
        wordStatuses: [Lexeme<string>, boolean | null][],
        explanation?: string,
        autogradingError?: string
      } |
      { perfect: string | null, explanation?: string }
    }
    | { grading: null }
    | null
  >(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const wordRefs = useRef<(SwipeableWordHandle | null)[]>([])

  // Reset tapped words and definition display when the sentence changes
  useEffect(() => {
    setTappedWords(new Set());
    setShowDefinitionFor(null);
    setCorrectTranslation(sentence.native_translations[0]);
  }, [sentence.target_language, sentence.native_translations]);

  const handleWordTap = (heteronym: Heteronym<string>) => {
    setTappedWords(prev => new Set(prev).add(JSON.stringify(heteronym)));
    setShowDefinitionFor({ Heteronym: heteronym });
    // If grading has already happened, and the tapped word was marked true, mark it false
    // This part will be handled more thoroughly in the grading logic modification step
  };

  // Clear tapped words when moving to manual grading or if a perfect grade was achieved and then a word is tapped.
  // This is to ensure that the yellow highlight is removed once the grading state is active.
  useEffect(() => {
    if (grade && 'graded' in grade) {
      setShowDefinitionFor(null); // Hide definition when grading starts or is complete
    }
  }, [grade]);

  const canContinue =
    (grade && 'graded' in grade && ('perfect' in grade.graded || grade.graded.wordStatuses.every(([lexeme, status]) => (JSON.stringify(lexeme) !== JSON.stringify(sentence.primary_expression)) || status !== null)))

  // Focus when input should be visible (component mount or when returning to input)
  useEffect(() => {
    const timer = setTimeout(() => {
      inputRef.current?.focus()
    }, 100)
    return () => clearTimeout(timer)
  }, [sentence.target_language])

  const handleCheckAnswer = useCallback(async () => {
    // Normalize text by removing punctuation, converting to lowercase, and expanding contractions
    const normalizeText = (text: string): string => {
      // First normalize all apostrophes to straight apostrophes
      text = text.replace(/'/g, "'")
      text = text.replace(/’/g, "'")
      text = text.replace(/‘/g, "'")
      text = text.replace(/“/g, '"')
      text = text.replace(/”/g, '"')

      // Common contractions mapping
      const contractions: { [key: string]: string } = {
        "it's": "it is",
        "that's": "that is",
        "what's": "what is",
        "where's": "where is",
        "who's": "who is",
        "there's": "there is",
        "here's": "here is",
        "he's": "he is",
        "she's": "she is",
        "i'm": "i am",
        "you're": "you are",
        "we're": "we are",
        "they're": "they are",
        "i've": "i have",
        "you've": "you have",
        "we've": "we have",
        "they've": "they have",
        "i'd": "i would",
        "you'd": "you would",
        "he'd": "he would",
        "she'd": "she would",
        "we'd": "we would",
        "they'd": "they would",
        "i'll": "i will",
        "you'll": "you will",
        "he'll": "he will",
        "she'll": "she will",
        "we'll": "we will",
        "they'll": "they will",
        "won't": "will not",
        "wouldn't": "would not",
        "shouldn't": "should not",
        "couldn't": "could not",
        "can't": "cannot",
        "don't": "do not",
        "doesn't": "does not",
        "didn't": "did not",
        "isn't": "is not",
        "aren't": "are not",
        "wasn't": "was not",
        "weren't": "were not",
        "hasn't": "has not",
        "haven't": "have not",
        "hadn't": "had not",
      }

      let normalized = text.toLowerCase()

      // Replace contractions
      Object.entries(contractions).forEach(([contraction, expansion]) => {
        const regex = new RegExp(`\\b${contraction}\\b`, 'g')
        normalized = normalized.replace(regex, expansion)
      })

      // Remove punctuation and normalize whitespace
      return normalized
        .replace(/[.,!?;:'"()-]/g, '')
        .replace(/\s+/g, ' ')
        .trim()
    }

    // If any word was tapped, it's not a perfect score from the start.
    const levenshtein = (a: string, b: string): number => {
      const matrix = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0))
      for (let i = 0; i <= a.length; i++) matrix[i][0] = i
      for (let j = 0; j <= b.length; j++) matrix[0][j] = j
      for (let i = 1; i <= a.length; i++) {
        for (let j = 1; j <= b.length; j++) {
          const cost = a[i - 1] === b[j - 1] ? 0 : 1
          matrix[i][j] = Math.min(
            matrix[i - 1][j] + 1,
            matrix[i][j - 1] + 1,
            matrix[i - 1][j - 1] + cost
          )
        }
      }
      return matrix[a.length][b.length]
    }

    const getClosest = (input: string) => {
      const normalizedUser = normalizeText(input)
      return sentence.native_translations.reduce((best, current) => {
        return levenshtein(normalizeText(current), normalizedUser) <
          levenshtein(normalizeText(best), normalizedUser)
          ? current
          : best
      })
    }

    if (tappedWords.size > 0) {
      // Proceed to autograding, but ensure it won't be marked as 'perfect'.
      // The tapped words will be marked as forgotten after autograding.
    } else {
      const match = sentence.native_translations.find(
        t => normalizeText(userTranslation) === normalizeText(t)
      )
      if (match) {
        setCorrectTranslation(match)
        setGrade({ graded: { perfect: match } })
        playSoundEffect('perfect')
        return
      }
    }

    if (userTranslation.trim()) {
      const closest = getClosest(userTranslation)
      setCorrectTranslation(closest)
      setGrade({ grading: null })

      try {
        const response = await autograde_translation(
          sentence.target_language,
          userTranslation,
          sentence.primary_expression,
          sentence.unique_target_language_lexemes,
          accessToken,
          targetLanguage
        )

        const explanation = response.explanation
        let finalWordStatuses: [Lexeme<string>, boolean | null][] = []

        if (tappedWords.size === 0 && response.expressions_forgot.length === 0) {
          setGrade({ graded: { perfect: null, explanation } })
          playSoundEffect('perfect')
        } else {
          finalWordStatuses = sentence.unique_target_language_lexemes.map((lexeme) => {
            const lexemeStr = JSON.stringify(lexeme)
            const heteronym = "Heteronym" in lexeme && lexeme.Heteronym !== undefined ? lexeme.Heteronym : undefined;
            const heteronymStr = JSON.stringify(heteronym);
            if (heteronym && tappedWords.has(heteronymStr)) {
              return [lexeme, false] // Tapped words are marked as forgotten
            }
            if (response.expressions_remembered.some(rememberedLexeme =>
              JSON.stringify(rememberedLexeme) === lexemeStr)) {
              return [lexeme, true]
            } else if (response.expressions_forgot.some(forgotLexeme =>
              JSON.stringify(forgotLexeme) === lexemeStr)) {
              return [lexeme, false]
            }
            return [lexeme, null]
          })
          setGrade({ graded: { wordStatuses: finalWordStatuses, explanation } })
        }
      } catch (error) {
        console.error('Autograde failed:', error)
        const fallbackStatuses: [Lexeme<string>, boolean | null][] = sentence.unique_target_language_lexemes.map((lexeme) => {
          if (tappedWords.has(JSON.stringify(lexeme))) {
            return [lexeme, false] // Tapped words are forgotten even in fallback
          }
          return [lexeme, null]
        })
        setGrade({ graded: { wordStatuses: fallbackStatuses, autogradingError: error instanceof Error ? error.message : 'Failed to grade automatically' } })
      }
    }
  }, [sentence, userTranslation, accessToken, tappedWords])

  const handleContinue = useCallback(() => {
    if (canContinue) {
      if (grade && "graded" in grade) {
        // Scroll to top when continuing
        window.scrollTo({ top: 0, behavior: 'smooth' })
        // The logic in handleCheckAnswer now ensures that if tappedWords.size > 0,
        // the grade will be in the { wordStatuses: ... } format, not { perfect: null }.
        // So, a specific conversion here is no longer necessary.
        onComplete(grade.graded, userTranslation)
      }
    }
  }, [canContinue, onComplete, grade, userTranslation])

  const handleWordSwipe = useCallback((lexeme: Lexeme<string>, remembered: boolean) => {
    setGrade(prevGrade => {
      if (prevGrade && "graded" in prevGrade && "wordStatuses" in prevGrade.graded) {
        const newStatuses = [...prevGrade.graded.wordStatuses]
        const index = newStatuses.findIndex(([lexeme_other]) => lexeme_other === lexeme)
        if (index !== -1) {
          newStatuses[index] = [lexeme, remembered]
        }
        return { graded: { wordStatuses: newStatuses, explanation: prevGrade.graded.explanation } }
      }
      return prevGrade
    })
  }, [])


  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Enter' && grade === null) {
        e.preventDefault()
        if (userTranslation.trim()) {
          handleCheckAnswer()
        }
      }
      else if (e.key === 'Enter' && grade && "graded" in grade) {
        e.preventDefault()
        handleContinue()
      }
      else if (e.key === 'ArrowRight' && grade && "graded" in grade && canContinue) {
        e.preventDefault()
        handleContinue()
        return
      }

      const wordStatuses = grade && 'graded' in grade && 'wordStatuses' in grade.graded && grade.graded.wordStatuses

      // Only handle arrow keys when words are shown and not all correct
      if (wordStatuses && wordStatuses.length > 0) {
        const lexemeCount = sentence.unique_target_language_lexemes.length

        switch (e.key) {
          case 'ArrowUp':
            e.preventDefault()
            setSelectedWordIndex(prev => {
              if (prev <= 0) return lexemeCount - 1
              return prev - 1
            })
            break

          case 'ArrowDown':
            e.preventDefault()
            setSelectedWordIndex(prev => {
              if (prev >= lexemeCount - 1) return 0
              return prev + 1
            })
            break

          case 'ArrowLeft':
            e.preventDefault()
            if (selectedWordIndex >= 0 && selectedWordIndex < lexemeCount) {
              const wordRef = wordRefs.current[selectedWordIndex]
              wordRef?.handleButtonClick(false)
            }
            break

          case 'ArrowRight':
            e.preventDefault()
            if (selectedWordIndex >= 0 && selectedWordIndex < lexemeCount) {
              const wordRef = wordRefs.current[selectedWordIndex]
              wordRef?.handleButtonClick(true)
            }
            break
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleContinue, sentence.unique_target_language_lexemes, selectedWordIndex, handleWordSwipe, canContinue, userTranslation, handleCheckAnswer, grade])

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div>
        <AnimatedCard
          className="bg-card text-card-foreground rounded-lg pt-3 pb-3 pl-3 pr-3 border relative"
        >
          <div className="space-y-6">
            <div className="text-center">
              <div className="flex items-center justify-between w-full">
                <AudioButton
                  audioRequest={sentence.audio}
                  accessToken={accessToken}
                  autoPlay={true}
                />

                <ChallengeSentence
                  literals={sentence.target_language_literals}
                  onWordTap={handleWordTap}
                  wordStatuses={grade && 'graded' in grade && 'wordStatuses' in grade.graded ? grade.graded.wordStatuses : undefined}
                  isPerfect={(grade && 'graded' in grade && 'perfect' in grade.graded) ?? undefined}
                  tappedWords={tappedWords}
                  uniqueTargetLanguageLexemes={sentence.unique_target_language_lexemes}
                />

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

            {showDefinitionFor && (
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -10 }}
                className="my-3"
              >
                <div className="relative">
                  <WordDefinition
                    lexeme={showDefinitionFor}
                    definitions={unique_target_language_lexeme_definitions.find(([l]) => JSON.stringify(l) === JSON.stringify(showDefinitionFor))?.[1] || []}
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => setShowDefinitionFor(null)}
                    className="absolute top-1 right-1 h-6 w-6"
                    aria-label="Close definition"
                  >
                    <X className="h-4 w-4" />
                  </Button>
                </div>
              </motion.div>
            )}

            {grade === null ? (
              <Input
                ref={inputRef}
                type="text"
                placeholder="Translation..."
                value={userTranslation}
                onChange={(e) => setUserTranslation(e.target.value)}
                className="text-lg"
              />
            ) :
              <motion.div
                className="space-y-4 mt-4"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.2 }}>{
                  "grading" in grade ?
                    <>
                      <div className="space-y-2">
                        <YourTranslation userTranslation={userTranslation} />
                        <CorrectTranslation sentence={correctTranslation} />
                        <FeedbackSkeleton />
                      </div>
                    </> :
                    "perfect" in grade.graded ? <>
                      <div className="space-y-2">
                        <CorrectTranslation sentence={correctTranslation} />

                        {"explanation" in grade.graded && grade.graded.explanation &&
                          <Feedback feedback={grade.graded.explanation} />}
                      </div>
                    </>
                      : <>
                        <div className="space-y-2">
                          <YourTranslation userTranslation={userTranslation} />
                          <CorrectTranslation sentence={correctTranslation} />
                        </div>

                        {"autogradingError" in grade.graded && grade.graded.autogradingError &&
                          <AutogradeError />}

                        {"explanation" in grade.graded && grade.graded.explanation &&
                          <Feedback feedback={grade.graded.explanation} />}

                        <WordStatuses
                          wordStatuses={grade.graded.wordStatuses}
                          wordRefs={wordRefs}
                          handleWordSwipe={handleWordSwipe}
                          selectedWordIndex={selectedWordIndex}
                          sentence={sentence}
                          setSelectedWordIndex={setSelectedWordIndex}
                          definitions={sentence.unique_target_language_lexeme_definitions}
                        />

                        <WordDefinitions wordStatuses={grade.graded.wordStatuses} definitions={sentence.unique_target_language_lexeme_definitions} />
                      </>
                }
              </motion.div>
            }
          </div>
        </AnimatedCard>
        <CardsRemaining
          dueCount={dueCount}
          totalCount={totalCount}
          className="mt-2"
        />
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
          {"grading" in grade ? "AI is grading..." :
            <>
              {'perfect' in grade.graded ? "Nailed it!" :
                "Continue"}
              <span className="ml-2 text-sm text-muted-foreground">(⏎)</span>
            </>
          }
        </Button>
      )}

      <ReportIssueModal
        context={`Sentence challenge: ${JSON.stringify(sentence)}"`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div>
  )
}
