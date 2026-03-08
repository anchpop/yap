// Background shader worker - handles all canvas rendering off the main thread

import { calculateColors, FALLBACK_BAND_INDEX, type ShaderTheme } from "../lib/shader-colors";

interface WorkerMessage {
  type: string;
  canvas?: OffscreenCanvas;
  theme?: ShaderTheme;
  width?: number;
  height?: number;
  devicePixelRatio?: number;
  multiplier?: number;
}

let gl: WebGLRenderingContext | null = null;
let canvas: OffscreenCanvas | null = null;
let currentTheme: ShaderTheme = "dark";
let animationFrameId: number | null = null;

// Overloaded function signatures
function zeno(
  current: number,
  target: number,
  delta_time: number,
  rate?: number
): number;
function zeno(
  current: number[],
  target: number[],
  delta_time: number,
  rate?: number
): number[];
function zeno(
  current: number[][],
  target: number[][],
  delta_time: number,
  rate?: number
): number[][];

// Implementation
function zeno(
  current: number | number[] | number[][],
  target: number | number[] | number[][],
  delta_time: number,
  rate = 5.0
): number | number[] | number[][] {
  const alpha = 1 - Math.exp(-rate * delta_time);

  // Scalar case
  if (typeof current === "number" && typeof target === "number") {
    return current + alpha * (target - current);
  }

  // Array case
  if (Array.isArray(current) && Array.isArray(target)) {
    // Check if it's array of arrays
    if (Array.isArray(current[0]) && Array.isArray(target[0])) {
      return (current as number[][]).map((row, i) =>
        row.map((val, j) => val + alpha * ((target as number[][])[i][j] - val))
      );
    }

    // Array of numbers
    return (current as number[]).map(
      (val, i) => val + alpha * ((target as number[])[i] - val)
    );
  }

  throw new Error("Invalid types for zeno function");
}


function createShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string
): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.error("Shader compile error:", gl.getShaderInfoLog(shader));
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

function createProgram(
  gl: WebGLRenderingContext,
  vertexShader: WebGLShader,
  fragmentShader: WebGLShader
): WebGLProgram | null {
  const program = gl.createProgram();
  if (!program) return null;
  gl.attachShader(program, vertexShader);
  gl.attachShader(program, fragmentShader);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    console.error("Program link error:", gl.getProgramInfoLog(program));
    return null;
  }
  return program;
}

function initWebGL(
  offscreenCanvas: OffscreenCanvas,
  theme: ShaderTheme
) {
  canvas = offscreenCanvas;
  currentTheme = theme;

  gl = canvas.getContext("webgl", {
    alpha: false,
    antialias: false,
    depth: false,
    stencil: false,
    preserveDrawingBuffer: false,
    powerPreference: "low-power",
  });

  if (!gl) {
    console.error("Failed to get WebGL context");
    return;
  }

  // Immediately clear to the middle band color so the canvas isn't black while shaders compile
  {
    const { colors } = calculateColors(theme);
    const offset = FALLBACK_BAND_INDEX * 3;
    gl.clearColor(colors[offset], colors[offset + 1], colors[offset + 2], 1.0);
    gl.clear(gl.COLOR_BUFFER_BIT);
  }

  const vertexShaderSrc = `
    attribute vec2 a_position;
    varying vec2 v_uv;
    void main() {
      v_uv = a_position * 0.5 + 0.5;
      gl_Position = vec4(a_position, 0.0, 1.0);
    }
  `;

  const fragmentShaderSrc = `
    precision mediump float;

    varying vec2 v_uv;
    uniform float u_time;
    uniform vec2 u_resolution;
    uniform float u_numBands;
    uniform vec3 u_colors[16]; // Pre-calculated colors (max 16 bands)

    #define PI 3.14159265359
    #define TAU 6.28318530718

    // === Smooth blob with extended tail ===
    float blob(vec2 uv, vec2 center, float radius) {
      float d = distance(uv, center);
      float t = 1.0 - smoothstep(0.0, radius * 2.0, d);
      return t * t;
    }

    void main() {
      vec2 uv = v_uv;
      float aspect = u_resolution.x / u_resolution.y;

      // Normalize to a consistent coordinate system based on the shorter edge
      // This ensures blobs appear the same size on both portrait and landscape
      vec2 uvAspect;
      if (aspect > 1.0) {
        // Landscape: scale x to fit
        uvAspect = vec2(uv.x * aspect, uv.y);
      } else {
        // Portrait: scale y to fit
        uvAspect = vec2(uv.x, uv.y / aspect);
      }

      float t = u_time;

      // Blob definitions
      const int NUM_BLOBS = 6;
      vec2 basePos[6];
      float radius[6];
      vec2 phase[6];
      float weight[6];

      basePos[0] = vec2(0.3, 0.3);   radius[0] = 0.35; phase[0] = vec2(0.0, 0.5);   weight[0] = 1.0;
      basePos[1] = vec2(0.75, 0.35); radius[1] = 0.32; phase[1] = vec2(1.0, 0.0);   weight[1] = 0.95;
      basePos[2] = vec2(0.5, 0.75);  radius[2] = 0.34; phase[2] = vec2(2.0, 1.5);   weight[2] = 1.0;
      basePos[3] = vec2(0.18, 0.65); radius[3] = 0.3;  phase[3] = vec2(0.5, 2.0);   weight[3] = 0.9;
      basePos[4] = vec2(0.85, 0.8);  radius[4] = 0.32; phase[4] = vec2(1.5, 0.3);   weight[4] = 0.9;
      basePos[5] = vec2(0.12, 0.15); radius[5] = 0.28; phase[5] = vec2(2.2, 1.8);   weight[5] = 0.85;

      // Accumulate grayscale value from all blobs
      float value = 0.0;

      for (int i = 0; i < NUM_BLOBS; i++) {
        vec2 offset = vec2(
          sin(t * 0.0003 + phase[i].x) * 0.14,
          cos(t * 0.00025 + phase[i].y) * 0.14
        );

        vec2 pos = basePos[i] + offset;

        // Apply same aspect ratio correction as uvAspect
        if (aspect > 1.0) {
          pos.x *= aspect;
        } else {
          pos.y /= aspect;
        }

        float influence = blob(uvAspect, pos, radius[i]);
        value += influence * weight[i];
      }

      // Add slow-moving background variation
      float bgWave = sin(uv.x * 2.5 + t * 0.00008) * 0.5 + 0.5;
      bgWave *= sin(uv.y * 2.0 + t * 0.00006) * 0.5 + 0.5;
      float baseVariation = bgWave * 0.4;

      // Blend: use base variation in empty areas, blob value where blobs are
      value = max(value, baseVariation * (1.0 - value * 0.8));
      value = clamp(value, 0.0, 0.99);

      // === QUANTIZE GRAYSCALE ===
      float band = floor(value * u_numBands) / u_numBands;

      int bandIndex = int(band * u_numBands);

      // Use pre-calculated color from uniform array
      // WebGL 1.0 doesn't support dynamic indexing, so we use conditionals
      vec3 color = u_colors[0];
      if (bandIndex == 1) color = u_colors[1];
      else if (bandIndex == 2) color = u_colors[2];
      else if (bandIndex == 3) color = u_colors[3];
      else if (bandIndex == 4) color = u_colors[4];
      else if (bandIndex == 5) color = u_colors[5];
      else if (bandIndex == 6) color = u_colors[6];
      else if (bandIndex == 7) color = u_colors[7];
      else if (bandIndex >= 8) color = u_colors[7]; // Clamp to last color

      // Subtle vignette
      float vignette = 1.0 - smoothstep(0.5, 1.5, length(v_uv - 0.5) * 1.3);
      color *= 0.94 + 0.06 * vignette;

      gl_FragColor = vec4(color, 1.0);
    }
  `;

  const vertexShader = createShader(gl, gl.VERTEX_SHADER, vertexShaderSrc);
  const fragmentShader = createShader(
    gl,
    gl.FRAGMENT_SHADER,
    fragmentShaderSrc
  );
  if (!vertexShader || !fragmentShader) return;

  const program = createProgram(gl, vertexShader, fragmentShader);
  if (!program) return;

  const positionBuffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]),
    gl.STATIC_DRAW
  );

  const positionLocation = gl.getAttribLocation(program, "a_position");
  const timeLocation = gl.getUniformLocation(program, "u_time");
  const resolutionLocation = gl.getUniformLocation(program, "u_resolution");
  const numBandsLocation = gl.getUniformLocation(program, "u_numBands");
  const colorsLocation = gl.getUniformLocation(program, "u_colors");

  let elapsedTime = 0;
  let lastFrameTime = performance.now();
  const targetSpeed = 0;
  let speed = 0.09; // Start with bump-like energy for initial lava-lamp effect
  let isAnimating = true;

  const SPEED_THRESHOLD = 0.0005;
  const COLOR_THRESHOLD = 0.001;

  const initialColorData = calculateColors(currentTheme);
  let targetColors = initialColorData.colors;
  let currentColors = [...targetColors]; // Start with target colors
  let numBands = initialColorData.numBands;

  function render() {
    if (!canvas || !gl) return;

    const now = performance.now();
    const deltaTime = now - lastFrameTime;
    lastFrameTime = now;

    // Decay speed back to target using zeno
    speed = zeno(speed, targetSpeed, deltaTime / 1000, 0.5);

    // Interpolate colors towards target
    currentColors = zeno(currentColors, targetColors, deltaTime / 1000, 18.0);

    elapsedTime += deltaTime * speed;

    gl.useProgram(program);

    gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
    gl.enableVertexAttribArray(positionLocation);
    gl.vertexAttribPointer(positionLocation, 2, gl.FLOAT, false, 0, 0);

    gl.uniform1f(timeLocation, elapsedTime);
    gl.uniform2f(resolutionLocation, canvas.width, canvas.height);
    gl.uniform1f(numBandsLocation, numBands);
    gl.uniform3fv(colorsLocation, currentColors);

    gl.drawArrays(gl.TRIANGLES, 0, 6);

    // Check if animation has fully settled (speed near zero and colors converged)
    if (speed < SPEED_THRESHOLD) {
      let colorsSettled = true;
      for (let i = 0; i < currentColors.length; i++) {
        if (
          Math.abs(currentColors[i] - targetColors[i]) > COLOR_THRESHOLD
        ) {
          colorsSettled = false;
          break;
        }
      }
      if (colorsSettled) {
        speed = 0;
        isAnimating = false;
        animationFrameId = null;
        return;
      }
    }

    animationFrameId = requestAnimationFrame(render);
  }

  // Restart the animation loop if it has settled
  function ensureAnimating() {
    if (!isAnimating) {
      isAnimating = true;
      lastFrameTime = performance.now();
      render();
    }
  }

  // Expose updateColors for theme changes
  (
    self as typeof self & {
      updateShaderColors?: () => void;
      bumpSpeed?: (multiplier?: number) => void;
    }
  ).updateShaderColors = () => {
    const newColorData = calculateColors(currentTheme);
    targetColors = newColorData.colors;
    numBands = newColorData.numBands;
    ensureAnimating();
  };

  // Expose ensureAnimating for resize redraws
  (
    self as typeof self & {
      updateShaderColors?: () => void;
      bumpSpeed?: (multiplier?: number) => void;
      ensureAnimating?: () => void;
    }
  ).ensureAnimating = ensureAnimating;

  // Expose bumpSpeed function
  (
    self as typeof self & {
      updateShaderColors?: () => void;
      bumpSpeed?: (multiplier?: number) => void;
    }
  ).bumpSpeed = (multiplier = 3.0) => {
    speed = 0.03 * multiplier;
    ensureAnimating();
  };

  render();
  self.postMessage({ type: "ready" });
}

// Listen for messages from the main thread
self.addEventListener("message", (event: MessageEvent<WorkerMessage>) => {
  const {
    type,
    canvas: offscreenCanvas,
    theme,
    width,
    height,
    devicePixelRatio,
  } = event.data;

  switch (type) {
    case "init": {
      if (offscreenCanvas && theme) {
        initWebGL(offscreenCanvas, theme);
      }
      break;
    }

    case "resize": {
      if (
        canvas &&
        gl &&
        width !== undefined &&
        height !== undefined &&
        devicePixelRatio !== undefined
      ) {
        const dpr = Math.min(devicePixelRatio, 1.5);
        const isMobile = width < 768;
        const scale = isMobile ? 0.35 : 0.75;
        canvas.width = width * dpr * scale;
        canvas.height = height * dpr * scale;
        gl.viewport(0, 0, canvas.width, canvas.height);
        const ensureAnim = (
          self as typeof self & { ensureAnimating?: () => void }
        ).ensureAnimating;
        if (ensureAnim) {
          ensureAnim();
        }
      }
      break;
    }

    case "theme": {
      if (theme) {
        currentTheme = theme;
        const updateColors = (
          self as typeof self & { updateShaderColors?: () => void }
        ).updateShaderColors;
        if (updateColors) {
          updateColors();
        }
      }
      break;
    }

    case "stop": {
      if (animationFrameId !== null) {
        cancelAnimationFrame(animationFrameId);
        animationFrameId = null;
      }
      break;
    }

    case "bump": {
      const bumpSpeed = (
        self as typeof self & { bumpSpeed?: (multiplier?: number) => void }
      ).bumpSpeed;
      if (bumpSpeed) {
        bumpSpeed(event.data.multiplier);
      }
      break;
    }

    default: {
      console.warn("[Worker] Unknown message type:", type);
      break;
    }
  }
});
