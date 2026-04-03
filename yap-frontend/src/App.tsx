import * as Sentry from '@sentry/react'
import { useState, useEffect, Profiler, useSyncExternalStore, useMemo, useCallback } from 'react'
import { useZeno } from '@/hooks/useZeno'
import { createBrowserRouter, RouterProvider, Outlet, useNavigate, useOutletContext, ScrollRestoration } from 'react-router-dom'
import { CardSummary, Deck, type Accomplishment, type CardType, type Challenge, type ChallengeRequirements, type Course, type DailyReviewTarget, type Heteronym, type Language, type LiteralGrades, type Gram, type /* comes from TranscriptionChallenge */ PartGraded, type Rating } from '../../yap-frontend-rs/pkg'
import { Button } from "@/components/ui/button.tsx"
import { Progress } from "@/components/ui/progress.tsx"
import { Skeleton } from "@/components/ui/skeleton"
import { Card } from "@/components/ui/card"
import { ThemeProvider } from "@/components/theme-provider"
import { supabase } from '@/lib/supabase'
import type { Session as SupabaseSession } from '@supabase/supabase-js'
import { useInterval, useNetworkState } from 'react-use';
import { Flashcard } from '@/components/Flashcard'
import { TranslationChallenge } from '@/components/challenges/TranslationChallenge'
import { PronunciationChallenge } from '@/components/challenges/PronunciationChallenge'
import { profilerOnRender, languageToIso6391 } from './lib/utils'
import { ResetPassword } from '@/pages/reset-password'
import { ConfirmEmail } from '@/pages/confirm-email'
import { AcceptInvite } from '@/pages/accept-invite'
import { ForgotPassword } from '@/pages/forgot-password'
import { UserProfilePage } from '@/pages/user-profile'
import { AboutPage } from '@/pages/about'
import { LandingPage } from '@/pages/landing'
import { NotFoundPage } from '@/pages/not-found'
import { GoalsPage } from '@/pages/goals'
import { playSoundEffect } from '@/lib/sound-effects'
import { registerSW } from 'virtual:pwa-register'
import { NoCardsReady } from '@/components/no-cards-ready'
import { AccomplishmentScreen } from '@/components/AccomplishmentScreen'
import { useGoal, goalToGoalSelection } from '@/hooks/useGoal'
import { SetDisplayName } from '@/components/SetDisplayName'

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
import { Toaster } from 'sonner'
import { BrowserNotSupported } from '@/components/browser-not-supported'
import { Stats } from '@/components/stats'
import { About } from '@/components/about'
import { Dictionary } from '@/components/Dictionary'
import { Leeches } from '@/components/Leeches'
import { TopPageLayout } from '@/components/TopPageLayout'
import { match, P } from 'ts-pattern';
import { ErrorMessage } from '@/components/ui/error-message'
import { BackgroundShader } from '@/components/BackgroundShader'
import { Movies } from '@/components/Movies'
import { getMovieMetadata } from '@/lib/movie-cache'
import { PlacementTest } from '@/components/PlacementTest'

// Essential user info to persist for offline functionality
export interface UserInfo {
  id: string
  email: string
  displayName: string | null | undefined
}

export type AppContextType = {
  userInfo: UserInfo | undefined
  accessToken: string | undefined
}

function AppMain() {
  // register service worker
  const updateIntervalMS = 60 * 5 * 1000; // every 5 minutes
  useEffect(() => {
    registerSW({ immediate: true })
  }, [])

  useRegisterSW({
    onRegistered(r) {
      if (r) {
        setInterval(() => {
          r.update().catch((e) => {
            if (navigator.onLine) {
              Sentry.captureException(e, { tags: { "sw.online": true } })
            }
          })
        }, updateIntervalMS)
      }
    }
  });

  return <AppCheckBrowserSupport />
}

function AppCheckBrowserSupport() {
  const token = useWeaponSupport()
  const supported = token.browserSupported
  const [progress, setProgress] = useState(0)

  useEffect(() => {
    if (supported !== null) return

    const start = Date.now()
    const timer = setInterval(() => {
      const diff = Date.now() - start
      setProgress(Math.max(1, Math.min(diff / 30, 100)))
    }, 480)

    return () => clearInterval(timer)
  }, [supported])

  const smoothProgress = useZeno(progress)

  if (supported === null) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center space-y-4">
        <p className="text-muted-foreground animate-fade-in-delay-2">Checking device compatibility...</p>
        <Progress value={smoothProgress} className="w-64 animate-fade-in-delay-2" disableTransition />
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
  const [signedOut, setSignedOut] = useState(false)
  const [displayName, setDisplayName] = useState<string | null | undefined>(undefined)

  useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setSession(session)
    })

    const { data: authListener } = supabase.auth.onAuthStateChange((event, session) => {
      setSession(session)
      if (event === 'SIGNED_IN') {
        Sentry.setUser({ id: session?.user.id, email: session?.user.email })
        localStorage.setItem('yap-user-info', JSON.stringify({
          id: session?.user.id,
          email: session?.user.email,
          displayName: undefined // Will be fetched from profiles table
        }))
        setSignedOut(false)
      } else if (event === 'SIGNED_OUT') {
        Sentry.setUser(null)
        localStorage.removeItem('yap-user-info')

        if (window.OneSignal) {
          window.OneSignal.logout()
        }

        setSession(null)
        setDisplayName(undefined)
        setSignedOut(true)
      }
    })

    return () => {
      authListener.subscription.unsubscribe()
    }
  }, [])

  // Fetch display name from Supabase when logged in
  useEffect(() => {
    if (!session?.user.id) {
      setDisplayName(undefined)
      return
    }

    // Fetch initial display name
    const fetchDisplayName = async () => {
      const { data, error } = await supabase
        .from('profiles')
        .select('display_name')
        .eq('id', session.user.id)
        .single()

      if (!error && data) {
        setDisplayName(data.display_name)
      }
    }

    fetchDisplayName()

    // Set up realtime subscription for display_name changes
    const channel = supabase
      .channel(`profile_${session.user.id}`)
      .on(
        'postgres_changes',
        {
          event: 'UPDATE',
          schema: 'public',
          table: 'profiles',
          filter: `id=eq.${session.user.id}`
        },
        (payload) => {
          if (payload.new && 'display_name' in payload.new) {
            setDisplayName(payload.new.display_name as string | null)
          }
        }
      )
      .subscribe()

    return () => {
      supabase.removeChannel(channel)
    }
  }, [session?.user.id])

  // Update localStorage when displayName changes (only when it's been fetched)
  useEffect(() => {
    if (session?.user.id && session?.user.email && displayName !== undefined) {
      localStorage.setItem('yap-user-info', JSON.stringify({
        id: session.user.id,
        email: session.user.email,
        displayName: displayName
      }))
    }
  }, [session?.user.id, session?.user.email, displayName])

  let userInfo: UserInfo | undefined;

  if (session) {
    userInfo = {
      id: session.user.id,
      email: session.user.email!,
      displayName: displayName
    }
  } else if (!signedOut) {
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

function AppTestWeapon({ userInfo, accessToken }: AppContextType) {
  const weaponState = useWeaponState()

  if (weaponState.type === 'loading') {
    return (
      <div>
        <div className="min-h-screen flex items-center justify-center">
          <p className="text-muted-foreground animate-fade-in-delayed">Loading...</p>
        </div>
      </div>
    )
  }
  else if (weaponState.type === 'error') {
    return (
      <div>
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="max-w-md w-full p-6 text-center gap-0">
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
          </Card>
        </div>
      </div>
    )
  }
  else if (weaponState.type === 'ready') {
    return <AppContent userInfo={userInfo} accessToken={accessToken} />
  }
}

function AppContent({ userInfo, accessToken }: AppContextType) {
  return (
    <Profiler id="App" onRender={profilerOnRender}>
      <div className="px-2">
        <div className="min-h-screen text-foreground">
          <div className="max-w-2xl mx-auto">
            <Profiler id="Content" onRender={profilerOnRender}>
              <Outlet context={{ userInfo, accessToken }} />
              <About />
            </Profiler>
            <div className="p-2"></div>
          </div>
        </div>
      </div>
    </Profiler>
  )
}

function LoadingProgress({ message, progress }: { message: string; progress: number }) {
  const smoothProgress = useZeno(progress)
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="w-full max-w-md space-y-4">
        <p className="text-muted-foreground text-center">{message}</p>
        <Progress value={smoothProgress} className="w-full" disableTransition />
      </div>
    </div>
  )
}

function ReviewPage() {
  const { userInfo, accessToken } = useOutletContext<AppContextType>()
  const deck = useDeck()
  const deckSelection = useDeckSelection()
  const navigate = useNavigate()

  useEffect(() => {
    if (deckSelection?.type === 'noLanguageSelected') {
      navigate('/', { replace: true })
    }
  }, [deckSelection, navigate])

  return (
    <div className="flex flex-col gap-6">
      {
        match(deck)
          .with({ type: "loading" }, ({ message, progress }) => (
            <TopPageLayout
              userInfo={userInfo}
              headerProps={{
                onChangeLanguage: () => navigate('/select-language'),
                showSignupNag: false
              }}
            >
              <LoadingProgress message={message} progress={progress} />
            </TopPageLayout>
          ))
          .with({ type: "deck", deck: null }, () => (
            <TopPageLayout
              userInfo={userInfo}
              headerProps={{
                onChangeLanguage: () => navigate('/select-language'),
                showSignupNag: false
              }}
            >
              <div className="flex-1 flex items-center justify-center">
                <p className="text-muted-foreground animate-fade-in-delayed">Loading...</p>
              </div>
            </TopPageLayout>
          ))
          .with({ type: "deck", deck: P.not(P.nullish) }, ({ deck, targetLanguage, nativeLanguage, startingFresh }) => {
            const reviewInfo = deck.get_review_info([], Date.now());

            // Calculate movie stats once for use in both Review and Movies components
            const movieStats = deck.get_movie_stats()
            const movieIds = movieStats.map(s => s.id)
            const metadata = getMovieMetadata(deck, movieIds)
            const metadataMap = new Map(metadata.map(m => [m.id, m]))
            const moviesWithMetadata = movieStats.map(stat => ({
              ...stat,
              ...(metadataMap.get(stat.id) || {}),
            }))

            return (
            <>
              <TopPageLayout
                userInfo={userInfo}
                headerProps={{
                  onChangeLanguage: () => navigate('/select-language'),
                  showSignupNag: deck !== null,
                  language: targetLanguage,
                  dueCount: reviewInfo.due_count || 0
                }}
              >
                <Review
                  userInfo={userInfo}
                  accessToken={accessToken}
                  deck={deck}
                  targetLanguage={targetLanguage}
                  nativeLanguage={nativeLanguage}
                  moviesWithMetadata={moviesWithMetadata}
                  startingFresh={startingFresh}
                />
              </TopPageLayout>
              <Tools deck={deck} />
              <Movies moviesWithMetadata={moviesWithMetadata} targetLanguageIso={languageToIso6391(targetLanguage)} deck={deck} />
              <Stats deck={deck} targetLanguage={targetLanguage} />
            </>
            );
          })
          .with({ type: "noLanguageSelected" }, () => (
            <TopPageLayout
              userInfo={userInfo}
              headerProps={{ showSignupNag: false }}
            >
              <div className="flex-1 flex items-center justify-center">
                <p className="text-muted-foreground animate-fade-in-delayed">Loading...</p>
              </div>
            </TopPageLayout>
          ))
          .with({ type: "error" }, ({ message, retry }) => (
            <TopPageLayout
              userInfo={userInfo}
              headerProps={{
                onChangeLanguage: () => navigate('/select-language'),
                showSignupNag: false
              }}
            >
              <div className="flex-1 flex items-center justify-center p-4">
                <Card className="max-w-md w-full p-6 gap-0">
                  <div className="w-12 h-12 bg-red-100 dark:bg-red-900/20 rounded-full flex items-center justify-center mx-auto mb-4">
                    <span className="text-red-600 dark:text-red-400 text-xl">⚠</span>
                  </div>
                  <h2 className="text-lg font-semibold mb-2 text-center">Failed to Load Language Data</h2>
                  <p className="text-muted-foreground mb-4 text-center">
                    Unable to download the language pack. Please check your internet connection.
                  </p>
                  <ErrorMessage message={message} title="Failed to load language data" className="mb-4" />
                  <Button onClick={retry} variant="outline" className="w-full">
                    Try Again
                  </Button>
                </Card>
              </div>
            </TopPageLayout>
          ))
          .with(null, () => (
            <TopPageLayout
              userInfo={userInfo}
              headerProps={{ showSignupNag: false }}
            >
            <div className="flex items-center justify-center p-4 animate-fade-in-delayed">
              <Skeleton className="h-48 w-full max-w-2xl" />
             </div>
            </TopPageLayout>
          ))
          .exhaustive()
      }
    </div>
  )
}

function Tools({ deck: _deck }: { deck: Deck }) {
  const navigate = useNavigate()

  return (
    <div className="">
      <h2 className="text-2xl font-semibold animate-fade-in-delay-2">Tools</h2>
      <Card className="p-4 mt-3 space-y-2 gap-0" animate>
        <button
          onClick={() => navigate('/dictionary')}
          className="w-full flex items-center justify-between px-3 py-2 rounded-md hover:bg-muted transition-colors mb-0"
        >
          <span>📖 Dictionary</span>
          <span className="text-muted-foreground">→</span>
        </button>
        <button
          onClick={() => navigate('/leeches')}
          className="w-full flex items-center justify-between px-3 py-2 rounded-md hover:bg-muted transition-colors"
        >
          <span>🩹 Leeches</span>
          <span className="text-muted-foreground">→</span>
        </button>
      </Card>
    </div>
  )
}

function DictionaryPage() {
  const { userInfo } = useOutletContext<AppContextType>()
  const deck = useDeck()
  const weapon = useWeapon()
  const navigate = useNavigate()

  useEffect(() => {
    if (deck?.type === 'noLanguageSelected') {
      navigate('/', { replace: true })
    }
  }, [deck, navigate])

  if (deck?.type === 'noLanguageSelected') {
    return null
  }

  if (deck?.type !== 'deck') {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: 'Dictionary', onBack: () => navigate('/learn') }
        }}
      >
        <div className="flex-1 flex items-center justify-center">
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </TopPageLayout>
    )
  }

  if (!deck.deck) {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: 'Dictionary', onBack: () => navigate('/learn') }
        }}
      >
        <div className="flex-1 bg-background flex items-center justify-center">
          <p className="text-muted-foreground">Loading dictionary...</p>
        </div>
      </TopPageLayout>
    )
  }

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{
        backButton: { label: 'Dictionary', onBack: () => navigate('/learn') }
      }}
    >
      <Dictionary deck={deck.deck} weapon={weapon} targetLanguage={deck.targetLanguage} nativeLanguage={deck.nativeLanguage} />
    </TopPageLayout>
  )
}

function LeechesPage() {
  const { userInfo } = useOutletContext<AppContextType>()
  const deck = useDeck()
  const navigate = useNavigate()

  useEffect(() => {
    if (deck?.type === 'noLanguageSelected') {
      navigate('/', { replace: true })
    }
  }, [deck, navigate])

  if (deck?.type === 'noLanguageSelected') {
    return null
  }

  if (deck?.type !== 'deck') {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: 'Leeches', onBack: () => navigate('/learn') }
        }}
      >
        <div className="flex-1 flex items-center justify-center">
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </TopPageLayout>
    )
  }

  if (!deck.deck) {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: 'Leeches', onBack: () => navigate('/learn') }
        }}
      >
        <div className="flex-1 bg-background flex items-center justify-center">
          <p className="text-muted-foreground">Loading leeches...</p>
        </div>
      </TopPageLayout>
    )
  }

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{
        backButton: { label: 'Leeches', onBack: () => navigate('/learn') }
      }}
    >
      <Leeches deck={deck.deck} targetLanguage={deck.targetLanguage} />
    </TopPageLayout>
  )
}

function findNextDueCard(deck: Deck): CardSummary | null {
  const allCards = deck.get_all_cards_summary()
  const now = Date.now()
  const futureCards = allCards.filter(card => card.due_timestamp_ms > now)
  return futureCards.length > 0 ? futureCards[0] : null
}

interface MovieWithMetadata {
  id: string
  percent_known: number
  all_available_learned: boolean
  cards_to_next_milestone: number | null | undefined
  title?: string
  year?: number
  original_language?: string
}

interface ReviewProps {
  userInfo: UserInfo | undefined
  accessToken: string | undefined
  deck: Deck
  targetLanguage: Language
  nativeLanguage: Language
  moviesWithMetadata: MovieWithMetadata[]
  startingFresh: boolean | undefined
}

function Review({ userInfo, accessToken, deck, targetLanguage, nativeLanguage, moviesWithMetadata, startingFresh }: ReviewProps) {
  const weapon = useWeapon()
  const { goal, setGoal } = useGoal(deck.get_goal())

  const CANT_LISTEN_DURATION_MS = 15 * 60 * 1000;

  const network = useNetworkState()
  const [cardsBecameDue, setCardsBecameDue] = useState<number>(0)
  const [lastAutoPlayReviewCount, setLastAutoPlayReviewCount] = useState<bigint | null>(null)
  const [dismissedSetDisplayName, setDismissedSetDisplayName] = useState(() => {
    return localStorage.getItem('yap-skipped-set-display-name') === 'true'
  })

  const totalReviewsCompleted = deck.get_total_reviews()
  const autoplayed = lastAutoPlayReviewCount == totalReviewsCompleted
  const setAutoplayed = useCallback(() => setLastAutoPlayReviewCount(totalReviewsCompleted), [totalReviewsCompleted])

  const accomplishment: Accomplishment | undefined = deck.get_accomplishment()
  const [dismissedAccomplishment, setDismissedAccomplishment] = useState(false)
  // Reset dismissed state when a new accomplishment appears (i.e. totalReviewsCompleted changes)
  useEffect(() => {
    setDismissedAccomplishment(false)
  }, [totalReviewsCompleted])

  const nextDueCard = findNextDueCard(deck)

  // Filter movies to target language for goal selector
  const targetLanguageIso = languageToIso6391(targetLanguage)
  const targetLanguageMovies = useMemo(() => {
    return moviesWithMetadata.filter(m => m.original_language === targetLanguageIso)
  }, [moviesWithMetadata, targetLanguageIso])

  const hasPimsleur = useMemo(() => {
    return deck.get_pimsleur_stats().length > 0
  }, [deck])


  // Update scheduled push notifications and language stats when the deck state changes
  useEffect(() => {
    if (accessToken && userInfo?.id) {
      deck.submit_push_notifications(accessToken, userInfo?.id)
        .catch(() => console.error("Failed to update notification schedule"));
      deck.submit_language_stats(accessToken)
        .catch(() => console.error("Failed to update language stats"));
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

  const computeBannedChallengeTypes = useCallback(() => {
    const banned: ChallengeRequirements[] = [];
    
    const cantListenTimestamp = localStorage.getItem('yap-cant-listen-timestamp');
    if (cantListenTimestamp) {
      const timestamp = parseInt(cantListenTimestamp);
      const elapsed = Date.now() - timestamp;

      if (elapsed < CANT_LISTEN_DURATION_MS) {
        banned.push('Listening');
      } else {
        localStorage.removeItem('yap-cant-listen-timestamp');
      }
    }
    
    const cantSpeakTimestamp = localStorage.getItem('yap-cant-speak-timestamp');
    if (cantSpeakTimestamp) {
      const timestamp = parseInt(cantSpeakTimestamp);
      const elapsed = Date.now() - timestamp;

      if (elapsed < CANT_LISTEN_DURATION_MS) {
        banned.push('Speaking');
      } else {
        localStorage.removeItem('yap-cant-speak-timestamp');
      }
    }
    
    return banned;
  }, [CANT_LISTEN_DURATION_MS]);
  const [bannedChallengeTypes, setBannedChallengeTypes] = useState<ChallengeRequirements[]>(() => computeBannedChallengeTypes());

  const reviewInfo = useMemo(() => {
    const now = Date.now();
    return deck.get_review_info(bannedChallengeTypes, now)
    // cardsBecameDue is intentionally included to trigger recalculation when cards become due
  }, [deck, bannedChallengeTypes, cardsBecameDue]);

  useInterval(() => setCardsBecameDue(cardsBecameDue => cardsBecameDue + 1), reviewInfo.due_count === 0 ? 1000 : 60000);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const currentChallenge: Challenge<any> | undefined = useMemo(() => reviewInfo.get_next_challenge(deck), [reviewInfo, deck]);

  useEffect(() => {
    if (!currentChallenge) {
      setBannedChallengeTypes(computeBannedChallengeTypes());
    }
  }, [currentChallenge, computeBannedChallengeTypes]);

  useEffect(() => {
    if (currentChallenge) return;
    const timeouts: ReturnType<typeof setTimeout>[] = [];

    const scheduleRefresh = (storageKey: string) => {
      const timestamp = localStorage.getItem(storageKey);
      if (!timestamp) return;
      const elapsed = Date.now() - parseInt(timestamp);
      const remaining = CANT_LISTEN_DURATION_MS - elapsed;

      if (remaining > 0) {
        timeouts.push(setTimeout(() => {
          setBannedChallengeTypes(computeBannedChallengeTypes());
        }, remaining));
      } else {
        setBannedChallengeTypes(computeBannedChallengeTypes());
      }
    };

    scheduleRefresh('yap-cant-listen-timestamp');
    scheduleRefresh('yap-cant-speak-timestamp');

    return () => timeouts.forEach(timeout => clearTimeout(timeout));
  }, [currentChallenge, bannedChallengeTypes, CANT_LISTEN_DURATION_MS, computeBannedChallengeTypes]);

  useEffect(() => {
    if (!currentChallenge) {
      setBannedChallengeTypes(computeBannedChallengeTypes());
    }
  }, [currentChallenge, computeBannedChallengeTypes]);

  useEffect(() => {
    if (currentChallenge) return;
    const timeouts: ReturnType<typeof setTimeout>[] = [];

    const scheduleRefresh = (storageKey: string) => {
      const timestamp = localStorage.getItem(storageKey);
      if (!timestamp) return;
      const elapsed = Date.now() - parseInt(timestamp);
      const remaining = CANT_LISTEN_DURATION_MS - elapsed;

      if (remaining > 0) {
        timeouts.push(setTimeout(() => {
          setBannedChallengeTypes(computeBannedChallengeTypes());
        }, remaining));
      } else {
        setBannedChallengeTypes(computeBannedChallengeTypes());
      }
    };

    scheduleRefresh('yap-cant-listen-timestamp');
    scheduleRefresh('yap-cant-speak-timestamp');

    return () => timeouts.forEach(timeout => clearTimeout(timeout));
  }, [currentChallenge, bannedChallengeTypes, CANT_LISTEN_DURATION_MS, computeBannedChallengeTypes]);

  useEffect(() => {
    const abortController = new AbortController();

    deck.cache_challenge_audio(accessToken, abortController.signal);

    return () => {
      abortController.abort();
    };
  }, [deck, accessToken, reviewInfo])


  const goalSelection = goalToGoalSelection(goal);

  const addNextCards = useCallback(async (card_type: CardType | undefined, count: number) => {
    const event = deck.add_next_unknown_cards(card_type, count, bannedChallengeTypes, goalSelection);
    if (event) {
      weapon.add_deck_event(event);
    }
  }, [deck, weapon, bannedChallengeTypes, goalSelection])

  const handleRating = async (rating: Rating) => {
    if (!currentChallenge || (currentChallenge.type !== 'FlashCardReview' && currentChallenge.type !== 'PronunciationChallenge')) {
      console.error("handleRating called with no current challenge or incompatible challenge type");
      return
    };

    // Play sound effect in background based on rating
    if (rating === 'again') {
      playSoundEffect('fail'); // Don't await - play in background
    } else {
      playSoundEffect('success'); // Don't await - play in background
    }

    window.scrollTo({ top: 0 });

    const event = deck.review_card(currentChallenge.indicator, rating);
    if (event) {
      weapon.add_deck_event(event);
    }
  }

  const handleTranslationComplete = useCallback(async (
    grade: { literalGrades: LiteralGrades, phrasesRemembered: Gram<string>[], phrasesForgot: Gram<string>[] } | { perfect: string | null },
    wordsTapped: Heteronym<string>[],
    submission: string
  ) => {
    if (!currentChallenge || currentChallenge.type !== 'TranslateComprehensibleSentence') {
      console.error("handleTranslationComplete called with no current challenge or no TranslateComprehensibleSentence in current challenge");
      return
    };

    // Play success sound in background for sentence completion (regardless of perfect or errors)
    playSoundEffect('success'); // Don't await - play in background
    window.scrollTo({ top: 0 });

    if ("perfect" in grade) {
      // Perfect sentence review
      const event = deck.translate_sentence_perfect(wordsTapped, currentChallenge.target_language);
      if (event) {
        weapon.add_deck_event(event);
      }
    } else {
      // Wrong sentence review - pass literal grades directly to Rust
      const event = deck.translate_sentence_wrong(
        currentChallenge.target_language,
        submission,
        grade.literalGrades,
        wordsTapped,
        grade.phrasesRemembered,
        grade.phrasesForgot
      );
      if (event) {
        weapon.add_deck_event(event);
      }
    }
  }, [deck, currentChallenge, weapon])

  const handleTranscriptionComplete = useCallback((grade: /* comes from TranscriptionChallenge*/ PartGraded[]) => {
    if (!currentChallenge || currentChallenge.type !== 'TranscribeComprehensibleSentence') {
      console.error("handleTranscriptionComplete called with no current challenge or no TranscribeComprehensibleSentence in current challenge");
      return
    };

    // Play success sound in background for sentence completion (regardless of perfect or errors)
    playSoundEffect('success'); // Don't await - play in background
    window.scrollTo({ top: 0 });

    const event = deck.transcribe_sentence(grade);
    if (event) {
      console.log("event", event);
      weapon.add_deck_event(event);
    }
  }, [deck, currentChallenge, weapon])

  const handleCantListen = () => {
    const timestamp = Date.now();
    localStorage.setItem('yap-cant-listen-timestamp', timestamp.toString());
    setBannedChallengeTypes(banned => banned.includes('Listening') ? banned : [...banned, 'Listening']);
  }
  
  const handleCantSpeak = () => {
    const timestamp = Date.now();
    localStorage.setItem('yap-cant-speak-timestamp', timestamp.toString());
    setBannedChallengeTypes(banned => banned.includes('Speaking') ? banned : [...banned, 'Speaking']);
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
        }
      }
    };

    window.addEventListener('keydown', handleKeyPress);

    return () => {
      window.removeEventListener('keydown', handleKeyPress);
    };
  }, [addNextCards, deck]);

  // Check if we should show the SetDisplayName prompt
  const shouldShowSetDisplayName =
    reviewInfo.due_count === 0 &&
    !currentChallenge &&
    totalReviewsCompleted >= 25n &&
    userInfo?.displayName === null &&
    network.online === true &&
    !dismissedSetDisplayName &&
    accessToken !== undefined;

  const shouldShowPlacementTest = startingFresh === false && !deck.has_taken_placement_test() && deck.num_cards() < 3;

  return (
    <>
      {/* main content */}
      <div className="flex flex-col flex-1 gap-2">
        {shouldShowPlacementTest ? (
          <PlacementTest
            deck={deck}
            targetLanguage={targetLanguage}
            onComplete={({ knownWords, unknownWords }) => {
              const event = deck.complete_placement_test(knownWords, unknownWords);
              weapon.add_deck_event(event);
            }}
          />
        ) : shouldShowSetDisplayName ? (
          <SetDisplayName
            accessToken={accessToken!}
            totalReviewsCompleted={totalReviewsCompleted}
            onComplete={() => setDismissedSetDisplayName(true)}
            onSkip={() => {
              localStorage.setItem('yap-skipped-set-display-name', 'true')
              setDismissedSetDisplayName(true)
            }}
          />
        ) : accomplishment && !dismissedAccomplishment ? (
          <AccomplishmentScreen
            accomplishment={accomplishment}
            dailyReviewTarget={deck.get_daily_review_target_setting()}
            onChangeDailyReviewTarget={(target: DailyReviewTarget) => {
              const event = deck.set_daily_review_target(target)
              weapon.add_deck_event(event)
            }}
            onDismiss={() => setDismissedAccomplishment(true)}
          />
        ) : reviewInfo.due_count === 0 && !currentChallenge ? (
          <NoCardsReady
            nextDueCard={nextDueCard}
            addNextCards={addNextCards}
            showEngagementPrompts={reviewInfo.total_count > 5 && network.online === true && userInfo !== undefined}
            targetLanguage={targetLanguage}
            deck={deck}
            bannedChallengeTypes={bannedChallengeTypes}
            userInfo={userInfo}
            goal={goal}
            setGoal={setGoal}
            moviesWithMetadata={targetLanguageMovies}
            hasPimsleur={hasPimsleur}
          />
        ) : currentChallenge ? (
          (currentChallenge.type === 'PronunciationChallenge') ? (
            <PronunciationChallenge
              pattern={currentChallenge.pattern}
              guide={currentChallenge.guide}
              audioRequests={currentChallenge.audio_requests}
              onRating={handleRating}
              accessToken={accessToken}
              onCantSpeak={handleCantSpeak}
              targetLanguage={targetLanguage}
              isNew={currentChallenge.is_new}
              timesTypeSeen={currentChallenge.times_type_seen}
              key={totalReviewsCompleted}
            />
          ) : (currentChallenge.type === 'FlashCardReview') ? (
            <Flashcard
              audioRequest={currentChallenge.flashcard.audio}
              content={currentChallenge.flashcard.content}
              isNew={currentChallenge.is_new}
              totalCount={reviewInfo.total_count}
              onRating={handleRating}
              accessToken={accessToken}
              key={totalReviewsCompleted}
              onCantListen={handleCantListen}
              targetLanguage={targetLanguage}
              nativeLanguage={nativeLanguage}
              listeningPrefix={currentChallenge.flashcard.listening_prefix}
              autoplayed={autoplayed}
              setAutoplayed={setAutoplayed}
              timesTypeSeen={currentChallenge.times_type_seen}
            />
          ) : (currentChallenge.type === 'TranslateComprehensibleSentence') ? (
            <TranslationChallenge
              sentence={currentChallenge}
              onComplete={handleTranslationComplete}
              accessToken={accessToken}
              key={totalReviewsCompleted}
              targetLanguage={targetLanguage}
              nativeLanguage={nativeLanguage}
              autoplayed={autoplayed}
              setAutoplayed={setAutoplayed}
              deck={deck}
            />
          ) : (
            <TranscriptionChallenge
              challenge={currentChallenge}
              onComplete={handleTranscriptionComplete}
              totalCount={reviewInfo.total_count}
              accessToken={accessToken}
              key={totalReviewsCompleted}
              onCantListen={handleCantListen}
              targetLanguage={targetLanguage}
              nativeLanguage={nativeLanguage}
              autoplayed={autoplayed}
              setAutoplayed={setAutoplayed}
              deck={deck}
            />
          )
        ) : <div>Unexpected challenge state. This is a bug. currentChallenge: {JSON.stringify(currentChallenge)}</div>}
      </div>
      {/* /main content */}


    </>
  )
}

function AppShell() {
  return (
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <BackgroundShader>
        <ScrollRestoration />
        <Outlet />
        <Toaster />
      </BackgroundShader>
    </ThemeProvider>
  )
}

const router = createBrowserRouter([
  {
    element: <AppShell />,
    children: [
      { path: "/reset-password", element: <ResetPassword /> },
      { path: "/confirm-email", element: <ConfirmEmail /> },
      { path: "/accept-invite", element: <AcceptInvite /> },
      { path: "/forgot-password", element: <ForgotPassword /> },
      { path: "/about", element: <AboutPage /> },
      {
        path: "/*",
        element: <AppMain />,
        children: [
          { index: true, element: <LandingPage /> },
          { path: "learn", element: <ReviewPage /> },
          { path: "dictionary", element: <DictionaryPage /> },
          { path: "leeches", element: <LeechesPage /> },
          { path: "goals", element: <GoalsPage /> },
          { path: "select-language", element: <SelectLanguagePage /> },
          { path: "user/id/:id", element: <UserProfilePage /> },
          { path: "*", element: <NotFoundPage /> },
        ],
      },
    ],
  },
])

function App() {
  return <RouterProvider router={router} />
}

function SelectLanguagePage() {
  const { userInfo } = useOutletContext<AppContextType>()
  const weapon = useWeapon()
  const deckSelection = useDeckSelection()
  const navigate = useNavigate()

  return match(deckSelection)
    .with({ type: "languageSelected" }, ({ targetLanguage, hasHeardAbout, onboardedLanguages }) => (
      <LanguageSelector

        currentTargetLanguage={targetLanguage}
        showResumeButton={true}
        onResume={() => navigate('/learn')}
        onLanguagesConfirmed={(native, target) => {
          weapon.add_deck_selection_event({ SelectBothLanguages: { native, target } })
          navigate('/learn')
        }}
        onOnboardingComplete={(selections, language) => {
          weapon.add_deck_selection_event({ SetOnboardingSelections: { selections, target_language: language } })
        }}
        hasHeardAbout={hasHeardAbout}
        onHeardAbout={(heard_about) => {
          weapon.add_deck_selection_event({ SetHeardAbout: { heard_about } })
        }}
        onboardedLanguages={onboardedLanguages}
        userInfo={userInfo}
        onBack={() => navigate('/learn')}
      />
    ))
    .with({ type: "noLanguageSelected" }, ({ hasHeardAbout, onboardedLanguages }) => (
      <LanguageSelector

        onLanguagesConfirmed={(native, target) => {
          weapon.add_deck_selection_event({ SelectBothLanguages: { native, target } })
          navigate('/learn')
        }}
        onOnboardingComplete={(selections, language) => {
          weapon.add_deck_selection_event({ SetOnboardingSelections: { selections, target_language: language } })
        }}
        hasHeardAbout={hasHeardAbout}
        onHeardAbout={(heard_about) => {
          weapon.add_deck_selection_event({ SetHeardAbout: { heard_about } })
        }}
        onboardedLanguages={onboardedLanguages}
        userInfo={userInfo}
      />
    ))
    .with(null, () => (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: 'Yap.Town', onBack: () => navigate('/') }
        }}
      >
        <div className="flex-1 flex items-center justify-center">
          <p className="text-muted-foreground animate-fade-in-delayed">Loading...</p>
        </div>
      </TopPageLayout>
    ))
    .exhaustive()
}


export function useDeckSelection():
  | { type: "languageSelected", nativeLanguage: Language, targetLanguage: Language, startingFresh: boolean | undefined, hasHeardAbout: boolean, onboardedLanguages: Language[] }
  | { type: "noLanguageSelected", hasHeardAbout: boolean, onboardedLanguages: Language[] }
  | null {
  const weapon = useWeapon()

  useEffect(() => {
    weapon.request_deck_selection()
  }, [weapon])

  const getSnapshot = useCallback(() => {
    try {
      return weapon.get_stream_num_events("deck_selection") ?? null
    } catch {
      return null
    }
  }, [weapon])

  const subscribe = useCallback((callback: () => void) => {
    const handle = weapon.subscribe_to_stream("deck_selection", () => { callback() })
    return () => { weapon.unsubscribe(handle) }
  }, [weapon])

  const numEvents = useSyncExternalStore(subscribe, getSnapshot)

  if (numEvents === null) return null

  const deckSelection = weapon.get_deck_selection_state()
  const hasHeardAbout = deckSelection?.heardAbout != null;
  const onboardedLanguages = deckSelection?.onboardedLanguages ?? [];

  if (!deckSelection?.targetLanguage || !deckSelection?.nativeLanguage) {
    return { type: "noLanguageSelected", hasHeardAbout, onboardedLanguages }
  }

  return {
    type: "languageSelected",
    nativeLanguage: deckSelection.nativeLanguage,
    targetLanguage: deckSelection.targetLanguage,
    startingFresh: deckSelection.onboardingSelections?.startingFresh,
    hasHeardAbout,
    onboardedLanguages,
  }
}

const LAST_COURSE_KEY = "yap-last-course"

export function useDeck(): { type: "deck", nativeLanguage: Language, targetLanguage: Language, deck: Deck | null, startingFresh: boolean | undefined } | { type: "noLanguageSelected" } | { type: "error", message: string, retry: () => void, retryCount: number } | { type: "loading", message: string, progress: number } | null {
  const weapon = useWeapon()
  const [retryCount, setRetryCount] = useState(0)
  const [loadingState, setLoadingState] = useState<{ message: string, progress: number } | null>(null)

  useEffect(() => {
    weapon.request_deck_selection()
    weapon.request_reviews()
  }, [weapon])

  const getSnapshot = useCallback(() => {
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
  }, [weapon])

  const subscribe = useCallback((callback: () => void) => {
    const handle_reviews = weapon.subscribe_to_stream("reviews", () => { callback() })
    const handle_deck_selection = weapon.subscribe_to_stream("deck_selection", () => { callback() })

    return () => {
      weapon.unsubscribe(handle_reviews)
      weapon.unsubscribe(handle_deck_selection)
    }
  }, [weapon])

  const numEvents = useSyncExternalStore(subscribe, getSnapshot)

  const retry = useCallback(() => {
    setRetryCount(count => count + 1)
  }, [])

  // Determine course: from weapon streams if ready, else from localStorage cache
  const deck_selection = weapon.get_deck_selection_state()
  const courseParts = numEvents !== null && deck_selection?.targetLanguage && deck_selection?.nativeLanguage
    ? { nativeLanguage: deck_selection.nativeLanguage, targetLanguage: deck_selection.targetLanguage }
    : null
  if (courseParts) {
    localStorage.setItem(LAST_COURSE_KEY, JSON.stringify(courseParts))
  }
  const cachedCourse = useMemo<Course | null>(() => {
    try {
      const cached = localStorage.getItem(LAST_COURSE_KEY)
      if (!cached) return null
      const parsed = JSON.parse(cached)
      if (parsed.nativeLanguage && parsed.targetLanguage) return parsed
    } catch { /* ignore */ }
    return null
  }, [])
  const course = courseParts ?? cachedCourse

  // Fetch language pack — only re-runs when course changes, not when numEvents changes
  const languagePackResult = useAsyncMemo(async () => {
    if (!course) return null
    Sentry.addBreadcrumb({
      category: "language-pack",
      message: `Loading language pack: ${course.targetLanguage} → ${course.nativeLanguage}`,
      level: "info",
    })
    try {
      await weapon.get_language_pack(course, (message: string, progress: number) => {
        Sentry.addBreadcrumb({
          category: "language-pack",
          message: `${message} (${Math.round(progress)}%)`,
          level: "info",
        })
        setLoadingState({ message, progress })
      })
      setLoadingState(null)
      return { ok: true as const }
    } catch (error) {
      setLoadingState(null)
      return { ok: false as const, error }
    }
  }, [weapon, course?.targetLanguage, course?.nativeLanguage, retryCount])

  // Build deck — re-runs when language pack is ready or streams change
  const state = useAsyncMemo(async () => {
    if (numEvents === null) return null

    if (!deck_selection?.targetLanguage || !deck_selection?.nativeLanguage) {
      return { type: "noLanguageSelected" } as { type: "noLanguageSelected" }
    }

    if (!course || !languagePackResult) return null

    if (!languagePackResult.ok) {
      const error = languagePackResult.error
      console.error("Failed to fetch language pack:", error)
      const errorMessage = error instanceof Error ? error.message : String(error)
      const isNetworkError = errorMessage.startsWith("Network error:")
      if (!isNetworkError) {
        // Only report non-network errors to Sentry. Network failures are expected
        // on flaky mobile connections and the user already sees a retry UI.
        Sentry.captureException(
          error instanceof Error ? error : new Error(errorMessage),
          {
            tags: {
              "language-pack.target": course.targetLanguage,
              "language-pack.native": course.nativeLanguage,
            },
            contexts: {
              "language-pack": {
                targetLanguage: course.targetLanguage,
                nativeLanguage: course.nativeLanguage,
                rawError: errorMessage,
              },
            },
          }
        )
      }
      return {
        type: "error",
        message: errorMessage,
        retry,
        retryCount,
      } as { type: "error", message: string, retry: () => void, retryCount: number }
    }

    return {
      type: "deck",
      startingFresh: deck_selection.onboardingSelections?.startingFresh,
      nativeLanguage: course.nativeLanguage,
      targetLanguage: course.targetLanguage,
      deck: await weapon.get_deck_state(course, new Date().getTimezoneOffset() * -60),
    } as { type: "deck", nativeLanguage: Language, targetLanguage: Language, deck: Deck | null, startingFresh: boolean | undefined }
  }, [weapon, numEvents, languagePackResult, retryCount])

  if (state?.type === "error" && state.retryCount < retryCount) {
    return null
  }

  // If we're loading and have progress info, return loading state
  if (loadingState && (state === null || state === undefined)) {
    return { type: "loading", message: loadingState.message, progress: loadingState.progress }
  }

  return state ?? null
}

export default App
