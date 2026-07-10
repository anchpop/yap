// No-op stand-in for yap-frontend's sound effects (howler + mp3s served from
// the app's public/). The widget lives inside a chat host; grading feedback
// stays visual. Same exports so reused components compile unchanged.
type SoundType = "perfect" | "success" | "fail" | "aiDoneGrading";

export const playSoundEffect = (_type: SoundType): Promise<void> =>
  Promise.resolve();

export const isSoundEffectPlaying = (): boolean => false;

export const stopCurrentSoundEffect = (): void => {};
