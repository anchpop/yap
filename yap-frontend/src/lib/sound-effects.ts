import { Howl } from 'howler'

// Sound effect instances
const sounds = {
  perfect: new Howl({
    src: ['/success-1.mp3'],
    volume: 0.5,
    preload: true
  }),
  success: new Howl({
    src: ['/success-2.mp3'],
    volume: 0.5,
    preload: true
  }),
  fail: new Howl({
    src: ['/success-3.mp3'],
    volume: 0.5,
    preload: true
  })
}

// Keep track of currently playing sound effects
let currentSoundEffect: Howl | null = null

export const playSoundEffect = (type: 'perfect' | 'success' | 'fail'): Promise<void> => {
  return new Promise((resolve) => {
    const sound = sounds[type]
    
    // Stop any currently playing sound effect
    if (currentSoundEffect && currentSoundEffect.playing()) {
      currentSoundEffect.stop()
    }
    
    currentSoundEffect = sound
    
    // Play the sound and resolve when it's done
    const soundId = sound.play()
    
    sound.once('end', () => {
      currentSoundEffect = null
      resolve()
    }, soundId)
    
    sound.once('loaderror', () => {
      currentSoundEffect = null
      console.error(`Failed to load ${type} sound`)
      resolve()
    }, soundId)
    
    sound.once('playerror', () => {
      currentSoundEffect = null
      console.error(`Failed to play ${type} sound`)
      resolve()
    }, soundId)
  })
}

export const isSoundEffectPlaying = (): boolean => {
  return currentSoundEffect !== null && currentSoundEffect.playing()
}

export const stopCurrentSoundEffect = (): void => {
  if (currentSoundEffect && currentSoundEffect.playing()) {
    currentSoundEffect.stop()
    currentSoundEffect = null
  }
}
