import { Howl } from "howler";

type SoundType = "perfect" | "success" | "fail" | "aiDoneGrading";

// Lazy-initialized sound instances. Created on first use to avoid
// initializing AudioContext at module import time, which throws
// "The provided value is non-finite" on iOS before user interaction.
const soundConfigs: Record<SoundType, { src: string[]; volume: number }> = {
  perfect: { src: ["/success-1.mp3"], volume: 0.5 },
  success: { src: ["/success-2.mp3"], volume: 0.5 },
  fail: { src: ["/success-3.mp3"], volume: 0.5 },
  aiDoneGrading: { src: ["/ai-done-grading.mp3"], volume: 0.5 },
};

const sounds: Partial<Record<SoundType, Howl>> = {};

function getSound(type: SoundType): Howl {
  let sound = sounds[type];
  if (!sound) {
    sound = new Howl({ ...soundConfigs[type], preload: true });
    sounds[type] = sound;
  }
  return sound;
}

let currentSoundEffect: Howl | null = null;

export const playSoundEffect = (type: SoundType): Promise<void> => {
  return new Promise((resolve) => {
    const sound = getSound(type);

    if (currentSoundEffect && currentSoundEffect.playing()) {
      currentSoundEffect.stop();
    }

    currentSoundEffect = sound;

    const soundId = sound.play();

    sound.once(
      "end",
      () => {
        currentSoundEffect = null;
        resolve();
      },
      soundId,
    );

    sound.once(
      "loaderror",
      () => {
        currentSoundEffect = null;
        console.error(`Failed to load ${type} sound`);
        resolve();
      },
      soundId,
    );

    sound.once(
      "playerror",
      () => {
        currentSoundEffect = null;
        console.error(`Failed to play ${type} sound`);
        resolve();
      },
      soundId,
    );
  });
};

export const isSoundEffectPlaying = (): boolean => {
  return currentSoundEffect !== null && currentSoundEffect.playing();
};
