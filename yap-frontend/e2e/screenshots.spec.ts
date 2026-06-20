/* eslint-disable no-console -- this harness logs its progress to the console by design */
import { test, expect, type Page } from "@playwright/test";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// End-to-end screenshot capture of the sentence-list UI (the screens that the
// "goal" -> "sentence list" rename touched). Drives the REAL app with the REAL
// WASM core; only Supabase auth is faked via the app's cached-login path
// (localStorage "yap-user-info" — set LOGGED_OUT=1 to capture the anonymous
// view instead). Seeds a French deck with a single deck-selection event (what
// the language picker emits), captures the /sentence-lists picker, then drives
// a full review session until the NoCardsReady carousel renders.

const OUT_DIR = path.join(__dirname, "..", "screenshots-out");

// Fake a logged-in-but-offline user so routes render without a Supabase
// session, and skip the OPFS capability probe. Runs before any app code.
async function fakeLogin(page: Page) {
  const loggedOut = process.env.LOGGED_OUT === "1";
  await page.addInitScript((loggedOut: boolean) => {
    if (!loggedOut) {
      localStorage.setItem(
        "yap-user-info",
        JSON.stringify({
          id: "e2e-screenshot-user",
          email: "screenshots@example.com",
          displayName: "Screenshot Bot",
        }),
      );
    }
    localStorage.setItem("opfs-test-passed", "true");
    localStorage.setItem("yap-pimsleur-acknowledged", "true");
  }, loggedOut);
}

// Wait for the dev-only window.__weapon hook, then seed French (target) /
// English (native). Mirrors LanguageSelector's onLanguagesConfirmed.
async function seedFrenchDeck(page: Page) {
  await page.waitForFunction(() => "__weapon" in window, null, {
    timeout: 30_000,
  });
  await page.evaluate(() => {
    const w = (
      window as unknown as {
        __weapon: { add_deck_selection_event: (event: unknown) => void };
      }
    ).__weapon;
    w.add_deck_selection_event({
      SelectBothLanguages: { native: "English", target: "French" },
    });
  });
}

test("capture sentence-list screens", async ({ page }) => {
  page.on("console", (m) => console.log(`  [browser:${m.type()}] ${m.text()}`));

  await fakeLogin(page);
  await page.goto("/"); // boot the weapon on a stable page
  await seedFrenchDeck(page);
  await page.waitForTimeout(4000); // let the deck-selection event flush to OPFS

  // Wait for the deck to finish loading (language pack download/deserialize
  // takes ~8s on a fresh context) — i.e. the "Loading..." placeholder is gone.
  // NB: don't wait for "networkidle" — the app keeps persistent connections
  // (Supabase realtime, periodic sync) open, so it never goes idle. Instead
  // wait for POSITIVE end-state content to appear (waiting for the absence of
  // a transient loading string is unreliable — it's briefly true before the
  // loading UI even mounts). `re` should match text only the loaded screen has.
  const settled = async (re: RegExp) => {
    await page
      .waitForFunction(
        (src: string) => new RegExp(src, "i").test(document.body.innerText),
        re.source,
        { timeout: 60_000 },
      )
      .catch(() => {});
    await page.waitForTimeout(2500); // let posters/images paint
  };

  // /sentence-lists -> the picker page (primary renamed surface)
  await page.goto("/sentence-lists");
  await settled(/pick a sentence list to focus/i);
  console.log("  /sentence-lists URL:", page.url());
  console.log("  /sentence-lists text:", (await page.locator("body").innerText()).slice(0, 300).replace(/\n+/g, " | "));
  await page.screenshot({ path: path.join(OUT_DIR, "sentence-lists.png") });
  // Fail loudly rather than ship a screenshot of a loading/wrong page.
  await expect(page.getByText(/pick a sentence list to focus/i)).toBeVisible();

  // /learn -> drive a full session until NoCardsReady (the non-empty "you're
  // doing great / change sentence list" carousel) renders. Start from the
  // "Ready to start learning" empty state, add cards, answer them all forward
  // (which schedules them out of the due window) until due_count hits 0.
  await page.goto("/learn");
  if (new URL(page.url()).pathname === "/") {
    await page.waitForTimeout(3000);
    await page.goto("/learn");
  }
  await settled(/ready to start learning|change sentence list|reviewing \d|show english/i);

  // The FULL NoCardsReady page (carousel + week day cells) only renders when
  // nextDueCard !== null — i.e. a card is scheduled for a FUTURE review but
  // nothing is due now. These headlines belong to that `else` branch only.
  const CAROUSEL =
    /all caught up|you're doing great|keep up the momentum|soon you'll hit|you're all done with|change sentence list/i;
  const isDone = async () => CAROUSEL.test(await page.locator("body").innerText());

  const visibleClick = async (locator: ReturnType<Page["getByRole"]>) => {
    if ((await locator.count()) > 0 && (await locator.first().isVisible())) {
      await locator.first().click().catch(() => {});
      return true;
    }
    return false;
  };

  let done = false;
  let started = false; // only add the first batch once; never top up
  for (let i = 0; i < 30 && !done; i++) {
    if (await isDone()) {
      done = true;
      break;
    }

    const body = (await page.locator("body").innerText()).replace(/\n+/g, " ");
    console.log(`  [step ${i}] ${body.slice(0, 90)}`);

    // 1) Empty state -> add the first (and only) batch of cards.
    if (!started) {
      if (await visibleClick(page.getByRole("button", { name: /start learning/i }))) {
        started = true;
        await page.waitForTimeout(700);
        continue;
      }
    }
    // 2) Placement test (defensive) -> advance / finish.
    if (await visibleClick(page.getByRole("button", { name: /begin learning|continue anyway/i }))) {
      await page.waitForTimeout(700);
      continue;
    }
    // 3) Translation / Transcription -> type, check, continue.
    const textarea = page.locator("textarea");
    if ((await textarea.count()) > 0 && (await textarea.first().isVisible())) {
      await textarea.first().fill("je ne sais pas");
      await visibleClick(page.getByRole("button", { name: /check answer/i }));
      const cont = page.getByRole("button", { name: /nailed it|continue/i });
      await cont.first().waitFor({ timeout: 20_000 }).catch(() => {});
      await visibleClick(cont);
      await page.waitForTimeout(500);
      continue;
    }
    // 4) Rate "Didn't know" / "Again" (left). This pushes the card onto a short
    //    learning step (~1 min) -> a future-scheduled card, which is what makes
    //    the full carousel render instead of "Ready for more?".
    if (await visibleClick(page.getByRole("button", { name: /didn't know|forgot|again/i }))) {
      await page.waitForTimeout(500);
      continue;
    }
    // 5) Flashcard front -> reveal (Space) then rate "Again" (ArrowLeft).
    await page.keyboard.press("Space");
    await page.waitForTimeout(350);
    await page.keyboard.press("ArrowLeft");
    await page.waitForTimeout(500);
  }

  console.log("  /learn done:", done, "URL:", page.url());
  console.log("  /learn text:", (await page.locator("body").innerText()).slice(0, 300).replace(/\n+/g, " | "));
  await page.screenshot({ path: path.join(OUT_DIR, "no-cards-ready.png") });
  // Fail loudly if the session never drained to the NoCardsReady carousel
  // (e.g. a card type we don't handle, or the deck never seeded).
  expect(done, "never reached the NoCardsReady carousel").toBe(true);
});
