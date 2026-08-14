// Single owner of every WebAuthn ceremony in the app.
//
// The browser allows only one WebAuthn ceremony at a time, and auth-js's
// internal abort service can't cancel a ceremony we started ourselves — so
// every passkey call (conditional autofill, modal sign-in, registration)
// must route through this module, which serializes them against one
// module-scoped controller.
//
// Sign-in uses Supabase's two-step API (passkey.startAuthentication /
// verifyAuthentication) instead of the one-shot signInWithPasskey() because
// the one-shot accepts no `mediation` option, and conditional UI (the
// passkey appearing in the email field's autofill dropdown) requires
// `mediation: "conditional"` on navigator.credentials.get().

import { supabase } from "@/lib/supabase";
import type { Session } from "@supabase/supabase-js";

// The one live ceremony, if any. Module scope, not React state: "one
// ceremony per browser" is a global invariant, and StrictMode double-mounts
// must observe each other.
let current: {
  controller: AbortController;
  promise: Promise<Session | null>;
  mediation: "conditional" | "required";
} | null = null;

/** Gates the modal "Sign in with a passkey" button. Deliberately coarse —
 * the modal path goes through auth-js, which has its own manual decode
 * fallback for browsers without the WebAuthn Level 3 JSON helpers. */
export function browserSupportsWebAuthn(): boolean {
  return (
    typeof window !== "undefined" &&
    "PublicKeyCredential" in window &&
    typeof navigator.credentials?.get === "function"
  );
}

let canCreatePromise: Promise<boolean> | undefined;
/** Gates passkey *creation* UI: settings and the two creation prompts. */
export function canCreatePasskey(): Promise<boolean> {
  canCreatePromise ??= (async () => {
    if (!browserSupportsWebAuthn()) return false;
    try {
      return await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable();
    } catch {
      return false;
    }
  })();
  return canCreatePromise;
}

let canAutofillPromise: Promise<boolean> | undefined;
/** Gates the conditional-UI (autofill) path only. Stricter than the modal
 * gate: we drive navigator.credentials.get() ourselves, so we need the
 * Level 3 JSON helpers (parseRequestOptionsFromJSON for the outbound leg,
 * toJSON for the return leg) — Safari 16.x–17.3 has conditional mediation
 * but not these, and there the modal button still works while autofill
 * must stay off. */
export function canAutofillPasskey(): Promise<boolean> {
  canAutofillPromise ??= (async () => {
    if (
      !browserSupportsWebAuthn() ||
      typeof PublicKeyCredential.isConditionalMediationAvailable !==
        "function" ||
      typeof PublicKeyCredential.parseRequestOptionsFromJSON !== "function" ||
      typeof PublicKeyCredential.prototype.toJSON !== "function"
    ) {
      return false;
    }
    try {
      return await PublicKeyCredential.isConditionalMediationAvailable();
    } catch {
      return false;
    }
  })();
  return canAutofillPromise;
}

/** True for "the user closed the sheet / we aborted", which callers should
 * swallow silently. Duck-typed: auth-js's WebAuthnError isn't exported
 * from the supabase-js bundle, so we match on the stable name/code fields
 * of both raw DOMExceptions and auth-js's wrapper. */
export function isCancelled(err: unknown): boolean {
  if (typeof err !== "object" || err === null) return false;
  const e = err as { name?: unknown; code?: unknown };
  return (
    e.name === "NotAllowedError" ||
    e.name === "AbortError" ||
    e.code === "ERROR_CEREMONY_ABORTED"
  );
}

/** Thrown/returned failures that aren't cancellations. */
export class PasskeyError extends Error {}

function abortCurrent(): Promise<unknown> {
  if (!current) return Promise.resolve();
  current.controller.abort();
  // Wait for the aborted ceremony to actually settle — starting a new
  // credentials.get() while the old one is still winding down throws
  // "NotAllowedError: a request is already pending".
  return current.promise.catch(() => {});
}

function clearIfMine(controller: AbortController) {
  if (current?.controller === controller) {
    current = null;
  }
}

/** Abort any live ceremony (e.g. when the auth dialog closes). */
export function disarm(): void {
  void abortCurrent();
}

async function verifyAssertion(
  challengeId: string,
  credential: PublicKeyCredential,
): Promise<Session | null> {
  const { data, error } = await supabase.auth.passkey.verifyAuthentication({
    challengeId,
    // Cast: the DOM lib types toJSON() loosely as a union of registration
    // and authentication shapes; at runtime a get() credential serializes
    // to the authentication shape verifyAuthentication expects.
    credential: credential.toJSON() as Parameters<
      typeof supabase.auth.passkey.verifyAuthentication
    >[0]["credential"],
  });
  if (error) throw error;
  return data.session;
}

/**
 * Arm a conditional (autofill) passkey request. Resolves with a session
 * when the user picks a passkey from the browser's autofill UI, or null if
 * the request was aborted/superseded. Idempotent while a conditional
 * request is live: repeat calls return the existing promise instead of
 * racing a second ceremony into "a request is already pending". If the
 * previous ceremony was aborted (StrictMode mount→cleanup→mount), we wait
 * for it to settle and start fresh.
 */
export async function armConditional(): Promise<Session | null> {
  if (
    current?.mediation === "conditional" &&
    !current.controller.signal.aborted
  ) {
    return current.promise;
  }
  await abortCurrent();
  // Another caller may have armed while we awaited the old ceremony.
  if (
    current?.mediation === "conditional" &&
    !current.controller.signal.aborted
  ) {
    return current.promise;
  }

  const controller = new AbortController();
  const promise = (async (): Promise<Session | null> => {
    // Never arm over an existing session (the dialog can be open while
    // signed in only transiently, but the guard is cheap).
    const { data } = await supabase.auth.getSession();
    if (data.session || controller.signal.aborted) return null;

    const { data: options, error } =
      await supabase.auth.passkey.startAuthentication();
    if (error || !options) {
      // Includes over_request_rate_limit: the user never asked for
      // autofill, so a failure to arm it is always silent.
      return null;
    }
    // The abort may have landed during the await above; entering
    // credentials.get() anyway would start a real ceremony on some
    // browsers despite the pre-aborted signal.
    if (controller.signal.aborted) return null;

    // Challenge lifetime, computed as a duration so client clock skew
    // can't produce "already expired" or "expires in 3 hours".
    // expires_at is a bare number; normalize seconds vs milliseconds.
    const expiresAtMs =
      options.expires_at > 1e12
        ? options.expires_at
        : options.expires_at * 1000;
    const ttlMs = Math.min(
      Math.max(expiresAtMs - Date.now(), 60_000),
      15 * 60_000,
    );
    const armedAt = Date.now();

    let credential: Credential | null;
    try {
      credential = await navigator.credentials.get({
        publicKey: PublicKeyCredential.parseRequestOptionsFromJSON(
          options.options,
        ),
        mediation: "conditional",
        signal: controller.signal,
      });
    } catch (err) {
      if (isCancelled(err)) return null;
      throw err;
    }
    if (!(credential instanceof PublicKeyCredential)) return null;

    // The user picked a passkey — from here on we owe them a session even
    // if this challenge died while the request sat pending. A stale
    // assertion can never be re-verified (it's signed over the dead
    // challenge), but the user just demonstrated intent, so escalate to
    // one fresh modal ceremony instead of surfacing an error.
    if (Date.now() - armedAt > ttlMs - 5_000) {
      clearIfMine(controller);
      return signInWithPasskeyModal();
    }
    try {
      return await verifyAssertion(options.challenge_id, credential);
    } catch {
      clearIfMine(controller);
      return signInWithPasskeyModal();
    }
  })();

  const tracked = promise.finally(() => clearIfMine(controller));
  // Avoid unhandled-rejection noise from the tracking wrapper when the
  // caller only consumes the returned promise.
  tracked.catch(() => {});
  current = { controller, promise: tracked, mediation: "conditional" };
  return tracked;
}

/**
 * Modal ("tap the button") passkey sign-in. Aborts any conditional request
 * first and waits for it to settle, then runs a fresh ceremony. Throws on
 * real failures, including cancellations (filter with isCancelled).
 */
export async function signInWithPasskeyModal(): Promise<Session | null> {
  await abortCurrent();

  if (!canParseJson()) {
    // Browsers with WebAuthn but without the Level 3 JSON helpers
    // (Safari 16.x–17.3): delegate to auth-js's one-shot, which carries
    // its own manual base64url decode fallback. Its internal abort
    // service manages that ceremony; `current` stays null.
    const { data, error } = await supabase.auth.signInWithPasskey();
    if (error) throw error;
    return data.session;
  }

  const controller = new AbortController();
  const promise = (async (): Promise<Session | null> => {
    const { data: options, error } =
      await supabase.auth.passkey.startAuthentication();
    if (error) throw error;
    if (controller.signal.aborted) return null;

    const credential = await navigator.credentials.get({
      publicKey: PublicKeyCredential.parseRequestOptionsFromJSON(
        options.options,
      ),
      signal: controller.signal,
    });
    if (!(credential instanceof PublicKeyCredential)) {
      throw new PasskeyError("No credential returned");
    }
    return verifyAssertion(options.challenge_id, credential);
  })();

  const tracked = promise.finally(() => clearIfMine(controller));
  tracked.catch(() => {});
  current = { controller, promise: tracked, mediation: "required" };
  return tracked;
}

function canParseJson(): boolean {
  return (
    typeof PublicKeyCredential !== "undefined" &&
    typeof PublicKeyCredential.parseRequestOptionsFromJSON === "function" &&
    typeof PublicKeyCredential.prototype.toJSON === "function"
  );
}

/**
 * Register a passkey for the signed-in user. Aborts any conditional
 * request first (auth-js's registerPasskey can't see our controller, so we
 * must clear the field ourselves before handing off).
 */
export async function registerPasskeyCeremony(): Promise<void> {
  await abortCurrent();
  const { error } = await supabase.auth.registerPasskey();
  if (error) throw error;
}
