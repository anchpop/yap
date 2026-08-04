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
import { LANGUAGES, detectBrowserLanguage } from "@/lib/languages";
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

  // Detect the browser's language, but only honour it if we teach in it.
  const detectedLanguage = useMemo(() => {
    const detectedLang = detectBrowserLanguage();
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
    (lang) => LANGUAGES[lang].status === "stable",
  );
  const alphaLanguages = targetLanguages.filter(
    (lang) => LANGUAGES[lang].status === "alpha",
  );
  const betaLanguages = targetLanguages.filter(
    (lang) => LANGUAGES[lang].status === "beta",
  );

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
          Sentry.captureException(e);
        });
    }
  }, [selectionState, weapon]);

  // Determine the Yaptown title to display based on selection state
  const yaptownTitle =
    selectionState.stage === "onboarding"
      ? LANGUAGES[selectionState.targetLanguage].yaptownName
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
                        style={{ background: LANGUAGES[lang].colors.gradient }}
                      />
                      <div className="relative z-10">
                        <div className="text-8xl mb-4">
                          {languageFlags[lang]}
                        </div>
                        <h2 className="text-2xl font-bold mb-1">
                          {LANGUAGES[lang].iSpeak}
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
                        LANGUAGES[currentTargetLanguage].colors.primary,
                    }}
                    onClick={onResume}
                    animate
                  >
                    <div
                      className="absolute inset-0 opacity-10 group-hover:opacity-20 transition-opacity duration-300"
                      style={{
                        background:
                          LANGUAGES[currentTargetLanguage].colors.gradient,
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
                          style={{ background: LANGUAGES[lang].colors.gradient }}
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
                              background: LANGUAGES[lang].colors.gradient,
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
                              background: LANGUAGES[lang].colors.gradient,
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
