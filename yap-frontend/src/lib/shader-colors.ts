// Shared color calculation for the background shader.
// Used by both the web worker and main thread.

export function lchToRgb(
  L: number,
  C: number,
  H: number
): [number, number, number] {
  const a = C * Math.cos(H);
  const b = C * Math.sin(H);

  const D65 = [0.95047, 1.0, 1.08883];
  const labFInv = (t: number) =>
    t > 0.206893 ? t * t * t : (t - 16.0 / 116.0) / 7.787;

  const fy = (L + 16.0) / 116.0;
  const fx = a / 500.0 + fy;
  const fz = fy - b / 200.0;

  const xyz = [
    D65[0] * labFInv(fx),
    D65[1] * labFInv(fy),
    D65[2] * labFInv(fz),
  ];

  const fromLinear = (c: number) =>
    c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1.0 / 2.4) - 0.055;

  const r = 3.2404542 * xyz[0] - 1.5371385 * xyz[1] - 0.4985314 * xyz[2];
  const g = -0.969266 * xyz[0] + 1.8760108 * xyz[1] + 0.041556 * xyz[2];
  const b2 = 0.0556434 * xyz[0] - 0.2040259 * xyz[1] + 1.0572252 * xyz[2];

  return [
    Math.max(0, Math.min(1, fromLinear(r))),
    Math.max(0, Math.min(1, fromLinear(g))),
    Math.max(0, Math.min(1, fromLinear(b2))),
  ];
}

export type ShaderTheme = "dark" | "light" | "oled";

export function calculateColors(theme: ShaderTheme) {
  const numBands = 6;
  const lightness = theme === "dark" ? 15.0 : theme === "oled" ? 5.0 : 78.0;
  const chroma = theme === "oled" ? 0.0 : theme === "dark" ? 9.0 : 35.0;
  const lightnessShift =
    theme === "oled" ? 15.0 : theme === "dark" ? 7.0 : 12.0;
  const hueStart = theme === "oled" ? 3.2 : theme === "dark" ? 5.2 : 3.2;
  const hueRange = theme === "oled" ? -3.0 : theme === "dark" ? 3.0 : -3.0;

  const colors: number[] = [];
  for (let i = 0; i < numBands; i++) {
    const band = i / numBands;
    let H = hueStart + band * hueRange;
    H = H % (2 * Math.PI);
    if (H < 0) H += 2 * Math.PI;

    const L = lightness + (band - 0.5) * lightnessShift;
    const rgb = lchToRgb(L, chroma, H);
    colors.push(rgb[0], rgb[1], rgb[2]);
  }

  return { colors, numBands };
}

export const FALLBACK_BAND_INDEX = 1;

export function getShaderBackgroundCss(theme: ShaderTheme): string {
  const { colors } = calculateColors(theme);
  const offset = FALLBACK_BAND_INDEX * 3;
  const r = Math.round(colors[offset] * 255);
  const g = Math.round(colors[offset + 1] * 255);
  const b = Math.round(colors[offset + 2] * 255);
  return `rgb(${r}, ${g}, ${b})`;
}
