import { useState, useEffect, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ArrowRight, Check, ChevronsUpDown } from "lucide-react";
import { useBackground } from "@/components/BackgroundShader";
import {
  Carousel,
  CarouselContent,
  CarouselItem,
  type CarouselApi,
} from "@/components/ui/carousel";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn, languageFlags, nativeLanguageNames } from "@/lib/utils";
import type { Language } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { useWeapon } from "@/weapon";
import { get_available_courses } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { TopPageLayout } from "@/components/TopPageLayout";
import type { UserInfo } from "@/App";

type LanguageSelectionState =
  | { stage: "selectingNative" }
  | { stage: "selectingTarget"; nativeLanguage: Language }
  | {
      stage: "askingExperience";
      nativeLanguage: Language;
      targetLanguage: Language;
    }
  | { stage: "onboarding"; nativeLanguage: Language; targetLanguage: Language };

interface LanguageSelectorProps {
  onLanguagesConfirmed: (native: Language, target: Language) => void;
  onExperience: (fresh: boolean, target: Language) => void;
  skipOnboarding: boolean;
  currentTargetLanguage?: Language;
  showResumeButton?: boolean;
  onResume?: () => void;
  userInfo?: UserInfo;
  onBack?: () => void;
}

export function LanguageSelector({
  onLanguagesConfirmed,
  onExperience,
  skipOnboarding,
  currentTargetLanguage,
  showResumeButton,
  onResume,
  userInfo,
  onBack,
}: LanguageSelectorProps) {
  const [selectionState, setSelectionState] = useState<LanguageSelectionState>({
    stage: "selectingNative",
  });
  const [api, setApi] = useState<CarouselApi>();
  const [current, setCurrent] = useState(0);
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const [userKnowsLanguage, setUserKnowsLanguage] = useState<
    "knows_some" | "beginner" | null
  >(null);
  const weapon = useWeapon();
  const { bumpBackground } = useBackground();

  // Get available courses
  const availableCourses = useMemo(() => get_available_courses(), []);

  // Get unique native languages - memoized for stability
  const nativeLanguages = useMemo(() => {
    const uniqueNative = new Set<Language>();
    availableCourses.forEach((course) => {
      uniqueNative.add(course.nativeLanguage);
    });
    return Array.from(uniqueNative);
  }, [availableCourses]);

  // Map browser language codes to our Language types and detect browser language
  // NOTE: This language map must be updated whenever a new language is added to the Language enum
  const detectedLanguage = useMemo(() => {
    const languageMap: Record<string, Language> = {
      en: "English",
      "en-US": "English",
      "en-GB": "English",
      fr: "French",
      "fr-FR": "French",
      es: "Spanish",
      "es-ES": "Spanish",
      ko: "Korean",
      "ko-KR": "Korean",
    };

    // Get browser language
    const browserLang = navigator.language || navigator.languages?.[0];

    // Check if browser language is supported
    const detectedLang = browserLang
      ? languageMap[browserLang] || languageMap[browserLang.split("-")[0]]
      : null;

    return detectedLang && nativeLanguages.includes(detectedLang)
      ? detectedLang
      : null;
  }, [nativeLanguages]);

  // Auto-select detected language on mount
  useEffect(() => {
    // Only run on initial mount when in native selection stage
    if (selectionState.stage !== "selectingNative") return;

    if (detectedLanguage) {
      setSelectionState({
        stage: "selectingTarget",
        nativeLanguage: detectedLanguage,
      });
    }
  }, []); // Only run on mount

  // Get target languages available for selected native language
  const targetLanguages =
    selectionState.stage === "selectingNative"
      ? []
      : availableCourses
          .filter(
            (course) => course.nativeLanguage === selectionState.nativeLanguage
          )
          .map((course) => course.targetLanguage);

  useEffect(() => {
    if (!api) {
      return;
    }

    setCurrent(api.selectedScrollSnap());

    api.on("select", () => {
      setCurrent(api.selectedScrollSnap());
    });
  }, [api]);

  // Type-safe language configuration - TypeScript will error if a language is missing
  const languageConfirmTexts: Record<Language, string> = {
    French: "Allons-y",
    Spanish: "Vamos",
    Korean: "가자",
    English: "Let's go",
    German: "Los geht's",
    Chinese: "走吧",
    Japanese: "行こう",
    Russian: "Пойдем",
    Portuguese: "Vamos lá",
    Italian: "Andiamo",
  };

  // "I speak [language]" in each language
  const iSpeakPhrases: Record<Language, string> = {
    English: "I speak English",
    French: "Je parle français",
    Spanish: "Hablo español",
    Korean: "한국어를 합니다",
    German: "Ich spreche Deutsch",
    Chinese: "我说中文",
    Japanese: "日本語を話します",
    Russian: "Я говорю по-русски",
    Portuguese: "Eu falo português",
    Italian: "Parlo italiano",
  };

  // "Yap.Town" in each language
  const yaptownNames: Record<Language, string> = {
    English: "Yap.Town",
    French: "Yap.Ville",
    Spanish: "Yap.Ciudad",
    Korean: "얍.타운",
    German: "Yap.Stadt",
    Chinese: "Yap.城",
    Japanese: "Yap.町",
    Russian: "Yap.Город",
    Portuguese: "Yap.Cidade",
    Italian: "Yap.Città",
  };

  const languageColors: Record<
    Language,
    { primary: string; secondary: string; accent: string; gradient: string }
  > = {
    French: {
      primary: "#002395",
      secondary: "#FFFFFF",
      accent: "#ED2939",
      gradient:
        "linear-gradient(90deg, #002395 33%, #FFFFFF 33% 66%, #ED2939 66%)",
    },
    Spanish: {
      primary: "#C60B1E",
      secondary: "#FFC400",
      accent: "#C60B1E",
      gradient:
        "linear-gradient(180deg, #C60B1E 25%, #FFC400 25% 75%, #C60B1E 75%)",
    },
    Korean: {
      primary: "#003478",
      secondary: "#FFFFFF",
      accent: "#C60B1E",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #C60B1E 50%)",
    },
    English: {
      primary: "#012169",
      secondary: "#FFFFFF",
      accent: "#C8102E",
      gradient:
        "linear-gradient(90deg, #012169 33%, #FFFFFF 33% 66%, #C8102E 66%)",
    },
    German: {
      primary: "#000000",
      secondary: "#DD0000",
      accent: "#FFCE00",
      gradient:
        "linear-gradient(180deg, #000000 33%, #DD0000 33% 66%, #FFCE00 66%)",
    },
    Chinese: {
      primary: "#DE2910",
      secondary: "#FFDE00",
      accent: "#DE2910",
      gradient: "linear-gradient(135deg, #DE2910 50%, #FFDE00 50%)",
    },
    Japanese: {
      primary: "#FFFFFF",
      secondary: "#BC002D",
      accent: "#BC002D",
      gradient: "linear-gradient(180deg, #FFFFFF 50%, #BC002D 50%)",
    },
    Russian: {
      primary: "#FFFFFF",
      secondary: "#0039A6",
      accent: "#D52B1E",
      gradient:
        "linear-gradient(180deg, #FFFFFF 33%, #0039A6 33% 66%, #D52B1E 66%)",
    },
    Portuguese: {
      primary: "#006600",
      secondary: "#FFCC00",
      accent: "#009900",
      gradient:
        "linear-gradient(135deg, #006600 33%, #FFCC00 33% 66%, #009900 66%)",
    },
    Italian: {
      primary: "#009246",
      secondary: "#FFFFFF",
      accent: "#CE2B37",
      gradient:
        "linear-gradient(90deg, #009246 33%, #FFFFFF 33% 66%, #CE2B37 66%)",
    },
  };

  // Determine if languages are beta
  const isBeta = (lang: Language) => {
    // French and German are stable, others are beta for now
    return lang !== "French" && lang !== "German";
  };

  useEffect(() => {
    if (selectionState.stage === "onboarding") {
      weapon.cache_language_pack({
        nativeLanguage: selectionState.nativeLanguage,
        targetLanguage: selectionState.targetLanguage,
      });
    }
  }, [selectionState, weapon]);

  const acknowledgeExperience =
    userKnowsLanguage === "knows_some"
      ? {
          title: "Great! Let's find your level.",
          content:
            "You'll start with a quick placement test to help us understand what you already know. It'll take less than 2 minutes!",
        }
      : {
          title: "Perfect! Welcome aboard.",
          content:
            "We'll start you at the very beginning and build your foundation step by step.",
        };

  const introScreens = skipOnboarding
    ? [acknowledgeExperience]
    : [
        acknowledgeExperience,
        {
          title: "Yap values your time.",
          content:
            "Every design decision in Yap is based on helping you learn the most in the time you spend.",
        },
        userKnowsLanguage === "knows_some"
          ? {
              title: "Yap adapts to your skill level.",
              content:
                "So you don't waste time reviewing what you already learned on Duolingo.",
            }
          : {
              title: "Yap teaches you the most common words first.",
              content:
                "It'll surprise you how much you can say with just a few words.",
            },
        {
          title: "Yap has no lesson plan.",
          content:
            "Every lesson adapts to what YOU struggle with, not what lesson 47 says you should know.",
        },
        {
          title: "Yap reminds you of words just before you forget them.",
          content:
            "We use spaced repetition to review each word at the perfect time.",
        },
      ];

  // Determine the Yaptown title to display based on selection state
  const yaptownTitle =
    selectionState.stage === "askingExperience" ||
    selectionState.stage === "onboarding"
      ? yaptownNames[selectionState.targetLanguage]
      : "Yap.Town";

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{
        showSignupNag: false,
        title: yaptownTitle,
        backButton: onBack ? { label: yaptownTitle, onBack } : undefined,
      }}
    >
      {/* Main content */}
      <div className="relative z-10 flex items-center justify-center mt-8">
        <AnimatePresence mode="wait">
          {selectionState.stage === "selectingNative" ? (
            // Step 1: Select native language
            <motion.div
              key="native-selection"
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -20 }}
              className="w-full max-w-4xl gap-4 flex flex-col items-center"
            >
              <div className="text-center">
                <h1
                  className="text-5xl font-bold mb-4"
                  style={{ textWrap: "balance" }}
                >
                  What's your native language?
                </h1>
                <p className="text-xl text-muted-foreground mb-8">
                  So we can talk to you!
                </p>
              </div>

              <div className="grid md:grid-cols-2 gap-8 w-full max-w-2xl">
                {nativeLanguages.map((lang) => (
                  <motion.div
                    key={lang}
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.98 }}
                  >
                    <Card
                      className="relative overflow-hidden p-2 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2 aspect-square flex items-center justify-center"
                      onClick={() => {
                        setSelectionState({
                          stage: "selectingTarget",
                          nativeLanguage: lang,
                        });
                      }}
                    >
                      <div
                        className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                        style={{ background: languageColors[lang]?.gradient }}
                      />
                      <div className="relative z-10">
                        <div className="text-8xl mb-4">
                          {languageFlags[lang]}
                        </div>
                        <h2 className="text-2xl font-bold mb-1">
                          {iSpeakPhrases[lang]}
                        </h2>
                        <p className="text-lg text-muted-foreground">
                          {nativeLanguageNames[lang]}
                        </p>
                      </div>
                    </Card>
                  </motion.div>
                ))}
              </div>
            </motion.div>
          ) : selectionState.stage === "selectingTarget" ? (
            // Step 2: Select target language
            <div
              key="target-selection"
              className="w-full max-w-4xl gap-4 flex flex-col items-center"
            >
              <div className="text-center">
                <h1
                  className="text-5xl font-bold mb-4"
                  style={{ textWrap: "balance" }}
                >
                  <span className="highlight animate-fade-in">
                    What language will you speak next?
                  </span>
                </h1>
                <div className="flex items-center justify-center gap-2 mb-6">
                  <span className="text-lg text-muted-foreground animate-fade-in">
                    Native language:
                  </span>
                  <Popover open={comboboxOpen} onOpenChange={setComboboxOpen}>
                    <PopoverTrigger asChild>
                      <Button
                        variant="outline"
                        role="combobox"
                        aria-expanded={comboboxOpen}
                        className="w-[180px] justify-between animate-fade-in"
                        animate
                      >
                        <>
                          <span className="mr-2">
                            {languageFlags[selectionState.nativeLanguage]}
                          </span>
                          {selectionState.nativeLanguage}
                        </>
                        <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent className="w-[180px] p-0">
                      <Command>
                        <CommandInput placeholder="Search language..." />
                        <CommandList>
                          <CommandEmpty>No language found.</CommandEmpty>
                          <CommandGroup>
                            {nativeLanguages.map((lang) => (
                              <CommandItem
                                key={lang}
                                value={lang}
                                onSelect={() => {
                                  setSelectionState({
                                    stage: "selectingTarget",
                                    nativeLanguage: lang,
                                  });
                                  setComboboxOpen(false);
                                }}
                              >
                                <Check
                                  className={cn(
                                    "mr-2 h-4 w-4",
                                    selectionState.nativeLanguage === lang
                                      ? "opacity-100"
                                      : "opacity-0"
                                  )}
                                />
                                <span className="mr-2">
                                  {languageFlags[lang]}
                                </span>
                                {lang}
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>
                </div>
              </div>

              {/* Resume button if already learning a language */}
              {showResumeButton && currentTargetLanguage && onResume && (
                <div
                  className="w-full max-w-md mb-4"
                >
                  <Card
                    className="relative overflow-hidden p-6 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-4"
                    style={{
                      borderColor:
                        languageColors[currentTargetLanguage]?.primary,
                    }}
                    onClick={onResume}
                    animate
                  >
                    <div
                      className="absolute inset-0 opacity-10 group-hover:opacity-20 transition-opacity duration-300"
                      style={{
                        background:
                          languageColors[currentTargetLanguage]?.gradient,
                      }}
                    />
                    <div className="relative z-10 flex items-center justify-center gap-4">
                      <div className="text-5xl">
                        {languageFlags[currentTargetLanguage]}
                      </div>
                      <div className="text-left">
                        <h3 className="text-2xl font-bold mb-1">
                          Resume {nativeLanguageNames[currentTargetLanguage]}
                        </h3>
                        <p className="text-sm text-muted-foreground">
                          Continue where you left off
                        </p>
                      </div>
                      <ArrowRight className="h-6 w-6 ml-auto" />
                    </div>
                  </Card>
                </div>
              )}

              {showResumeButton && currentTargetLanguage && (
                <div className="w-full max-w-md mb-2">
                  <div className="relative">
                    <div className="absolute inset-0 flex items-center">
                      <span className="w-full border-t" />
                    </div>
                    <div className="relative flex justify-center text-xs uppercase">
                      <span className="px-2 text-foreground">
                        Or choose a different language
                      </span>
                    </div>
                  </div>
                </div>
              )}

              <div className="grid md:grid-cols-3 grid-cols-2 gap-8 w-full">
                {targetLanguages.map((lang) => (
                  <motion.div
                    key={lang}
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.98 }}
                  >
                    <Card
                      className="relative overflow-hidden p-2 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2 aspect-square flex items-center justify-center"
                      onClick={() => {
                        bumpBackground(50.0);
                        setSelectionState({
                          stage: "askingExperience",
                          nativeLanguage: selectionState.nativeLanguage,
                          targetLanguage: lang,
                        });
                      }}
                      animate
                    >
                      {isBeta(lang) && (
                        <Badge className="absolute bottom-1 right-1 z-20 gap-1">
                          Beta
                        </Badge>
                      )}
                      <div
                        className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                        style={{ background: languageColors[lang]?.gradient }}
                      />
                      <div className="relative z-10">
                        <div className="md:text-8xl text-6xl mb-4">
                          {languageFlags[lang]}
                        </div>
                        <h2 className="md:text-3xl text-2xl font-bold mb-2">
                          {nativeLanguageNames[lang]}
                        </h2>
                      </div>
                    </Card>
                  </motion.div>
                ))}
              </div>

              <div className="text-center mb-12">
                <p className="text-xl text-muted-foreground/70">
                  (Yap.Town is great for beginner and intermediate students.)
                </p>
              </div>
            </div>
          ) : selectionState.stage === "askingExperience" ? (
            // Step 3: Ask about experience level
            <motion.div
              key="experience-question"
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -20 }}
              className="w-full max-w-2xl gap-4 flex flex-col items-center"
            >
              <div className="text-center mb-8">
                <h1
                  className="text-5xl font-bold mb-4"
                  style={{ textWrap: "balance" }}
                >
                  Do you already know some{" "}
                  {nativeLanguageNames[selectionState.targetLanguage]}?
                </h1>
              </div>

              <div className="flex flex-col gap-4 w-full max-w-md">
                <Button
                  size="lg"
                  variant="outline"
                  onClick={() => {
                    setUserKnowsLanguage("knows_some");
                    onExperience(false, selectionState.targetLanguage);
                    setSelectionState({
                      stage: "onboarding",
                      nativeLanguage: selectionState.nativeLanguage,
                      targetLanguage: selectionState.targetLanguage,
                    });
                  }}
                  className="text-lg py-8 hover:scale-105 transition-transform"
                >
                  Yes, I know some
                </Button>
                <Button
                  size="lg"
                  variant="outline"
                  onClick={() => {
                    setUserKnowsLanguage("beginner");
                    onExperience(true, selectionState.targetLanguage);
                    setSelectionState({
                      stage: "onboarding",
                      nativeLanguage: selectionState.nativeLanguage,
                      targetLanguage: selectionState.targetLanguage,
                    });
                  }}
                  className="text-lg py-8 hover:scale-105 transition-transform"
                >
                  No, I'm starting fresh
                </Button>
              </div>

              <Button
                variant="ghost"
                className="mt-6"
                onClick={() => {
                  setSelectionState({
                    stage: "selectingTarget",
                    nativeLanguage: selectionState.nativeLanguage,
                  });
                }}
              >
                Back
              </Button>
            </motion.div>
          ) : selectionState.stage === "onboarding" ? (
            // Step 4: Onboarding screens (if not skipping)
            <div className="flex flex-col items-center justify-center w-full max-w-4xl">
              <Carousel
                setApi={setApi}
                className="w-full"
                opts={{
                  align: "start",
                }}
              >
                <CarouselContent>
                  {introScreens.map((screen, index) => (
                    <CarouselItem key={index}>
                      <Card className="p-4 pt-12 pb-12" animate>
                        <div className="text-center">
                          <h2
                            className="text-3xl font-bold mb-6"
                            style={{ textWrap: "balance" }}
                          >
                            {screen.title}
                          </h2>
                          <p
                            className="text-lg mb-8"
                            style={{ textWrap: "balance" }}
                          >
                            {screen.content}
                          </p>
                        </div>
                      </Card>
                    </CarouselItem>
                  ))}
                  <CarouselItem>
                    <Card className="p-12" animate>
                      <div className="text-center">
                        <motion.div
                          initial={{ scale: 0 }}
                          animate={{ scale: 1 }}
                          transition={{
                            type: "spring",
                            stiffness: 200,
                            damping: 15,
                          }}
                          className="text-9xl mb-6"
                        >
                          {languageFlags[selectionState.targetLanguage]}
                        </motion.div>
                        <h2 className="text-3xl font-bold mb-6">
                          {selectionState.targetLanguage ===
                            currentTargetLanguage ||
                          userKnowsLanguage == "knows_some"
                            ? `Ready to continue learning ${
                                nativeLanguageNames[
                                  selectionState.targetLanguage
                                ]
                              }?`
                            : `Ready to start learning ${
                                nativeLanguageNames[
                                  selectionState.targetLanguage
                                ]
                              }?`}
                        </h2>
                      </div>
                    </Card>
                  </CarouselItem>
                </CarouselContent>
              </Carousel>
              <div className="flex gap-4 justify-center mt-6">
                <Button
                  variant="outline"
                  size="lg"
                  onClick={() => {
                    if (current === 0) {
                      setSelectionState({
                        stage: "selectingTarget",
                        nativeLanguage: selectionState.nativeLanguage,
                      });
                    } else {
                      api?.scrollPrev();
                    }
                  }}
                  className="min-w-[120px]"
                >
                  Back
                </Button>
                <Button
                  size="lg"
                  onClick={() => {
                    if (current === introScreens.length) {
                      onLanguagesConfirmed(
                        selectionState.nativeLanguage,
                        selectionState.targetLanguage
                      );
                    } else {
                      api?.scrollNext();
                    }
                  }}
                  className={`min-w-[120px] flex items-center gap-2 ${
                    current === introScreens.length
                      ? "text-white hover:opacity-90 active:scale-95 transition-all"
                      : ""
                  }`}
                  style={{
                    ...(current === introScreens.length
                      ? {
                          background: `linear-gradient(135deg, ${
                            languageColors[selectionState.targetLanguage]
                              ?.primary
                          }, ${
                            languageColors[selectionState.targetLanguage]
                              ?.accent
                          })`,
                        }
                      : {}),
                  }}
                >
                  {current === introScreens.length
                    ? languageConfirmTexts[selectionState.targetLanguage]
                    : "Next"}
                  <ArrowRight className="h-4 w-4" />
                </Button>
              </div>
            </div>
          ) : null}
        </AnimatePresence>
      </div>
    </TopPageLayout>
  );
}
