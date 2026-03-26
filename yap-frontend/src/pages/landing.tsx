import { useState, useEffect, useMemo } from 'react'
import { useNavigate, useOutletContext } from 'react-router-dom'
import { get_showcase_data, type CourseShowcase } from '../../../yap-frontend-rs/pkg'
import { motion, AnimatePresence } from 'framer-motion'
import { Button } from "@/components/ui/button.tsx"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs.tsx"
import { TopPageLayout } from '@/components/TopPageLayout'
import { ArrowRight } from 'lucide-react'
import type { AppContextType } from '@/App'
import { useDeckSelection } from '@/App'

const LANGUAGE_DISPLAY_NAMES: Record<string, string> = {
  French: "French", English: "English", Spanish: "Spanish", Korean: "Korean",
  German: "German", Italian: "Italian", Portuguese: "Portuguese",
  Chinese: "Chinese", Japanese: "Japanese", Russian: "Russian",
}

const BROWSER_LANG_MAP: Record<string, string> = {
  en: "English", "en-US": "English", "en-GB": "English",
  fr: "French", "fr-FR": "French",
  es: "Spanish", "es-ES": "Spanish",
  ko: "Korean", "ko-KR": "Korean",
  de: "German", "de-DE": "German",
  it: "Italian", "it-IT": "Italian",
  pt: "Portuguese", "pt-BR": "Portuguese", "pt-PT": "Portuguese",
  ru: "Russian", "ru-RU": "Russian",
}

function useDetectedNativeLanguage(): string {
  return useMemo(() => {
    const browserLang = navigator.language || navigator.languages?.[0]
    if (!browserLang) return "English"
    return BROWSER_LANG_MAP[browserLang] || BROWSER_LANG_MAP[browserLang.split("-")[0]] || "English"
  }, [])
}

function HighlightPhrase({ text, phrase }: { text: string, phrase: string }) {
  const idx = text.toLowerCase().indexOf(phrase.toLowerCase())
  if (idx === -1) return <>{text}</>
  return (
    <>
      {text.slice(0, idx)}
      <mark className="bg-accent-foreground/20 text-foreground rounded-sm px-0.5">{text.slice(idx, idx + phrase.length)}</mark>
      {text.slice(idx + phrase.length)}
    </>
  )
}

function SentenceShowcase({ showcaseData }: { showcaseData: CourseShowcase[] }) {
  const [selectedIdx, setSelectedIdx] = useState(0)
  const [phraseIdx, setPhraseIdx] = useState(0)
  const nativeLang = useDetectedNativeLanguage()

  const filteredData = useMemo(() => {
    const forNative = showcaseData.filter(c => c.nativeLanguage === nativeLang)
    return forNative.length > 0 ? forNative : showcaseData.filter(c => c.nativeLanguage === "English")
  }, [showcaseData, nativeLang])

  useEffect(() => {
    const id = setInterval(() => setPhraseIdx(i => i + 1), 5000)
    return () => clearInterval(id)
  }, [selectedIdx])

  const course = filteredData[selectedIdx]
  if (!course || course.phrases.length === 0) return null

  const phrase = course.phrases[phraseIdx % course.phrases.length]
  const totalSentences = filteredData.reduce((sum, c) => sum + c.sentenceCount, 0)

  return (
    <div id="how-it-works" className="min-h-dvh flex flex-col items-center justify-center w-full gap-12 py-24">
      <div className="flex flex-col items-center gap-4">
        <h2 className="text-4xl md:text-5xl font-black tracking-tight" style={{ textWrap: "balance" }}>
          {totalSentences.toLocaleString()}+ sentences.
        </h2>
        <p className="text-lg text-muted-foreground max-w-lg" style={{ textWrap: "balance" }}>
          Real sentences from movies and TV, in {filteredData.length} languages.
        </p>
      </div>

      <Tabs
        value={String(selectedIdx)}
        onValueChange={(v) => { setSelectedIdx(Number(v)); setPhraseIdx(0) }}
      >
        <TabsList className="flex-wrap h-auto gap-1">
          {filteredData.map((c, i) => (
            <TabsTrigger key={i} value={String(i)}>
              {LANGUAGE_DISPLAY_NAMES[c.targetLanguage] ?? c.targetLanguage}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <AnimatePresence mode="wait">
        <motion.div
          key={`${selectedIdx}-${phraseIdx % course.phrases.length}`}
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -10 }}
          transition={{ duration: 0.25 }}
          className="w-full max-w-lg flex flex-col items-center gap-4"
        >
          <div className="flex flex-col items-center gap-1">
            <p className="text-2xl font-bold">{phrase.displayText}</p>
            <p className="text-muted-foreground text-sm">{phrase.definition}</p>
          </div>

          <div className="flex flex-col gap-4 w-full">
            {phrase.examples.map((ex, i) => (
              <div key={i}>
                <p className="font-medium text-sm"><HighlightPhrase text={ex.target} phrase={phrase.displayText} /></p>
                <p className="text-sm text-muted-foreground mt-1">{ex.native}</p>
              </div>
            ))}
          </div>

          <p className="text-sm text-muted-foreground">
            {course.sentenceCount.toLocaleString()} sentences in {LANGUAGE_DISPLAY_NAMES[course.targetLanguage] ?? course.targetLanguage}
          </p>
        </motion.div>
      </AnimatePresence>

      <a
        href="/d/"
        className="text-sm text-muted-foreground hover:text-foreground transition-colors underline underline-offset-4"
      >
        Look up any word in our dictionaries &rarr;
      </a>
    </div>
  )
}

export function LandingPage() {
  const { userInfo } = useOutletContext<AppContextType>()
  const deckSelection = useDeckSelection()
  const navigate = useNavigate()

  useEffect(() => {
    if (deckSelection?.type === 'languageSelected') {
      navigate('/learn', { replace: true })
    }
  }, [deckSelection, navigate])

  const showcaseData = useMemo(() => {
    try { return get_showcase_data() } catch { return [] }
  }, [])

  if (deckSelection?.type === 'languageSelected') {
    return null
  }

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{ showSignupNag: false, title: "Yap.Town" }}
    >
      <div className="relative z-10 flex items-center justify-center">
        <div className="w-full max-w-2xl flex flex-col items-center text-center">
          <div className="flex flex-col items-center justify-center gap-8 h-[calc(100dvh-6rem)]">
            <h1
              className="text-5xl md:text-6xl font-black tracking-tight"
              style={{ textWrap: "balance" }}
            >
              Spaced repetition with{" "}
              <span className="text-accent-foreground italic squiggly-underline">actual sentences.</span>
            </h1>

            <p className="text-lg text-muted-foreground max-w-lg" style={{ textWrap: "balance" }}>
              SRS works. But isolated flashcards only get you so far.<br />
              <span className="text-foreground font-semibold">
                Every word in Yap lives inside a real sentence.
              </span>
            </p>

            <div className="flex items-center gap-4">
              <Button
                size="lg"
                variant="ghost"
                onClick={() => {
                  document.getElementById('how-it-works')?.scrollIntoView({ behavior: 'smooth' })
                }}
                className="text-lg"
              >
                How it works
              </Button>
              <Button
                size="lg"
                onClick={() => navigate('/select-language')}
                className="text-lg px-8"
              >
                Start learning
                <ArrowRight className="ml-2 h-5 w-5" />
              </Button>
            </div>

          </div>

          {showcaseData.length > 0 && <SentenceShowcase showcaseData={showcaseData} />}

          <div
            className="min-h-dvh flex flex-col items-center justify-center w-full gap-16 py-24"
          >
            <div className="flex flex-col items-center gap-6 max-w-lg">
              <h2 className="text-4xl md:text-5xl font-black tracking-tight" style={{ textWrap: "balance" }}>
                Language modelling like you've never seen before.
              </h2>
              <div className="flex flex-col gap-4">
                <p className="text-lg text-muted-foreground" style={{ textWrap: "balance" }}>
                  Yap understands <span className="text-foreground font-semibold">phrases and polysemy</span>, so every card targets a precise meaning in context.
                </p>
                <p className="text-lg text-muted-foreground" style={{ textWrap: "balance" }}>
                  And every deck is <span className="text-foreground font-semibold">premade and ready to go</span>.<br /> Just pick a language and start learning immediately.
                </p>
              </div>
              <div className="flex items-center gap-4">
                <span className="text-lg text-muted-foreground">Powered by FSRS</span>
                <Button
                  size="lg"
                  onClick={() => navigate('/select-language')}
                  className="text-lg px-8"
                >
                  Start learning
                  <ArrowRight className="ml-2 h-5 w-5" />
                </Button>
              </div>
              <a
                href="/blog/"
                className="text-sm text-muted-foreground hover:text-foreground transition-colors underline underline-offset-4"
              >
                Read about it on our blog &rarr;
              </a>
            </div>
          </div>
        </div>
      </div>
    </TopPageLayout>
  )
}
