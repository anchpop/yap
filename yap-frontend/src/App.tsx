import { useState, useEffect, Profiler, useMemo, useCallback, useSyncExternalStore } from 'react'
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { CardSummary, Deck, type CardType, type Challenge, type ChallengeType, type Language, type Lexeme, type /* comes from TranscriptionChallenge */ PartGraded } from '../../yap-frontend-rs/pkg'
import { Button } from "@/components/ui/button.tsx"
import { ThemeProvider } from "@/components/theme-provider"
import { supabase } from '@/lib/supabase'
import type { Session as SupabaseSession } from '@supabase/supabase-js'
import { useNetworkState } from 'react-use';
import { Flashcard } from '@/components/Flashcard'
import { TranslationChallenge } from '@/components/challenges/TranslationChallenge'
import { profilerOnRender } from './lib/utils'
import { ResetPassword } from '@/pages/reset-password'
import { ConfirmEmail } from '@/pages/confirm-email'
import { AcceptInvite } from '@/pages/accept-invite'
import { ForgotPassword } from '@/pages/forgot-password'
import { playSoundEffect } from '@/lib/sound-effects'
import { registerSW } from 'virtual:pwa-register'
import { NoCardsDue } from '@/components/no-cards-due'

import type { Dispatch, SetStateAction } from 'react'
import type { RegisterSWOptions } from 'vite-plugin-pwa/types'
declare module 'virtual:pwa-register/react' {
  export function useRegisterSW(options?: RegisterSWOptions): {
    needRefresh: [boolean, Dispatch<SetStateAction<boolean>>]
    offlineReady: [boolean, Dispatch<SetStateAction<boolean>>]
    updateServiceWorker: (reloadPage?: boolean) => Promise<void>
  }
}
import { useRegisterSW } from 'virtual:pwa-register/react'
import { TranscriptionChallenge } from './components/challenges/TranscriptionChallenge'
import { LanguageSelector } from './components/LanguageSelector'
import { WeaponProvider, useAsyncMemo, useWeapon, useWeaponState, useWeaponSupport, type WeaponToken } from './weapon'
import { Header } from '@/components/header'
import { Toaster } from 'sonner'
import { BrowserNotSupported } from '@/components/browser-not-supported'
import { Stats } from '@/components/stats'
import { About } from '@/components/about'
import { match, P } from 'ts-pattern';

// Essential user info to persist for offline functionality
export interface UserInfo {
  id: string
  email: string
}

function AppMain() {
  // register service worker
  const updateIntervalMS = 60 * 5 * 1000; // every 5 minutes
  useEffect(() => {
    registerSW({ immediate: true })
    console.log("service worker hopefully registered");
  }, [])

  useRegisterSW({
    onRegistered(r) {
      if (r) {
        setInterval(() => {
          r.update()
        }, updateIntervalMS)
      }
    }
  });

  return (
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <AppCheckBrowserSupport />
      <Toaster />
    </ThemeProvider>
  )
}

function AppCheckBrowserSupport() {
  const token = useWeaponSupport()
  const supported = token.browserSupported
  if (supported === null) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <p className="text-muted-foreground">Checking browser support...</p>
      </div>
    )
  }
  else if (supported === false) {
    return <BrowserNotSupported />
  }
  else {
    return <AppCheckLoggedIn weaponToken={{ browserSupported: supported }} />
  }
}

function AppCheckLoggedIn({ weaponToken }: { weaponToken: WeaponToken }) {
  void weaponToken
  const [session, setSession] = useState<SupabaseSession | null>(null)

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setSession(session)
    })
    supabase.auth.onAuthStateChange((event, session) => {
      setSession(session)
      if (event === 'SIGNED_IN') {
        localStorage.setItem('yap-user-info', JSON.stringify({
          id: session?.user.id,
          email: session?.user.email
        }))
      } else if (event === 'SIGNED_OUT') {
        localStorage.removeItem('yap-user-info')

        if (window.OneSignal) {
          window.OneSignal.logout()
        }
      }
    })
  }, [])

  let userInfo: UserInfo | undefined;

  if (session) {
    userInfo = {
      id: session.user.id,
      email: session.user.email!
    }
  } else {
    const cachedUserInfo = localStorage.getItem('yap-user-info')
    if (cachedUserInfo) {
      try {
        userInfo = JSON.parse(cachedUserInfo)
      } catch {
        localStorage.removeItem('yap-user-info')
      }
    }
  }

  const accessToken = session?.access_token

  return (
    <WeaponProvider userId={userInfo?.id} accessToken={accessToken}>
      <AppTestWeapon userInfo={userInfo} accessToken={accessToken} />
    </WeaponProvider>
  )
}

function AppTestWeapon({ userInfo, accessToken }: { userInfo: UserInfo | undefined, accessToken: string | undefined }) {
  const weaponState = useWeaponState()

  if (weaponState.type === 'loading') {
    return (
      <div>
        <div className="min-h-screen bg-background flex items-center justify-center">
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </div>
    )
  }
  else if (weaponState.type === 'error') {
    return (
      <div>
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <div className="max-w-md w-full bg-card border rounded-lg p-6 text-center">
            <div className="w-12 h-12 bg-red-100 dark:bg-red-900/20 rounded-full flex items-center justify-center mx-auto mb-4">
              <span className="text-red-600 dark:text-red-400 text-xl">⚠</span>
            </div>
            <h2 className="text-lg font-semibold mb-2">Failed to Initialize Deck</h2>
            <p className="text-muted-foreground mb-4">{weaponState.message}</p>
            <Button
              onClick={() => window.location.reload()}
              variant="outline"
            >
              Try Again
            </Button>
          </div>
        </div>
      </div>
    )
  }
  else if (weaponState.type === 'ready') {
    return <AppContent userInfo={userInfo} accessToken={accessToken} />
  }
}

function AppContent({ userInfo, accessToken }: { userInfo: UserInfo | undefined, accessToken: string | undefined }) {
  const weapon = useWeapon()
  const deck = useDeck()

  const [requestedLanguageChange, setRequestedLanguageChange] = useState(false);

  return (
    <Profiler id="App" onRender={profilerOnRender}>
      <div>
        <div className="min-h-screen bg-background text-foreground">
          <div className="max-w-2xl mx-auto">
            <Profiler id="Review" onRender={profilerOnRender}>
              <div className="flex flex-col p-2" style={{ minHeight: 'calc(100dvh)' }}>
                <Header
                  userInfo={userInfo}
                  onSignOut={() => supabase.auth.signOut()}
                  onChangeLanguage={deck?.type === 'deck' ? () => {
                    setRequestedLanguageChange(true)
                  } : undefined}
                  showSignupNag={deck?.type === 'deck' && deck.deck !== null}
                />
                {
                  match(deck)
                    .with({ type: "deck", deck: null }, () =>
                      <div className="min-h-screen bg-background flex items-center justify-center">
                        <p className="text-muted-foreground">Loading...</p>
                      </div>)
                    .with({ type: "deck", deck: P.not(P.nullish) }, ({ deck, language }) => (
                      !requestedLanguageChange ?
                        <Review
                          userInfo={userInfo}
                          accessToken={accessToken}
                          deck={deck}
                          targetLanguage={language}
                        /> :
                        <LanguageSelector
                          skipOnboarding={true}
                          onLanguageConfirmed={(language) => {
                            console.log(`Language selected: ${language}`)
                            weapon.add_deck_selection_event({ SelectLanguage: language })
                            setRequestedLanguageChange(false)
                          }} />

                    ))
                    .with({ type: "noLanguageSelected" }, () => (
                      <LanguageSelector
                        skipOnboarding={false}
                        onLanguageConfirmed={(language) => {
                          console.log(`Language selected: ${language}`)
                          weapon.add_deck_selection_event({ SelectLanguage: language })
                        }} />
                    ))
                    .with(null, () =>
                      <div className="min-h-screen bg-background flex items-center justify-center">
                        <p className="text-muted-foreground">Loading...</p>
                      </div>)
                    .exhaustive()
                }
              </div>
              {deck ? (
                deck.type === "deck" && !requestedLanguageChange ? (
                  deck.deck ? (
                    <Stats deck={deck.deck} />
                  ) : <></>
                ) : <></>
              ) : <></>}
              <About />
            </Profiler>
            <div className="p-2"></div>
          </div>
        </div>
      </div>
    </Profiler>
  )
}
function findNextDueCard(deck: Deck): CardSummary | null {
  const allCards = deck.get_all_cards_summary()
  const now = Date.now()
  const futureCards = allCards.filter(card => card.due_timestamp_ms > now)
  return futureCards.length > 0 ? futureCards[0] : null
}

interface ReviewProps {
  userInfo: UserInfo | undefined
  accessToken: string | undefined
  deck: Deck
  targetLanguage: Language
}



function Review({ userInfo, accessToken, deck, targetLanguage }: ReviewProps) {
  const weapon = useWeapon()

  const CANT_LISTEN_DURATION_MS = 15 * 60 * 1000;

  const [showAnswer, setShowAnswer] = useState(false)
  const network = useNetworkState()
  const [cardsBecameDue, setCardsBecameDue] = useState<number>(0)

  const nextDueCard = findNextDueCard(deck)

  // Update scheduled push notifications when the deck state changes
  useEffect(() => {
    try {
      if (accessToken && userInfo?.id) { deck.submit_push_notifications(accessToken, userInfo?.id) }
    }
    catch {
      console.error("An error occured when trying to update the notification schedule");
    }
  }, [deck, userInfo?.id, accessToken])

  // Schedule re-render when next card becomes due
  useEffect(() => {
    const next_due_timestamp_ms = nextDueCard?.due_timestamp_ms;
    if (next_due_timestamp_ms) {
      const timeUntilDueMs = next_due_timestamp_ms - Date.now();

      if (timeUntilDueMs > 0 && timeUntilDueMs < 24 * 60 * 60 * 1000) { // Only schedule if within 24 hours
        const timeout = setTimeout(() => {
          setCardsBecameDue(cardsBecameDue => cardsBecameDue + 1000)
        }, timeUntilDueMs + 1)

        return () => clearTimeout(timeout)
      }
    }
  }, [nextDueCard?.due_timestamp_ms])

  // reviewInfo.get_next_challenge can sometimes be slow, so we memoize it
  const [bannedChallengeTypes, setBannedChallengeTypes] = useState<ChallengeType[]>(() => {
    const cantListenTimestamp = localStorage.getItem('yap-cant-listen-timestamp');
    if (cantListenTimestamp) {
      const timestamp = parseInt(cantListenTimestamp);
      const elapsed = Date.now() - timestamp;

      if (elapsed < CANT_LISTEN_DURATION_MS) {
        console.log(`Can't listen mode active (${(elapsed / (1000 * 60)).toFixed(1)} minutes ago)`);
        return ['Listening'];
      } else {
        localStorage.removeItem('yap-cant-listen-timestamp');
      }
    }
    return [];
  });

  useEffect(() => {
    if (bannedChallengeTypes.includes('Listening')) {
      const cantListenTimestamp = localStorage.getItem('yap-cant-listen-timestamp');
      if (cantListenTimestamp) {
        const timestamp = parseInt(cantListenTimestamp);
        const elapsed = Date.now() - timestamp;
        const remaining = CANT_LISTEN_DURATION_MS - elapsed;

        if (remaining > 0) {
          const timeout = setTimeout(() => {
            setBannedChallengeTypes(banned => banned.filter(t => t !== 'Listening'));
            localStorage.removeItem('yap-cant-listen-timestamp');
            console.log("Can't listen mode expired after 15 minutes");
          }, remaining);
          return () => clearTimeout(timeout);
        } else {
          setBannedChallengeTypes(banned => banned.filter(t => t !== 'Listening'));
          localStorage.removeItem('yap-cant-listen-timestamp');
        }
      }
    }
  }, [bannedChallengeTypes]);

  const reviewInfo = useMemo(() => {
    return deck.get_review_info(bannedChallengeTypes)
  }, [deck, bannedChallengeTypes, cardsBecameDue]);

  const { currentChallenge, currentCardId, addCardOptions } = useMemo(() => {
    const currentCardId = reviewInfo.get_next_review_card();
    const currentChallenge: Challenge<string> | undefined = currentCardId !== undefined ? reviewInfo.get_challenge_for_card(deck, currentCardId) : undefined;
    const addCardOptions = deck.add_card_options();
    return { currentChallenge, currentCardId, addCardOptions };
  }, [deck, reviewInfo])

  useEffect(() => {
    const abortController = new AbortController();

    deck.cache_challenge_audio(reviewInfo, accessToken, abortController.signal);

    return () => {
      abortController.abort();
    };
  }, [deck, accessToken, reviewInfo])


  const addNextCards = useCallback(async (card_type: CardType | undefined, count: number) => {
    const event = deck.add_next_unknown_cards(card_type, count);
    if (event) {
      weapon.add_deck_event(event);
    }
  }, [deck, weapon])

  const handleRating = async (rating: 'again' | 'hard' | 'good' | 'easy') => {
    if (!currentChallenge || !('FlashCardReview' in currentChallenge)) {
      console.error("handleRating called with no current challenge or no FlashCardReview in current challenge");
      return
    };

    // Play sound effect in background based on rating
    if (rating === 'again') {
      playSoundEffect('fail'); // Don't await - play in background
    } else {
      playSoundEffect('success'); // Don't await - play in background
    }

    const event = deck.review_card(currentChallenge.FlashCardReview.indicator, rating);
    if (event) {
      weapon.add_deck_event(event);
      setShowAnswer(false);
    }
  }

  const handleTranslationComplete = async (grade: { wordStatuses: [Lexeme<string>, boolean | null][] } | { perfect: string | null }, submission: string) => {
    if (!currentChallenge || !('TranslateComprehensibleSentence' in currentChallenge)) {
      console.error("handleTranslationComplete called with no current challenge or no TranslateComprehensibleSentence in current challenge");
      return
    };

    // Play success sound in background for sentence completion (regardless of perfect or errors)
    playSoundEffect('success'); // Don't await - play in background

    const sentence = currentChallenge.TranslateComprehensibleSentence;

    if ("perfect" in grade) {
      // Perfect sentence review
      const event = deck.translate_sentence_perfect(sentence.target_language);
      if (event) {
        weapon.add_deck_event(event);
      }
      setShowAnswer(false);
    } else {
      // Wrong sentence review with word statuses
      const wordsRemembered: Lexeme<string>[] = [];
      const wordsForgotten: Lexeme<string>[] = [];

      grade.wordStatuses.forEach(([word, status]) => {
        if (status === true) {
          wordsRemembered.push(word);
        } else if (status === false) {
          wordsForgotten.push(word);
        }
      });

      const event = deck.translate_sentence_wrong(
        sentence.target_language,
        submission,
        wordsRemembered,
        wordsForgotten
      );
      if (event) {
        weapon.add_deck_event(event);
      }
      setShowAnswer(false);
    }
  }

  const handleTranscriptionComplete = useCallback((grade: /* comes from TranscriptionChallenge*/ PartGraded[]) => {
    if (!currentChallenge || !('TranscribeComprehensibleSentence' in currentChallenge)) {
      console.error("handleTranscriptionComplete called with no current challenge or no TranscribeComprehensibleSentence in current challenge");
      return
    };

    // Play success sound in background for sentence completion (regardless of perfect or errors)
    playSoundEffect('success'); // Don't await - play in background

    console.log('Transcription challenge completed:', grade)
    const event = deck.transcribe_sentence(grade);
    if (event) {
      weapon.add_deck_event(event);
    }
    setShowAnswer(false)
  }, [deck, currentChallenge, weapon])

  const toggleAnswer = () => {
    setShowAnswer(!showAnswer)
  }

  const handleCantListen = () => {
    const timestamp = Date.now();
    localStorage.setItem('yap-cant-listen-timestamp', timestamp.toString());
    setBannedChallengeTypes(banned => banned.includes('Listening') ? banned : [...banned, 'Listening']);
    console.log('Can\'t listen mode activated at:', new Date(timestamp).toISOString());
  }

  useEffect(() => {
    const handleKeyPress = (event: KeyboardEvent) => {
      // Don't handle shortcuts if user is typing in an input field
      const target = event.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT') {
        return;
      }

      if (event.code === 'Space' || event.code === 'Enter') {
        if (deck.num_cards() === 0) {
          event.preventDefault();
          addNextCards(undefined, 1);
        } else if (reviewInfo.due_count === 0 && !currentChallenge) {
          event.preventDefault();
          addNextCards(undefined, addCardOptions.smart_add);
        }
      }
    };

    window.addEventListener('keydown', handleKeyPress);

    return () => {
      window.removeEventListener('keydown', handleKeyPress);
    };
  }, [addNextCards, deck, reviewInfo, currentChallenge, addCardOptions.smart_add]);

  return (
    <>
      {/* main content */}
      <div className="flex flex-col flex-1">

        {deck.num_cards() === 0 ? (
          <div className="bg-card text-card-foreground rounded-lg p-12 text-center border">
            <p className="text-lg mb-2">You don't have any flashcards yet!</p>
            <Button
              onClick={() => addNextCards(undefined, 1)}
              variant="default"
            >
              Add a word to my deck
            </Button>
          </div>
        ) : reviewInfo.due_count === 0 && !currentChallenge ? (
          <NoCardsDue
            nextDueCard={nextDueCard}
            addNextCards={addNextCards}
            showEngagementPrompts={reviewInfo.total_count > 5 && network.online === true && userInfo !== undefined}
            addCardOptions={addCardOptions}
            targetLanguage={targetLanguage}
          />
        ) : currentChallenge ? (
          ('FlashCardReview' in currentChallenge) ? (
            <Flashcard
              audioRequest={currentChallenge.FlashCardReview.audio}
              content={currentChallenge.FlashCardReview.content}
              isNew={currentChallenge.FlashCardReview.is_new}
              showAnswer={showAnswer}
              onToggle={toggleAnswer}
              dueCount={reviewInfo.due_count || 0}
              totalCount={reviewInfo.total_count}
              onRating={handleRating}
              accessToken={accessToken}
              key={currentCardId}
              onCantListen={handleCantListen}
              targetLanguage={targetLanguage}
              listeningPrefix={currentChallenge.FlashCardReview.listening_prefix}
            />
          ) : ('TranslateComprehensibleSentence' in currentChallenge) ? (
            <TranslationChallenge
              sentence={currentChallenge.TranslateComprehensibleSentence}
              onComplete={handleTranslationComplete}
              dueCount={reviewInfo.due_count || 0}
              totalCount={reviewInfo.total_count}
              accessToken={accessToken}
              key={currentCardId}
              unique_target_language_lexeme_definitions={currentChallenge.TranslateComprehensibleSentence.unique_target_language_lexeme_definitions}
              targetLanguage={targetLanguage}
            />
          ) : (
            <TranscriptionChallenge
              challenge={currentChallenge.TranscribeComprehensibleSentence}
              onComplete={handleTranscriptionComplete}
              dueCount={reviewInfo.due_count || 0}
              totalCount={reviewInfo.total_count}
              accessToken={accessToken}
              key={currentCardId}
              onCantListen={handleCantListen}
              targetLanguage={targetLanguage}
            />
          )
        ) : <div>Unexpected challenge state. This is a bug. currentChallenge: {JSON.stringify(currentChallenge)}</div>}
      </div>
      {/* /main content */}


    </>
  )
}

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/reset-password" element={<ResetPassword />} />
        <Route path="/confirm-email" element={<ConfirmEmail />} />
        <Route path="/accept-invite" element={<AcceptInvite />} />
        <Route path="/forgot-password" element={<ForgotPassword />} />
        <Route path="/*" element={<AppMain />} />
      </Routes>
    </BrowserRouter>
  )
}


function useDeck(): { type: "deck", language: Language, deck: Deck | null } | { type: "noLanguageSelected" } | null {
  const weapon = useWeapon()

  useEffect(() => {
    weapon.request_deck_selection()
    weapon.request_reviews()
  }, [weapon])

  const getSnapshot = () => {
    try {
      const num_reviews = weapon.get_stream_num_events("reviews")
      const num_deck_selection = weapon.get_stream_num_events("deck_selection")
      if (num_reviews === undefined || num_deck_selection === undefined) {
        return null
      }
      return num_reviews + num_deck_selection
    } catch {
      return null
    }
  }

  const subscribe = (callback: () => void) => {
    const handle_reviews = weapon.subscribe_to_stream("reviews", () => { callback() })
    const handle_deck_selection = weapon.subscribe_to_stream("deck_selection", () => { callback() })

    return () => {
      weapon.unsubscribe(handle_reviews)
      weapon.unsubscribe(handle_deck_selection)
    }
  }

  const numEvents = useSyncExternalStore(subscribe, getSnapshot)

  const state = useAsyncMemo(async () => {
    if (numEvents === null) {
      return null
    }

    const deck_selection = weapon.get_deck_selection_state()
    console.log("deck_selection", deck_selection)
    if (deck_selection === undefined || deck_selection === null) {
      return null
    }
    if (deck_selection == "noneSelected") {
      return { type: "noLanguageSelected" } as { type: "noLanguageSelected" }
    } else {
      return { type: "deck", language: deck_selection.selected, deck: await weapon.get_deck_state(deck_selection.selected) } as { type: "deck", language: Language, deck: Deck | null }
    }
  }, [weapon, numEvents])

  return state ?? null
}

export default App
