import { useState, useEffect, useMemo } from "react";
import { useNavigate, useOutletContext } from "react-router-dom";
import {
  get_showcase_data,
  type CourseShowcase,
} from "../../../yap-frontend-rs/pkg";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button.tsx";
import { Card } from "@/components/ui/card.tsx";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs.tsx";
import { TopPageLayout } from "@/components/TopPageLayout";
import { cn } from "@/lib/utils";
import {
  ArrowRight,
  Volume2,
  MoreVertical,
  Moon,
  Heart,
} from "lucide-react";
import type { ReactNode } from "react";
import type { AppContextType } from "@/App";
import { useDeckSelection } from "@/App";

const LANGUAGE_DISPLAY_NAMES: Record<string, string> = {
  French: "French",
  English: "English",
  Spanish: "Spanish",
  Korean: "Korean",
  German: "German",
  Italian: "Italian",
  Portuguese: "Portuguese",
  ChineseSimplified: "Chinese (Simplified)",
  ChineseTraditional: "Chinese (Traditional)",
  Japanese: "Japanese",
  Russian: "Russian",
};

const BROWSER_LANG_MAP: Record<string, string> = {
  en: "English",
  "en-US": "English",
  "en-GB": "English",
  fr: "French",
  "fr-FR": "French",
  es: "Spanish",
  "es-ES": "Spanish",
  ko: "Korean",
  "ko-KR": "Korean",
  de: "German",
  "de-DE": "German",
  it: "Italian",
  "it-IT": "Italian",
  pt: "Portuguese",
  "pt-BR": "Portuguese",
  "pt-PT": "Portuguese",
  ru: "Russian",
  "ru-RU": "Russian",
};

function useDetectedNativeLanguage(): string {
  return useMemo(() => {
    const browserLang = navigator.language || navigator.languages?.[0];
    if (!browserLang) return "English";
    return (
      BROWSER_LANG_MAP[browserLang] ||
      BROWSER_LANG_MAP[browserLang.split("-")[0]] ||
      "English"
    );
  }, []);
}

function HighlightPhrase({ text, phrase }: { text: string; phrase: string }) {
  const idx = text.toLowerCase().indexOf(phrase.toLowerCase());
  if (idx === -1) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark className="bg-accent-foreground/20 text-foreground rounded-sm px-0.5">
        {text.slice(idx, idx + phrase.length)}
      </mark>
      {text.slice(idx + phrase.length)}
    </>
  );
}

function SentenceShowcase({
  showcaseData,
}: {
  showcaseData: CourseShowcase[];
}) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [phraseIdx, setPhraseIdx] = useState(0);
  const nativeLang = useDetectedNativeLanguage();

  const filteredData = useMemo(() => {
    const forNative = showcaseData.filter(
      (c) => c.nativeLanguage === nativeLang,
    );
    return forNative.length > 0
      ? forNative
      : showcaseData.filter((c) => c.nativeLanguage === "English");
  }, [showcaseData, nativeLang]);

  useEffect(() => {
    const id = setInterval(() => setPhraseIdx((i) => i + 1), 5000);
    return () => clearInterval(id);
  }, [selectedIdx]);

  const course = filteredData[selectedIdx];
  if (!course || course.phrases.length === 0) return null;

  const phrase = course.phrases[phraseIdx % course.phrases.length];
  const totalSentences = filteredData.reduce(
    (sum, c) => sum + c.sentenceCount,
    0,
  );

  return (
    <div
      id="how-it-works"
      className="min-h-dvh flex flex-col items-center justify-center w-full gap-12 py-24"
    >
      <div className="flex flex-col items-center gap-4">
        <h2
          className="text-4xl md:text-5xl font-black tracking-tight"
          style={{ textWrap: "balance" }}
        >
          {totalSentences.toLocaleString()}+ sentences.
        </h2>
        <p
          className="text-lg text-muted-foreground max-w-lg"
          style={{ textWrap: "balance" }}
        >
          Real sentences from movies and TV, in {filteredData.length} languages.
        </p>
      </div>

      <Tabs
        value={String(selectedIdx)}
        onValueChange={(v) => {
          setSelectedIdx(Number(v));
          setPhraseIdx(0);
        }}
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
                <p className="font-medium text-sm">
                  <HighlightPhrase
                    text={ex.target}
                    phrase={phrase.displayText}
                  />
                </p>
                <p className="text-sm text-muted-foreground mt-1">
                  {ex.native}
                </p>
              </div>
            ))}
          </div>

          <p className="text-sm text-muted-foreground">
            {course.sentenceCount.toLocaleString()} sentences in{" "}
            {LANGUAGE_DISPLAY_NAMES[course.targetLanguage] ??
              course.targetLanguage}
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
  );
}

function StartLearningButton() {
  const navigate = useNavigate();
  return (
    <span className="rainbow-ripple-wrap">
      <Button
        size="lg"
        onClick={() => navigate("/select-language")}
        className="text-lg px-8"
        variant="attention"
      >
        Start learning
        <ArrowRight className="ml-2 h-5 w-5" />
      </Button>
    </span>
  );
}

function FeatureCard({
  icon,
  title,
  description,
  className,
  titleClassName,
}: {
  icon?: ReactNode;
  title: string;
  description: string;
  className?: string;
  titleClassName?: string;
}) {
  return (
    <Card
      variant="light"
      className={cn("flex-row items-start gap-3 p-4 text-left", className)}
    >
      {icon && (
        <div className="shrink-0 rounded-lg bg-accent-foreground/10 p-2 text-accent-foreground">
          {icon}
        </div>
      )}
      <div>
        <h3 className={cn("font-semibold text-base", titleClassName)}>
          {title}
        </h3>
        <p className="text-sm mt-0.5 leading-snug">{description}</p>
      </div>
    </Card>
  );
}

function MockFlashcard() {
  const wordClass = "border-b-2 border-dotted border-pink-400 pb-0.5";
  return (
    <Card className="w-fit max-w-xl text-left gap-5 p-5 rotate-[5deg]">
      <div className="flex items-center justify-between px-2">
        <div className="flex items-center gap-2">
          <span className="text-2xl leading-none" aria-hidden>
            🇫🇷
          </span>
          <span className="font-bold text-lg">Yap</span>
        </div>
        <div className="flex items-center gap-3 text-base text-muted-foreground">
          <span className="rounded-md border border-border/60 px-2 py-0.5 text-sm font-medium">
            25
          </span>
          <span>André</span>
          <Moon className="h-5 w-5" />
        </div>
      </div>

      <Card className="gap-4 p-6">
        <div
          className="flex items-center gap-3"
          style={{ anchorName: "--sentence" } as React.CSSProperties}
        >
          <Volume2 className="h-6 w-6 text-muted-foreground shrink-0" />
          <p
            lang="fr"
            className="text-2xl md:text-3xl font-semibold tracking-tight flex-1 flex flex-wrap gap-x-2 gap-y-1"
          >
            <span className={wordClass}>Il</span>
            <span className={wordClass}>m'en</span>
            <span className={wordClass}>faut</span>
            <span
              className="inline-flex gap-x-2 text-yellow-400"
              style={{ anchorName: "--phrase" } as React.CSSProperties}
            >
              <span className={wordClass}>un</span>
              <span className={wordClass}>autre.</span>
            </span>
          </p>
          <MoreVertical className="h-5 w-5 text-muted-foreground shrink-0" />
        </div>
        <p className="text-muted-foreground text-lg">Translation...</p>
      </Card>
      <p
        className="text-sm font-medium text-sky-600 dark:text-sky-100 px-2 text-center"
        style={{ anchorName: "--review" } as React.CSSProperties}
      >
        Reviewing 6 words
      </p>
    </Card>
  );
}

export function LandingPage() {
  const { userInfo } = useOutletContext<AppContextType>();
  const deckSelection = useDeckSelection();
  const navigate = useNavigate();

  useEffect(() => {
    if (deckSelection?.type === "languageSelected") {
      navigate("/learn", { replace: true });
    }
  }, [deckSelection, navigate]);

  const showcaseData = useMemo(() => {
    try {
      return get_showcase_data();
    } catch {
      return [];
    }
  }, []);

  if (deckSelection?.type === "languageSelected") {
    return null;
  }

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{ showSignupNag: false, title: "Yap.Town" }}
    >
      <div className="relative z-10 flex flex-col items-center">
        <div className="w-screen flex flex-col items-center text-center min-h-[calc(100dvh-6rem)] py-8 px-4">
          <div className="flex-1 flex flex-col items-center justify-center gap-8 w-full">
          <div className="flex flex-col items-center gap-6 w-full max-w-3xl">
            <h1
              className="text-5xl md:text-6xl font-black tracking-tight"
              style={{ textWrap: "balance" }}
            >
              Spaced repetition with{" "}
              <span className="text-accent-foreground italic squiggly-underline">
                actual sentences.
              </span>
            </h1>

            <p
              className="text-lg text-muted-foreground max-w-lg"
              style={{ textWrap: "balance" }}
            >
              SRS works. But isolated flashcards only get you so far.
              <br />
              <span className="text-foreground font-semibold">
                Every word in Yap lives inside a real sentence.
              </span>
            </p>

            <div className="flex items-center gap-4">
              <Button
                size="lg"
                variant="ghost"
                onClick={() => {
                  document
                    .getElementById("how-it-works")
                    ?.scrollIntoView({ behavior: "smooth" });
                }}
                className="text-lg"
              >
                How it works
              </Button>
              <StartLearningButton />
            </div>
          </div>

          <div className="flex flex-col items-center gap-4 relative w-full lg:gap-0 lg:justify-center lg:pb-40">
            <MockFlashcard />

            <div
              className="w-full max-w-sm lg:absolute lg:w-64 lg:max-w-none"
              style={
                {
                  positionAnchor: "--sentence",
                  positionArea: "left",
                } as React.CSSProperties
              }
            >
              <FeatureCard
                title="Real sentences"
                description="Learn words in context, from real movies and TV."
                className="-rotate-[3deg]"
                titleClassName="text-pink-400"
              />
            </div>

            <div
              className="w-full max-w-sm lg:absolute lg:w-64 lg:max-w-none"
              style={
                {
                  positionAnchor: "--phrase",
                  positionArea: "right",
                } as React.CSSProperties
              }
            >
              <FeatureCard
                title="Phrase-aware"
                description="Cards target precise meanings, not just words."
                className="-rotate-[3deg]"
                titleClassName="text-yellow-400"
              />
            </div>

            <div
              className="w-full max-w-sm lg:absolute lg:w-64 lg:max-w-none"
              style={
                {
                  positionAnchor: "--review",
                  positionArea: "bottom",
                } as React.CSSProperties
              }
            >
              <FeatureCard
                title="Smarter review"
                description="SRS ensures you never forget what you learned."
                className="-rotate-[3deg]"
                titleClassName="text-sky-600 dark:text-sky-100"
              />
            </div>
          </div>
          </div>

          <p className="text-base flex items-center gap-2 mt-8">
            <Heart className="h-4 w-4 fill-accent-foreground text-accent-foreground" />
            Made for serious language learners who want more than flashcards.
          </p>
        </div>

        <div className="w-full max-w-2xl flex flex-col items-center text-center">
          {showcaseData.length > 0 && (
            <SentenceShowcase showcaseData={showcaseData} />
          )}

          <div className="min-h-dvh flex flex-col items-center justify-center w-full gap-16 py-24">
            <div className="flex flex-col items-center gap-6 max-w-lg">
              <h2
                className="text-4xl md:text-5xl font-black tracking-tight"
                style={{ textWrap: "balance" }}
              >
                Language modelling like you've never seen before.
              </h2>
              <div className="flex flex-col gap-4">
                <p
                  className="text-lg text-muted-foreground"
                  style={{ textWrap: "balance" }}
                >
                  Yap understands{" "}
                  <span className="text-foreground font-semibold">
                    phrases and polysemy
                  </span>
                  , so every card targets a precise meaning in context.
                </p>
                <p
                  className="text-lg text-muted-foreground"
                  style={{ textWrap: "balance" }}
                >
                  And every deck is{" "}
                  <span className="text-foreground font-semibold">
                    premade and ready to go
                  </span>
                  .<br /> Just pick a language and start learning immediately.
                </p>
              </div>
              <div className="flex items-center gap-4">
                <span className="text-lg text-muted-foreground">
                  Powered by FSRS
                </span>
                <StartLearningButton />
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
  );
}
