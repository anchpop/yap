# Forecast: your past pace, simulated forward to every goal

**Status:** Draft spec v2 (reworked after Andre's feedback). No new serialized events — everything here is derived state.

**Shipped v1 (this session):** a "🔮 Simulate" tool in the Tools section (`/simulate`). `Deck::start_simulation(timestamp_ms)` returns a `DeckSimulation` wasm object; the frontend calls `simulate_days(14)` in two-week chunks, yielding to the main thread between chunks, and renders the samples incrementally as a recharts line chart (goal % + overall %, reference line at 95%, early-stop when the goal is reached, stop button, unmount cancellation). Uses the stock simulator policy (clear queue daily, ~10 new cards/day, new cards failed once) — labeled in the UI as a best-case pace. Also fixed a latent simulator bug found during verification: `finish_day` passed `None` as the sentence list to `get_no_cards_ready_info`, and replaying the resulting `AddCards { sentence_list: None }` event reset the simulated deck's list after one day — simulations (including the production audio pre-cache one) drew new cards from the global list instead of the user's selected movie/lesson. It now uses `deck.get_sentence_list()`. Measured cost of a 365-day simulation on a real user deck: ~5s native release; ~10-20s in wasm, ~150-400ms per two-week chunk. Calibration (§3) and the rest of this spec remain future work.

## 1. The pitch

Yap tells you *where you are* (percent known per movie, "42 cards to 95%") but never *when you'll arrive*. This feature draws one chart: your actual progress over the past weeks as a solid line, and a **calibrated simulation of the next year** as a dashed line — with your goals (watch Amélie, finish Pimsleur II, hit 50% overall) as markers on that line.

> **"Amélie — watchable around August 14. At 15 new cards/day instead of 10, July 29."**

Why this is the right feature for Yap specifically:

1. **The simulator already exists and is production-proven.** `DailySimulationIterator` (`simulation.rs`) drives real `Challenge`s through `get_review_info` and answers them by constructing real events through `process_event`/`finalize` — it already powers audio pre-caching (`cache_challenge_audio`, `lib.rs` ~2444), complete with a yield-to-main-thread + `AbortSignal` pattern. There is zero model drift from production behavior: the forecast adds cards with the same `smart_add` logic, schedules with the same FSRS, grades with the same state machine.
2. **One run forecasts everything.** Per-goal coverage is a pure function of the simulated known-sets (`percent_known_in(goal_freq_list, known_written, known_listening)`). Snapshot the known-sets weekly during a single simulation run and you get the trajectory for *every* movie, every Pimsleur lesson, and overall percent-known simultaneously. The movie grid's watch dates, the Pimsleur ETAs, and the forecast chart are all views of one cached artifact.
3. **It gives pace a purpose.** "Add 5 more cards a day and watch the movie 16 days sooner" is the most honest motivation lever in language learning, and it lives exactly where the decision is made: the add-new-cards page.

## 2. Definitions

- **Watchable threshold:** 95% token coverage (matches the existing milestone ceiling); named constant.
- **Coverage** (existing semantics, unchanged): token-weighted; 1.0 × count if a gram is known in both modalities, 0.5 if one; "known" = FSRS `Review` state or regression-predicted ≥ 80% for unadded grams.
- **Pace metrics** (per Andre): **new cards/day** (what the user chooses) and **freshly-learned cards/day** (cards newly graduated to `Review`; what moves coverage). New-cards/day is the simulator's input knob; freshly-learned/day is a display stat and a calibration check.

## 3. Calibrating the simulator to the user

The simulator's current policy is a fixed persona: 10 new cards/day, new cards rated `Again` once then `Remembered`, sentence challenges answered perfectly. Calibration replaces that persona with *this user's* recent behavior, measured from the event log.

### 3.1 Measured inputs (from `daily_history`, §3.3)

Over a trailing window (28 days, active days only, medians for robustness):

| Parameter | Measurement | Used for |
|---|---|---|
| `new_cards_per_day` | median adds/day on active days × (active days ÷ 28) | `finish_day` add count (replaces the hardcoded 10) |
| `challenges_per_day` | median challenges answered/day, same activity scaling | per-day cap on the challenge iterator (carryover stays due, like real life) |
| `forget_calibration_k` | observed forget rate ÷ expected forget rate (§3.2) | rating policy |

Scaling by the activity ratio bakes rest days into the pace instead of pretending the user studies daily — otherwise every date we show is a promise we know they'll miss.

### 3.2 Forgetting "under expectation"

Raw %-forgotten is confounded by deck maturity (a backlog of overdue cards inflates it; a young deck deflates it). The principled version: at each historical review, FSRS knows the card's predicted retrievability `R`; the *expected* forget rate over a window is `mean(1 − R)` at the user's actual reviews. Define:

```
k = observed_forget_rate / expected_forget_rate      (clamped to, say, [0.5, 2.0])
```

`k ≈ 1` means the user forgets exactly as FSRS predicts; `k > 1` means they underperform the model. In simulation, each review forgets with probability `k × (1 − R)` — preserving FSRS structure (young/hard cards forgotten more often) while matching the user's overall bias.

**Determinism without RNG:** accumulate `k × (1 − R)` per card-review into a running total; emit `Again` whenever the total crosses an integer (low-discrepancy / error-diffusion style). Deterministic for tests and for cache stability, and converges to the right rate.

**Sentence challenges** (translation/transcription) stay answered-perfectly in v1; `k` is measured on flashcard reviews and applied to flashcard reviews, so the calibration is self-consistent. Extending the forget model to per-word sentence grading is a listed refinement, not v1.

### 3.3 `daily_history` — the past half of the chart

The deck is already reconstructed by event replay at startup; accumulate a `daily_history: BTreeMap<LocalDate, DayAggregate>` on `Deck` during that replay:

```rust
pub struct DayAggregate {
    pub new_cards: u32,          // added
    pub freshly_learned: u32,    // graduated to Review that day
    pub challenges: u32,
    pub seconds_spent: u32,
    pub remembered: u32,
    pub forgot: u32,
    pub expected_forgot: f64,    // sum of (1 - R) at review time, for §3.2
    pub percent_known: f64,      // overall coverage at end of day
}
```

Derived state only — nothing new serialized, no `deck_event` compatibility concerns. Buckets by local day using recorded event timezones (carried since 92bd67e). This is both the calibration source and the solid "past" line on the chart, and incidentally frees the stats page from its current 7-day memory (`get_current_week_progress`).

Per-goal *past* trajectories (e.g. Amélie's coverage over the last month) are **not** accumulated for all goals during replay — that would walk every frequency list every day. Overall `percent_known` is cheap enough to keep per day; a per-goal history, if we ever want it, can be replayed lazily for one goal on demand.

## 4. The forecast run

One background simulation, cached, feeding every surface.

- **Setup:** clone the current deck; calibrated parameters from §3; sentence list = the user's *actual current* sentence list (so `smart_add` behaves exactly as production will).
- **Per simulated day:** answer challenges via the calibrated rating policy, capped at `challenges_per_day` (remainder carries over as genuinely due — that's what real users experience); `finish_day` adds `new_cards_per_day` cards.
- **Weekly snapshot:** the two comprehensible-gram sets (written/listening). From each snapshot, compute coverage against: every movie's frequency list, every Pimsleur lesson's, and the global list. Store as `Vec<WeeklySample { day_offset, per_goal_percent, overall_percent }>`. (Compute coverage at snapshot time and store the numbers, not the sets — a few hundred goals × 52 weeks of f64s is tiny.)
- **Horizon:** 365 days, with **early emission** — samples stream out as computed (§6), so the UI has the 3-month picture long before the year finishes.
- **Watch dates:** first weekly sample where a goal crosses its threshold, linearly interpolated between samples. Displayed with fuzzy granularity (§5.4).

### 4.1 Scenarios

The baseline run uses measured `new_cards_per_day`. Scenario runs (e.g. −5 / +5 / +10 cards/day) are identical simulations with that one knob changed — **computed on demand only** (user taps a scenario chip), never eagerly, since each costs a full run. Scenario results cache alongside the baseline.

"If you focus on this movie" (simulating with a different sentence list than the current one) is the same mechanism — an on-demand run with the sentence-list knob changed — and is a Phase-3 nicety, not core.

### 4.2 Invalidation & caching

The forecast is a pure function of (deck event count, calibration params, sentence list, horizon). Cache the result in memory keyed on that; recompute when the key changes, i.e. effectively once per study session — kicked off in the background after a session ends or on app load, exactly like `cache_challenge_audio` is today. Don't persist to OPFS in v1; a session-start recompute that streams progressively is fine, and persistence adds an invalidation surface we don't need yet.

## 5. Surfaces

### 5.1 The forecast chart — on the add-new-cards page (`no-cards-ready.tsx`)

Andre's call, and the right one: this page already shows the single-step projection (`percent_known_after`) and it's where the pace decision is made. The chart generalizes that projection:

- **Solid line:** past weeks of overall percent-known from `daily_history`.
- **Dashed line:** the simulated year (streams in progressively — the line literally draws itself as the run completes).
- **Goal markers:** current sentence-list goal prominent (e.g. Amélie @ 95% → flag at the crossing date); overall-% milestones as subtle gridline flags.
- **Scenario chips** under the chart: "10/day (now) · 15/day → Jul 29 · 20/day → Jul 21" — tapping runs that scenario and overlays a second dashed line. recharts (already a dependency) handles all of this; mirror `FrequencyKnowledgeChart`'s lazy-Suspense pattern.

### 5.2 Movie grid (`MoviePosterCard.tsx` / `Movies.tsx`)

- One added line per card from the cached baseline run: **"Watchable ~ Aug 14"** / "~ early Nov" / "Watchable now 🍿". Omit until the cached run reaches that movie's crossing (or shows it beyond horizon → "a year+ at this pace").
- New sort: **Soonest watchable** — turns the catalog into a roadmap; reads straight from the cache.
- Same treatment applies to Pimsleur lessons for free (they're just another `FrequencySourceId`).

### 5.3 Wasm API

```rust
#[wasm_bindgen]
pub fn get_daily_history(&self, days: u32) -> Vec<DayAggregateJs>;   // past line + stats page

#[wasm_bindgen]
pub fn get_calibration(&self) -> Calibration;
// { new_cards_per_day, challenges_per_day, freshly_learned_per_day,
//   forget_calibration_k, active_days_28, confidence }

#[wasm_bindgen]
pub async fn run_forecast(
    &self,
    overrides: Option<ForecastOverrides>,        // scenario knobs: new_cards_per_day, sentence_list
    on_progress: &js_sys::Function,              // streams WeeklySample batches to the UI
    abort_signal: Option<web_sys::AbortSignal>,  // same pattern as cache_challenge_audio
) -> ForecastResult;
// ForecastResult { samples: Vec<WeeklySample>, goal_dates: Vec<GoalDate { goal_id, date_ms: Option<f64> }> }
```

`get_movie_stats()` grows `watchable_forecast_ms: Option<f64>`, read from the cached baseline result if present (frontend passes it in, or the Deck holds the cache — implementation's choice; spec only requires the grid never *triggers* a run).

### 5.4 Fuzzy dates

False precision is the failure mode. Display granularity degrades with distance and confidence (confidence = active days in window: ≥14 high, 5–13 medium, <5 low → scenario framing only, using a shipped default persona):

| Distance | High | Medium/Low |
|---|---|---|
| < 3 weeks | "Aug 14" | "mid-August" |
| 3 wk – 3 mo | "mid-August" | "August" |
| 3 – 9 mo | "October" | "this fall" |
| > 9 mo | "a year+ at this pace" | scenario framing only |

### 5.5 Celebration

When a session pushes a goal across its threshold: **"🍿 Movie night unlocked"** section in `AccomplishmentScreen` (confetti infra exists). Detected by comparing `get_movie_stats` snapshots before/after the session — no new event.

## 6. Performance — the honest section

The cost driver: each simulated answer runs `apply_event` = full `Deck → DeckState → process_event → finalize`. A mature deck at ~100 challenges/day × 365 days ≈ 36k full pipeline invocations — plausibly tens of seconds to minutes in wasm. Mitigations, in order of importance:

1. **Run it like audio pre-caching.** Background, chunked with the existing yield pattern, abortable, started when the user isn't waiting on it (post-session / app idle). The UI never blocks on a forecast; surfaces show "forecasting…" until samples stream in.
2. **Progressive emission.** Weekly samples stream to the UI as computed. The near future (which users care most about, and which fuzzy display rewards) arrives in the first seconds; month 12 can take its time.
3. **Cache = compute once per session** (§4.2). Amortized over a session, even a multi-minute run is acceptable; it just can't be *per surface* or *per render*.
4. **Amortize `finalize`.** The structural win, needs `weapon::AppState` cooperation: process a whole simulated day's events into `DeckState` and finalize once per day instead of once per event. If `finalize` dominates (likely — it rebuilds derived state), this alone could be ~a 100× reduction on the pipeline's fixed costs. Worth a profiling spike *before* building the batching, to confirm where the time actually goes.
5. **Cheap snapshot math.** Weekly coverage sampling walks each goal's frequency list; a few hundred lists weekly is fine, but keep the known-sets incremental (mutate, don't rebuild) and store computed percentages, not sets.
6. **Escape hatch if it's still too slow:** drop simulated *challenge* fidelity for far horizon — full fidelity for 90 days, then switch to a coarse mode (skip sentence-challenge generation, flashcard events only). Fallback, not plan-of-record; it reintroduces model drift.

A profiling spike on a large real deck (via `/inspect-user` events) is the first implementation task — everything in this section is sized by that number.

## 7. Edge cases

| Case | Behavior |
|---|---|
| Goal already ≥ threshold | "Watchable now", no date |
| `all_available_learned` but < 95% | "Watchable now*" — Yap has no more content for it; never show an unreachable date |
| < 5 active days in window (new/lapsed user) | No "current pace" line; scenario framing with default persona, worded as estimate |
| Crossing beyond horizon | "a year+ at this pace" + scenario chips |
| User changes sentence list | Cache key changes → recompute; other goals' dates keep "at your current focus" semantics silently (the run *is* the current focus) |
| Deck changes mid-run | Abort + restart via `AbortSignal` (established pattern) |
| Simulation hits `smart_add` exhaustion (nothing left to add) | Curve plateaus honestly; goals past the plateau show "not reachable with current content" |

## 8. Testing & validation

- **Unit:** calibration math on synthetic `daily_history` fixtures (activity scaling, medians, `k` clamping); error-diffusion forget policy hits target rates deterministically; interpolated crossing dates monotone in `new_cards_per_day`.
- **Simulation:** extend the existing `test_simulate_365_days_test_data_deck` to calibrated policies; fixed inputs → identical curves (determinism test mirrors `test_simulator_is_deterministic`).
- **Backtest (the credibility check):** truncate a real user's event log at time T, calibrate on data before T, simulate 30/60 days, compare simulated vs. actual coverage at T+30/T+60. Run across a few real accounts via `/inspect-user` before shipping the "at your current pace" copy. This also tunes the default persona and the `k` clamp range.

## 9. Phasing

1. **Phase 0 — profiling spike.** Time a 365-day calibrated run on a heavy real deck; measure where it goes (`finalize`?). Sizes everything else.
2. **Phase 1 — history + calibration.** `daily_history` in replay, `get_daily_history`, `get_calibration`. Shippable alone as a stats-page trend line ("your last 8 weeks").
3. **Phase 2 — the run.** Calibrated policy in the simulator, weekly snapshots, `run_forecast` with streaming + abort, in-memory cache. Chart on the add-new-cards page with goal markers.
4. **Phase 3 — spread + loop.** Watch dates on movie grid + "soonest watchable" sort; scenario chips; celebration in `AccomplishmentScreen`; optional "if you focus on this movie" runs. Notifications ("your movie is ready") are outward-facing → separate review.

## 10. Open questions for Andre

1. **Challenge cap vs. exhaust-the-queue:** §3.1 caps simulated challenges/day at the measured median (backlog carries over). Alternative: assume the user always clears their queue (current simulator behavior), which is simpler and more flattering but less honest for goal-missing users. Preference?
2. **Where should the cache live** — Deck-owned (Rust holds `ForecastResult`, wasm getters read it) or frontend-owned (JS holds the streamed samples)? Deck-owned keeps `get_movie_stats` self-contained; frontend-owned keeps the Deck stateless-ish.
3. **Scenario knob units:** spec uses new-cards/day (5 → 20 range). Should scenario chips instead mirror the existing time-based daily goals (5/10/15/20 min), converted via measured seconds-per-challenge? Cards/day is more direct; minutes/day matches existing UI language.
4. Is the stats-page trend line (Phase 1 byproduct) worth shipping on its own first?
