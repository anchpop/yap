import { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { ArrowRight } from 'lucide-react';
import {
  Carousel,
  CarouselContent,
  CarouselItem,
  type CarouselApi,
} from "@/components/ui/carousel";
import type { Language } from '../../../yap-frontend-rs/pkg/yap_frontend_rs';
import { useWeapon } from '@/weapon';

interface LanguageSelectorProps {
  onLanguageConfirmed: (language: Language) => void;
  skipOnboarding: boolean;
}

export function LanguageSelector({ onLanguageConfirmed, skipOnboarding }: LanguageSelectorProps) {
  const [selectedLanguage, setSelectedLanguage] = useState<Language | null>(null);
  const [api, setApi] = useState<CarouselApi>();
  const [current, setCurrent] = useState(0);
  const weapon = useWeapon();

  useEffect(() => {
    if (!api) {
      return;
    }

    setCurrent(api.selectedScrollSnap());

    api.on("select", () => {
      setCurrent(api.selectedScrollSnap());
    });
  }, [api]);

  const languages = [
    {
      name: 'French' as Language,
      flag: '🇫🇷',
      confirmText: 'Allons-y',
      colors: {
        primary: '#002395',
        secondary: '#FFFFFF',
        accent: '#ED2939',
        gradient: 'linear-gradient(90deg, #002395 33%, #FFFFFF 33% 66%, #ED2939 66%)'
      }
    },
    {
      name: 'Spanish' as Language,
      flag: '🇪🇸',
      confirmText: 'Vamos',
      colors: {
        primary: '#C60B1E',
        secondary: '#FFC400',
        accent: '#C60B1E',
        gradient: 'linear-gradient(180deg, #C60B1E 25%, #FFC400 25% 75%, #C60B1E 75%)'
      }
    },
    {
      name: 'Korean' as Language,
      flag: '🇰🇷',
      confirmText: '가자',
      colors: {
        primary: '#003478',
        secondary: '#FFFFFF',
        accent: '#C60B1E',
        gradient: 'linear-gradient(180deg, #FFFFFF 50%, #C60B1E 50%)'
      }
    }
  ];

  const selectedLang = languages.find(l => l.name === selectedLanguage);

  useEffect(() => {
    if (selectedLanguage) {
      weapon.cache_language_pack(selectedLanguage);
    }
  }, [selectedLanguage, weapon])

  const introScreens = skipOnboarding ? [] : [
    {
      title: "Yap works differently than other language learning apps.",
      content: "Other apps break your learning down into discrete lessons, where you study topics like \"animals\" or \"body parts\"."
    },
    {
      title: "In Yap, you'll learn the most common words first.",
      content: "It might surprise you which words are the most common!"
    },
    {
      title: "Other apps have a set lesson plan. They don't adapt to you.",
      content: "This makes you waste time. Your learning needs are unique!"
    },
    {
      title: "But Yap adapts your lessons to focus on the words you most need to study.",
      content: "Yap's scheduler has you review each word right when you're about to forget it."
    },
    {
      title: "This is called \"spaced repetition\", and it's the most efficient way to learn.",
      content: "Once you see how efficient it is, you'll never want to go back."
    }
  ];

  return (
    <div className="flex items-center justify-center p-8">
      <AnimatePresence mode="wait">
        {!selectedLanguage ? (
          <motion.div
            key="selection"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="w-full max-w-4xl gap-4 flex flex-col items-center"
          >
            <div className="text-center">
              <h1 className="text-5xl font-bold mb-4" style={{ textWrap: "balance" }}>
                What do you want to learn?
              </h1>
            </div>

            <div className="grid md:grid-cols-3 gap-8 w-full">
              {languages.map((lang) => (
                <motion.div
                  key={lang.name}
                  whileHover={{ scale: 1.05 }}
                  whileTap={{ scale: 0.98 }}
                >
                  <Card
                    className="relative overflow-hidden p-8 text-center group transition-all duration-300 hover:shadow-2xl cursor-pointer border-2"
                    onClick={() => {
                      setSelectedLanguage(lang.name);
                    }}
                  >
                    <div
                      className="absolute inset-0 opacity-0 group-hover:opacity-10 transition-opacity duration-300"
                      style={{ background: lang.colors.gradient }}
                    />
                    <div className="relative z-10">
                      <div className="text-8xl mb-4">{lang.flag}</div>
                      <h2 className="text-3xl font-bold mb-2">
                        {lang.name}
                      </h2>
                    </div>
                  </Card>
                </motion.div>
              ))}
            </div>

            <div className="text-center mb-12">
              <p className="text-xl">
                (Yap.Town is great for beginner and intermediate students.)
              </p>
            </div>
          </motion.div>
        ) : selectedLang ? (
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            className="w-full max-w-4xl"
          >
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
                    <Card className="p-12">
                      <div className="text-center">
                        <h2 className="text-3xl font-bold mb-6" style={{ textWrap: "balance" }}>
                          {screen.title}
                        </h2>
                        <p className="text-lg mb-8" style={{ textWrap: "balance" }}>
                          {screen.content}
                        </p>
                      </div>
                    </Card>
                  </CarouselItem>
                ))}
                <CarouselItem>
                  <Card
                    className="p-12"
                    style={{
                      background: `linear-gradient(135deg, ${selectedLang.colors.primary}10, ${selectedLang.colors.accent}10)`,
                    }}
                  >
                    <div className="text-center">
                      <motion.div
                        initial={{ scale: 0 }}
                        animate={{ scale: 1 }}
                        transition={{ type: "spring", stiffness: 200, damping: 15 }}
                        className="text-9xl mb-6"
                      >
                        {selectedLang.flag}
                      </motion.div>
                      <h2 className="text-3xl font-bold mb-6">
                        Ready to start learning {selectedLang.name}?
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
                    setSelectedLanguage(null);
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
                    onLanguageConfirmed(selectedLang.name);
                  } else {
                    api?.scrollNext();
                  }
                }}
                className={`min-w-[120px] flex items-center gap-2 ${current === introScreens.length ? 'text-white hover:opacity-90 active:scale-95 transition-all' : ''
                  }`}
                style={{
                  ...(current === introScreens.length ? {
                    background: `linear-gradient(135deg, ${selectedLang.colors.primary}, ${selectedLang.colors.accent})`,
                  } : {})
                }}
              >
                {current === introScreens.length ? selectedLang.confirmText : 'Next'}
                <ArrowRight className="h-4 w-4" />
              </Button>
            </div>
          </motion.div>
        ) : (
          null
        )}
      </AnimatePresence>
    </div>
  );
}
