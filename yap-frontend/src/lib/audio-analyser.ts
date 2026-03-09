let audioContext: AudioContext | null = null;
let analyser: AnalyserNode | null = null;

function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
  }
  return audioContext;
}

export function getAnalyser(): AnalyserNode {
  if (!analyser) {
    const ctx = getAudioContext();
    analyser = ctx.createAnalyser();
    analyser.fftSize = 256;
    analyser.smoothingTimeConstant = 0.7;
    analyser.connect(ctx.destination);
  }
  return analyser;
}

export function connectAudioElement(audio: HTMLAudioElement): void {
  const ctx = getAudioContext();
  // Resume context if suspended (browsers require user gesture)
  if (ctx.state === "suspended") {
    void ctx.resume();
  }
  const source = ctx.createMediaElementSource(audio);
  source.connect(getAnalyser());
}

/**
 * Get normalized frequency data from the analyser (0-1 range).
 * Returns an empty array if no analyser exists.
 */
export function getFrequencyData(): Float32Array {
  const node = getAnalyser();
  const data = new Uint8Array(node.frequencyBinCount);
  node.getByteFrequencyData(data);
  const normalized = new Float32Array(data.length);
  for (let i = 0; i < data.length; i++) {
    normalized[i] = data[i] / 255;
  }
  return normalized;
}
