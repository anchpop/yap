import { useState, useEffect, useRef, useMemo, useCallback } from 'react'
import { autograde_transcription, type TranscribeComprehensibleSentence, type PartGraded, type PartSubmitted, type WordGrade, type Language } from '../../../../yap-frontend-rs/pkg/yap_frontend_rs'
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { AudioButton } from "../AudioButton"
import { playSoundEffect } from '@/lib/sound-effects'
import { motion } from "framer-motion"
import { CantListenButton } from "../CantListenButton"
import Markdown from 'react-markdown'
import { AudioVisualizer } from "../AudioVisualizer"
import { CardsRemaining } from "../CardsRemaining"
import { AnimatedCard } from "../AnimatedCard"
import { AccentedCharacterKeyboard } from "../AccentedCharacterKeyboard"
import { MobileKeyboardTip } from "../MobileKeyboardTip"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { MoreVertical } from "lucide-react"
import { ReportIssueModal } from "./ReportIssueModal"

interface TranscriptionChallengeProps {
  challenge: TranscribeComprehensibleSentence<string>
  onComplete: (grade: PartGraded[]) => void
  dueCount: number
  totalCount: number
  accessToken: string | undefined
  onCantListen?: () => void
  targetLanguage: Language
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

type GradingState =
  | null // Not started
  | { grading: null } // Grading in progress
  | {
    graded: {
      results: PartGraded[];
      explanation: string | undefined;
      compare: string[];
      autograding_error?: string
    }
  };

export function TranscriptionChallenge({
  challenge,
  onComplete,
  dueCount,
  totalCount,
  accessToken,
  onCantListen,
  targetLanguage
}: TranscriptionChallengeProps) {
  const [userInputs, setUserInputs] = useState<Map<number, string>>(new Map())
  const [gradingState, setGradingState] = useState<GradingState>(null)
  const [showReportModal, setShowReportModal] = useState(false)
  const [isTranslationRevealed, setIsTranslationRevealed] = useState(false)
  const [focusedInputIndex, setFocusedInputIndex] = useState<number | null>(null)
  const inputRefs = useRef<(HTMLInputElement | null)[]>([])

  // Find indices of words that should be blanks
  const blankIndices: number[] = useMemo(() => {
    const blankIndices: number[] = [];
    challenge.parts.forEach((item, index) => {
      if ("AskedToTranscribe" in item) {
        blankIndices.push(index)
      }
    })
    return blankIndices;
  }, [challenge])

  // Focus first input on mount and reset translation reveal
  useEffect(() => {
    const firstBlankIndex = blankIndices[0]
    if (firstBlankIndex !== undefined) {
      setTimeout(() => {
        inputRefs.current[firstBlankIndex]?.focus()
      }, 100)
    }
    // Reset translation reveal state for new challenge
    setIsTranslationRevealed(false)
  }, [blankIndices])

  const handleInputChange = (index: number, value: string) => {
    const newInputs = new Map(userInputs)
    newInputs.set(index, value)
    setUserInputs(newInputs)
  }

  const handleCharacterInsert = (char: string) => {
    // Use the last focused input index, or the first blank if none was focused
    const targetIndex = focusedInputIndex !== null ? focusedInputIndex : blankIndices[0]

    if (targetIndex !== undefined) {
      const currentValue = userInputs.get(targetIndex) || ''
      const input = inputRefs.current[targetIndex]

      if (input) {
        // Focus the input first to get correct selection
        input.focus()

        const start = input.selectionStart || currentValue.length
        const end = input.selectionEnd || currentValue.length
        const newValue = currentValue.substring(0, start) + char + currentValue.substring(end)

        handleInputChange(targetIndex, newValue)

        // Set cursor position after the inserted character
        setTimeout(() => {
          if (input) {
            const newPosition = start + char.length
            input.setSelectionRange(newPosition, newPosition)
            input.focus()
            setFocusedInputIndex(targetIndex)
          }
        }, 0)
      }
    }
  }

  const allBlanksFilledOut = blankIndices.every((index) =>
    userInputs.get(index)?.trim() !== undefined && userInputs.get(index)?.trim() !== ''
  )


  const handleSubmit = useCallback(async () => {
    if (gradingState !== null) return

    setGradingState({ grading: null })

    const request: PartSubmitted[] = challenge.parts.map((part, index) => {
      if ("AskedToTranscribe" in part) {
        return {
          AskedToTranscribe: {
            parts: part.AskedToTranscribe.parts,
            submission: userInputs.get(index)?.trim() || ''
          }
        }
      } else {
        return {
          Provided: { part: part.Provided.part }
        }
      }
    })

    const graded = await autograde_transcription(request, accessToken, targetLanguage)
    const isAllCorrect = graded.results.every(result => 'Provided' in result || result.AskedToTranscribe.parts.every(part => 'Perfect' in part.grade))

    setGradingState({
      graded
    })

    if (isAllCorrect) {
      playSoundEffect('perfect')
    }
  }, [gradingState, challenge.parts, userInputs, accessToken, targetLanguage])

  // Global keyboard handler for Enter key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const activeElement = document.activeElement
      const isInputFocused = activeElement?.tagName === 'INPUT'

      if (e.key === 'Enter') {
        if (isInputFocused) {
          // Handle input navigation
          e.preventDefault()

          // Find which input is focused
          const currentIndex = inputRefs.current.findIndex(ref => ref === activeElement)
          if (currentIndex === -1) return

          // Find next blank
          const currentBlankPosition = blankIndices.findIndex(index => index === currentIndex)
          const nextBlankIndex = blankIndices[currentBlankPosition + 1]

          if (nextBlankIndex !== undefined) {
            // Focus next input
            inputRefs.current[nextBlankIndex]?.focus()
          } else if (gradingState === null && allBlanksFilledOut) {
            // This was the last input, submit
            handleSubmit()
          }
        } else if (gradingState && 'graded' in gradingState) {
          // Handle continue when graded and no input focused
          e.preventDefault()
          onComplete(gradingState.graded.results)
        }
      } else if (e.key === 'ArrowRight' && gradingState && 'graded' in gradingState && !isInputFocused) {
        e.preventDefault()
        onComplete(gradingState.graded.results)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [gradingState, onComplete, blankIndices, allBlanksFilledOut, handleSubmit])

  const renderSentenceWithBlanks = () => {
    return challenge.parts.map((item, index) => {
      if ("AskedToTranscribe" in item) {
        const asked_to_transcribe = item.AskedToTranscribe
        if (asked_to_transcribe.parts.length === 0) {
          throw new Error("AskedToTranscribe part has no parts")
        }
        const end_whitespace = asked_to_transcribe.parts[asked_to_transcribe.parts.length - 1].whitespace
        return (
          <span key={index}>
            <Input
              ref={el => { inputRefs.current[index] = el }}
              type="text"
              value={userInputs.get(index) || ''}
              onChange={(e) => handleInputChange(index, e.target.value)}
              onFocus={() => setFocusedInputIndex(index)}
              onBlur={() => {
                // Keep track of last focused input but allow blur
                // The accent keyboard will refocus when clicked
              }}
              disabled={gradingState !== null}
              className={`inline-block w-32 mx-1 text-center text-2xl font-semibold ${getInputClassName(index)}`}
              placeholder=""
            />
            <span>{end_whitespace}</span>
          </span>
        )
      } else {
        const provided = item.Provided.part
        return <span key={index}>{provided.text}{provided.whitespace}</span>
      }
    })
  }

  const getInputClassName = (index: number) => {
    if (gradingState && 'graded' in gradingState) {
      const blankPosition = blankIndices.findIndex((blankIndex) => blankIndex === index)
      const result = gradingState.graded.results[blankPosition]

      if ('Perfect' in result) {
        return 'border-green-500 bg-green-50 dark:bg-green-950'
      } else if ('PhoneticallyIdenticalButContextuallyIncorrect' in result) {
        return 'border-yellow-500 bg-yellow-50 dark:bg-yellow-950'
      } else if ('PhoneticallySimilarButContextuallyIncorrect' in result) {
        return 'border-orange-500 bg-orange-50 dark:bg-orange-950'
      } else if ('Incorrect' in result) {
        return 'border-red-500 bg-red-50 dark:bg-red-950'
      }
    }
    return 'border-muted-foreground/30'
  }

  return (
    <div className="flex flex-col flex-1 justify-between">
      <div>
        <AnimatedCard
          className="bg-card text-card-foreground rounded-lg pt-3 pb-3 pl-3 pr-3 border relative"
        >
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
            {/* Audio section with waveform */}
            <div className="flex flex-col items-center space-y-4">
              <div className="flex items-center gap-4">
                <AudioButton
                  audioRequest={challenge.audio}
                  accessToken={accessToken}
                  autoPlay={true}
                />

                <AudioVisualizer />
              </div>

              <p className="text-sm text-muted-foreground">Listen and fill in the blanks</p>
            </div>

            {/* Sentence with blanks */}
            <div className="text-center pt-4">
              <div className="text-2xl font-semibold leading-relaxed">
                {renderSentenceWithBlanks()}
              </div>
            </div>

            {/* Result feedback */}
            {gradingState && 'graded' in gradingState && (
              <motion.div
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.2 }}
                className="space-y-2"
              >
                <div className="rounded-lg p-4 border bg-green-500/10 border-green-500/20">
                  <p className="text-sm font-medium mb-1 text-green-600 dark:text-green-400">
                    Correct answer:
                  </p>
                  <p className="text-lg font-medium">{challenge.target_language}</p>
                </div>

                {"autograding_error" in gradingState.graded && gradingState.graded.autograding_error &&
                  <AutogradeError />}

                <WordGrades wordGrades={gradingState.graded.results} setGrade={results => { setGradingState({ ...gradingState, graded: { ...gradingState.graded, results: results } }) }} open_by_default={"autograding_error" in gradingState.graded && gradingState.graded.autograding_error !== undefined} />

                {gradingState.graded.explanation && (
                  <div className="rounded-lg p-4 border bg-blue-500/10 border-blue-500/20">
                    <p className="text-sm font-medium mb-1 text-blue-600 dark:text-blue-400">
                      Feedback:
                    </p>
                    <Markdown>{gradingState.graded.explanation}</Markdown>
                  </div>
                )}

                {Array.isArray(gradingState.graded.compare) && gradingState.graded.compare.length > 0 && (
                  <div className="rounded-lg p-4 border">
                    <div className="flex flex-row items-center gap-3">
                      <p className="text-sm font-medium">
                        Listen:
                      </p>
                      <div className="flex flex-row justify-around items-center gap-3">
                        {(gradingState.graded.compare).map((item, idx) => (
                          (
                            <div key={idx} className="flex items-center gap-1">
                              <span className="font-medium">{item}</span>
                              <AudioButton
                                audioRequest={{ request: { text: item, language: targetLanguage }, provider: "Google" }}
                                accessToken={accessToken}
                                size="icon"
                                variant="ghost"
                              />
                            </div>
                          )
                        ))}
                      </div>
                    </div>
                  </div>
                )}

                <div
                  className="rounded-lg p-4 border cursor-pointer select-none"
                  onClick={() => setIsTranslationRevealed(!isTranslationRevealed)}
                >
                  <p className="text-sm font-medium mb-1 text-muted-foreground">
                    English translation (click to reveal):
                  </p>
                  <p
                    className={`text-lg font-medium transition-all duration-100 ${isTranslationRevealed ? '' : 'blur-md'
                      }`}
                  >
                    {challenge.native_language}
                  </p>
                </div>
              </motion.div>
            )}
          </div>
        </AnimatedCard>
        <CardsRemaining
          dueCount={dueCount}
          totalCount={totalCount}
          className="mt-2"
        />

        {/* Accented character keyboard - show when not graded, language supports it, and not on small screens */}
        {gradingState === null && (targetLanguage === 'French' || targetLanguage === 'Spanish') && (
          <AccentedCharacterKeyboard
            onCharacterInsert={handleCharacterInsert}
            language={targetLanguage}
            className="hidden md:flex mt-3 p-3 border rounded-lg bg-muted/30"
          />
        )}

        {/* Mobile keyboard tip - show on small screens when conditions are met */}
        {gradingState === null && totalCount < 60 && (
          <MobileKeyboardTip
            language={targetLanguage}
            totalCount={totalCount}
          />
        )}
      </div >

      <div className="mt-4 flex flex-col gap-2">
        {onCantListen && gradingState === null && (
          <CantListenButton onClick={onCantListen} />
        )}

        {/* Submit/Continue button at the bottom */}
        <Button
          onClick={gradingState && 'graded' in gradingState
            ? () => onComplete(gradingState.graded.results)
            : handleSubmit}
          disabled={(gradingState === null && !allBlanksFilledOut) || ((gradingState !== null) && ('grading' in gradingState))}
          className="w-full h-14"
          size="lg"
        >
          {gradingState === null ? (
            <>
              Check Answer
              <span className="ml-2 text-sm text-muted-foreground">(⏎)</span>
            </>
          ) : 'grading' in gradingState ? (
            "AI is grading..."
          ) : 'error' in gradingState ? (
            "Error"
          ) : (
            <>
              {gradingState.graded.results.every(result => 'Provided' in result || result.AskedToTranscribe.parts.every(part => 'Perfect' in part.grade)) ? "Nailed it!" : "Continue"}
              <span className="ml-2 text-sm text-muted-foreground">(⏎)</span>
            </>
          )}
        </Button>
      </div>

      <ReportIssueModal
        context={`Transcription challenge: ${JSON.stringify(challenge)}`}
        open={showReportModal}
        onOpenChange={setShowReportModal}
        targetLanguage={targetLanguage}
      />
    </div >
  )
}


interface WordGradesProps {
  wordGrades: PartGraded[],
  setGrade: (results: PartGraded[]) => void,
  open_by_default: boolean
}

function WordGrades({
  wordGrades,
  setGrade,
  open_by_default,
}: WordGradesProps) {
  const [isOpen, setIsOpen] = useState(open_by_default)

  const gradeOptions = [
    { value: 'Perfect', label: 'Perfect' },
    { value: 'CorrectWithTypo', label: 'Correct with Typo' },
    { value: 'PhoneticallyIdenticalButContextuallyIncorrect', label: 'Phonetically Identical' },
    { value: 'PhoneticallySimilarButContextuallyIncorrect', label: 'Phonetically Similar' },
    { value: 'Incorrect', label: 'Incorrect' },
  ]

  const getGradeKey = (grade: WordGrade): string => {
    return Object.keys(grade)[0]
  }

  const handleGradeChange = (partIndex: number, wordIndex: number, newGradeKey: string) => {
    const updatedGrades = [...wordGrades]
    const part = updatedGrades[partIndex]

    if ('AskedToTranscribe' in part) {
      const newGrade: WordGrade = { [newGradeKey]: {} } as WordGrade
      part.AskedToTranscribe.parts[wordIndex].grade = newGrade
    }

    setGrade(updatedGrades)
  }

  const transcribedParts = wordGrades.filter(part => 'AskedToTranscribe' in part)

  if (transcribedParts.length === 0) {
    return null
  }

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between p-0">
          <span className="text-sm font-medium">
            Grade Words Manually
          </span>
          <span className="text-xs text-muted-foreground">
            {isOpen ? "Hide" : "Show"}
          </span>
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="mt-3 space-y-3">
          {wordGrades.map((part, partIndex) => {
            if ('AskedToTranscribe' in part) {
              return (
                <div key={partIndex} className="space-y-2">
                  <div className="text-sm text-muted-foreground">
                    Your answer: "{part.AskedToTranscribe.submission}"
                  </div>
                  <div className="grid gap-2">
                    {part.AskedToTranscribe.parts.map((wordPart, wordIndex) => (
                      <div key={wordIndex} className="flex items-center gap-3 p-2 rounded-lg bg-muted/30">
                        <div className="flex-1">
                          <span className="font-medium">{wordPart.heard.text}</span>
                        </div>
                        <Select
                          value={getGradeKey(wordPart.grade)}
                          onValueChange={(value: string) => handleGradeChange(partIndex, wordIndex, value)}
                        >
                          <SelectTrigger className="w-[200px]">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            {gradeOptions.map((option) => (
                              <SelectItem key={option.value} value={option.value}>
                                {option.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                    ))}
                  </div>
                </div>
              )
            }
            return null
          })}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
