import * as Sentry from "@sentry/react";
import { useState, useEffect, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ArrowRight, Check, ChevronsUpDown } from "lucide-react";
import {
  OnboardingFlow,
  type OnboardingSelections,
  type HeardAbout,
} from "@/components/OnboardingFlow";
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
  | { stage: "onboarding"; nativeLanguage: Language; targetLanguage: Language };

interface LanguageSelectorProps {
  onLanguagesConfirmed: (native: Language, target: Language) => void;
  onOnboardingComplete: (
    selections: OnboardingSelections,
    target: Language,
  ) => void;
  onHeardAbout: (value: HeardAbout) => void;
  hasHeardAbout: boolean;
  onboardedLanguages: Language[];
  currentTargetLanguage?: Language;
  showResumeButton?: boolean;
  onResume?: () => void;
  userInfo?: UserInfo;
  onBack?: () => void;
}

export function LanguageSelector({
  onLanguagesConfirmed,
  onOnboardingComplete,
  onHeardAbout,
  hasHeardAbout,
  onboardedLanguages,
  currentTargetLanguage,
  showResumeButton,
  onResume,
  userInfo,
  onBack,
}: LanguageSelectorProps) {
  const [selectionState, setSelectionState] = useState<LanguageSelectionState>({
    stage: "selectingNative",
  });
  const [comboboxOpen, setComboboxOpen] = useState(false);
  const weapon = useWeapon();

  const handleTargetLanguageSelected = (
    nativeLanguage: Language,
    lang: Language,
  ) => {
    if (onboardedLanguages.includes(lang)) {
      // Already onboarded for this language — skip straight to learning
      onLanguagesConfirmed(nativeLanguage, lang);
    } else {
      setSelectionState({
        stage: "onboarding",
        nativeLanguage,
        targetLanguage: lang,
      });
    }
  };

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
      ru: "Russian",
      "ru-RU": "Russian",
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

  // Auto-select detected language when entering native selection stage
  useEffect(() => {
    if (selectionState.stage !== "selectingNative") return;

    if (detectedLanguage) {
      setSelectionState({
        stage: "selectingTarget",
        nativeLanguage: detectedLanguage,
      });
    }
  }, [selectionState.stage, detectedLanguage]);

  // Determine language stability level
  const getLanguageStatus = (lang: Language): "stable" | "alpha" | "beta" => {
    if (lang === "Russian" || lang === "Korean") {
      return "alpha";
    }
    if (lang === "Italian" || lang === "Portuguese") {
      return "beta";
    }
    return "stable";
  };

  // Get target languages available for selected native language
  const targetLanguages =
    selectionState.stage === "selectingNative"
      ? []
      : availableCourses
          .filter(
            (course) => course.nativeLanguage === selectionState.nativeLanguage,
          )
          .map((course) => course.targetLanguage);

  // Group languages by stability status
  const stableLanguages = targetLanguages.filter(
    (lang) => getLanguageStatus(lang) === "stable",
  );
  const alphaLanguages = targetLanguages.filter(
    (lang) => getLanguageStatus(lang) === "alpha",
  );
  const betaLanguages = targetLanguages.filter(
    (lang) => getLanguageStatus(lang) === "beta",
  );

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
    Hindi: "मैं हिन्दी बोलता हूँ",
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
    Hindi: "यैप.टाउन",
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
      primary: "#009B3A",
      secondary: "#FFDF00",
      accent: "#002776",
      gradient:
        "linear-gradient(135deg, #009B3A 40%, #FFDF00 40% 60%, #002776 60%)",
    },
    Italian: {
      primary: "#009246",
      secondary: "#FFFFFF",
      accent: "#CE2B37",
      gradient:
        "linear-gradient(90deg, #009246 33%, #FFFFFF 33% 66%, #CE2B37 66%)",
    },
    Hindi: {
      primary: "#FF9933",
      secondary: "#FFFFFF",
      accent: "#138808",
      gradient:
        "linear-gradient(180deg, #FF9933 33%, #FFFFFF 33% 66%, #138808 66%)",
    },
  };

  useEffect(() => {
    if (selectionState.stage === "onboarding") {
      Sentry.addBreadcrumb({
        category: "language-pack",
        message: `Caching ${selectionState.nativeLanguage} → ${selectionState.targetLanguage}`,
        level: "info",
      });
      weapon
        .cache_language_pack({
          nativeLanguage: selectionState.nativeLanguage,
          targetLanguage: selectionState.targetLanguage,
        })
        .catch((e: unknown) => {
          const message = e instanceof Error ? e.message : String(e);
          // Network failures are expected on flaky mobile connections during
          // onboarding; only report unexpected errors to Sentry.
          if (!message.startsWith("Network error:")) {
            Sentry.captureException(e);
          }
        });
    }
  }, [selectionState, weapon]);

  // Determine the Yaptown title to display based on selection state
  const yaptownTitle =
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
      <div className="relative z-10 flex items-center justify-center">
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
              className="w-full max-w-4xl gap-4 flex flex-col items-center gap-8"
            >
              <div className="text-center">
                <h1
                  className="text-5xl font-bold mb-4 mt-16"
                  style={{ textWrap: "balance" }}
                >
                  <span className="highlight animate-fade-in">
                    What language will you speak next?
                  </span>
                </h1>
              </div>

              {/* Resume button if already learning a language */}
              {showResumeButton && currentTargetLanguage && onResume && (
                <div className="w-full max-w-md mb-4">
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

              {/* Stable languages (unlabeled) */}
              {stableLanguages.length > 0 && (
                <div className="grid md:grid-cols-3 grid-cols-2 gap-8 w-full">
                  {stableLanguages.map((lang) => (
                    <motion.div
                      key={lang}
                      whileHover={{ scale: 1.05 }}
                      whileTap={{ scale: 0.98 }}
                    >
                      <Card
                        className="relative overflow-hidden p-2 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2 aspect-square flex items-center justify-center"
                        onClick={() =>
                          handleTargetLanguageSelected(
                            selectionState.nativeLanguage,
                            lang,
                          )
                        }
                        animate
                      >
                        <div
                          className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                          style={{ background: languageColors[lang]?.gradient }}
                        />
                        <div className="relative z-10">
                          <div className="md:text-8xl text-6xl mb-4">
                            {languageFlags[lang]}
                          </div>
                          <h2 className="text-3xl font-bold mb-2">
                            {nativeLanguageNames[lang]}
                          </h2>
                        </div>
                      </Card>
                    </motion.div>
                  ))}
                </div>
              )}

              {/* Beta section divider */}
              {betaLanguages.length > 0 && (
                <>
                  <div className="w-full max-w-md">
                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="px-2 text-foreground">
                          Beta Languages
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className="grid md:grid-cols-3 grid-cols-2 gap-8 w-full">
                    {betaLanguages.map((lang) => (
                      <motion.div
                        key={lang}
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.98 }}
                      >
                        <Card
                          className="relative overflow-hidden p-2 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2 aspect-square flex items-center justify-center"
                          onClick={() =>
                            handleTargetLanguageSelected(
                              selectionState.nativeLanguage,
                              lang,
                            )
                          }
                          animate
                        >
                          <div
                            className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                            style={{
                              background: languageColors[lang]?.gradient,
                            }}
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
                </>
              )}

              {/* Alpha section divider */}
              {alphaLanguages.length > 0 && (
                <>
                  <div className="w-full max-w-md">
                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="px-2 text-foreground">
                          Alpha Languages
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className="grid md:grid-cols-3 grid-cols-2 gap-8 w-full">
                    {alphaLanguages.map((lang) => (
                      <motion.div
                        key={lang}
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.98 }}
                      >
                        <Card
                          className="relative overflow-hidden p-2 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2 aspect-square flex items-center justify-center"
                          onClick={() =>
                            handleTargetLanguageSelected(
                              selectionState.nativeLanguage,
                              lang,
                            )
                          }
                          animate
                        >
                          <div
                            className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                            style={{
                              background: languageColors[lang]?.gradient,
                            }}
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
                </>
              )}

              {/* Native language selector */}
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
                                    : "opacity-0",
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

              <div className="text-center mb-12">
                <p className="text-xl text-muted-foreground/70">
                  (Yap.Town is great for beginner and intermediate students.)
                </p>
              </div>
            </div>
          ) : selectionState.stage === "onboarding" ? (
            <OnboardingFlow
              targetLanguage={selectionState.targetLanguage}
              nativeLanguage={selectionState.nativeLanguage}
              hasHeardAbout={hasHeardAbout}
              onHeardAbout={onHeardAbout}
              onComplete={(selections) => {
                onOnboardingComplete(selections, selectionState.targetLanguage);
                onLanguagesConfirmed(
                  selectionState.nativeLanguage,
                  selectionState.targetLanguage,
                );
              }}
              onBack={() => {
                setSelectionState({
                  stage: "selectingTarget",
                  nativeLanguage: selectionState.nativeLanguage,
                });
              }}
            />
          ) : null}
        </AnimatePresence>
      </div>
    </TopPageLayout>
  );
}
