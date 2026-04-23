// Background shader worker - handles all canvas rendering off the main thread

import {
  calculateColors,
  FALLBACK_BAND_INDEX,
  type ShaderTheme,
} from "../lib/shader-colors";

interface WorkerMessage {
  type: string;
  canvas?: OffscreenCanvas;
  theme?: ShaderTheme;
  width?: number;
  height?: number;
  devicePixelRatio?: number;
  multiplier?: number;
  x?: number;
  y?: number;
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
  rate?: number,
): number;
function zeno(
  current: number[],
  target: number[],
  delta_time: number,
  rate?: number,
): number[];
function zeno(
  current: number[][],
  target: number[][],
  delta_time: number,
  rate?: number,
): number[][];

// Implementation
function zeno(
  current: number | number[] | number[][],
  target: number | number[] | number[][],
  delta_time: number,
  rate = 5.0,
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
        row.map((val, j) => val + alpha * ((target as number[][])[i][j] - val)),
      );
    }

    // Array of numbers
    return (current as number[]).map(
      (val, i) => val + alpha * ((target as number[])[i] - val),
    );
  }

  throw new Error("Invalid types for zeno function");
}

function createShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string,
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
  fragmentShader: WebGLShader,
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

function initWebGL(offscreenCanvas: OffscreenCanvas, theme: ShaderTheme) {
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
    uniform float u_isDark; // 1.0 for waves+glow (dark/oled), 0.0 for metaballs (light)
    uniform float u_oled; // 1.0 for OLED (pure black base), 0.0 otherwise
    uniform vec2 u_mouse; // Normalized mouse position (0..1), drives the sun anchor

    #define PI 3.14159265359
    #define TAU 6.28318530718

    // === Smooth blob with extended tail (light/oled themes) ===
    float blob(vec2 uv, vec2 center, float radius) {
      float d = distance(uv, center);
      float t = 1.0 - smoothstep(0.0, radius * 2.0, d);
      return t * t;
    }

    // A shared slow wave that all layers ride on, creating coherent large-scale
    // swells. Two sines at different low frequencies, moving in opposite directions.
    float sharedWave(float x, float t) {
      return 0.07 * sin(x * 8.8 + t * 0.12)
           + 0.05 * sin(x * 4.8 - t * 0.08);
    }

    // Multi-octave sine ridge: three octaves with decreasing amplitude and
    // staggered phases so crests don't align into a pure sine.
    float layerHeight(float x, float baseY, float amp, float freq, float phase) {
      float h = 0.0;
      h += 1.00 * sin(x * freq        + phase);
      h += 0.50 * sin(x * freq * 2.17 + phase * 1.7 + 1.3);
      h += 0.25 * sin(x * freq * 4.31 + phase * 2.3 + 2.9);
      h /= (1.00 + 0.50 + 0.25);
      return baseY + amp * h;
    }

    // Map a 0..1 point into the same aspect-corrected space used by uvAspect,
    // so distances are measured on a common coordinate system regardless of
    // orientation. Keep everything that feeds into lightDist/facing going
    // through this helper.
    vec2 toAspect(vec2 p, float aspect) {
      return aspect > 1.0 ? vec2(p.x * aspect, p.y) : vec2(p.x, p.y / aspect);
    }

    void main() {
      vec2 uv = v_uv;
      float aspect = u_resolution.x / u_resolution.y;
      vec2 uvAspect = toAspect(uv, aspect);

      float t = u_time;
      float value = 0.0;

      if (u_isDark > 0.5) {
        // ===== Dark theme: layered sine mountains lit from the upper-right =====
        // u_time accumulates in ms-scale via the render loop's speed factor;
        // convert to a seconds-ish scale for the per-layer drift.
        float iTime = t * 0.001;

        vec3 baseCol = mix(vec3(34.0, 21.0, 40.0) / 255.0, vec3(0.0), u_oled);
        vec3 haloCol = vec3(168.0, 77.0, 74.0) / 255.0;
        vec3 hotCol  = vec3(248.0, 150.0, 112.0) / 255.0;
        vec3 lineCol = vec3(228.0, 119.0, 91.0) / 255.0;

        vec2 lightAnchor = toAspect(u_mouse, aspect);
        float lightDist = distance(uvAspect, lightAnchor);
        // Tighter falloff for OLED — keeps most of the screen pure black.
        float haloRadius = mix(1.3, 0.55, u_oled);
        float coreRadius = mix(0.4, 0.22, u_oled);
        float lightInfluence = 1.0 - smoothstep(0.0, haloRadius, lightDist);
        lightInfluence = lightInfluence * lightInfluence;

        float lightCore = 1.0 - smoothstep(0.0, coreRadius, lightDist);
        lightCore = lightCore * lightCore * lightCore;

        vec3 col = baseCol * 0.35;

        float baseY[5];  float amp[5];  float freq[5];  float phase[5];
        float speed[5]; float rimDepth[5]; float rimBoost[5];

        speed[0] =  0.07; speed[1] = -0.11; speed[2] =  0.13; speed[3] = -0.05; speed[4] =  0.17;

        // Layer 0 is the "mega-mountain" that sweeps across the upper half.
        baseY[0] = 0.75; amp[0] = 0.25; freq[0] = 1.4; phase[0] = 0.3 + iTime * speed[0];
        baseY[1] = 0.42; amp[1] = 0.09; freq[1] = 4.0; phase[1] = 1.7 + iTime * speed[1];
        baseY[2] = 0.34; amp[2] = 0.07; freq[2] = 5.5; phase[2] = 2.9 + iTime * speed[2];
        baseY[3] = 0.26; amp[3] = 0.10; freq[3] = 3.5; phase[3] = 0.9 + iTime * speed[3];
        baseY[4] = 0.18; amp[4] = 0.06; freq[4] = 6.0; phase[4] = 4.2 + iTime * speed[4];

        rimDepth[0] = 0.20; rimDepth[1] = 0.09; rimDepth[2] = 0.08; rimDepth[3] = 0.10; rimDepth[4] = 0.07;
        rimBoost[0] = 0.30; rimBoost[1] = 0.12; rimBoost[2] = 0.14; rimBoost[3] = 0.16; rimBoost[4] = 0.18;

        for (int i = 0; i < 5; i++) {
          float h = layerHeight(uv.x, baseY[i], amp[i], freq[i], phase[i]) + sharedWave(uv.x, iTime);

          // Numerical slope of the full ridge (all fBm octaves + shared wave).
          // Cheaper to write than to differentiate by hand and automatically
          // correct if layerHeight or sharedWave changes.
          float eps = 0.001;
          float hLeft  = layerHeight(uv.x - eps, baseY[i], amp[i], freq[i], phase[i]) + sharedWave(uv.x - eps, iTime);
          float hRight = layerHeight(uv.x + eps, baseY[i], amp[i], freq[i], phase[i]) + sharedWave(uv.x + eps, iTime);
          float dhdx = (hRight - hLeft) / (2.0 * eps);
          vec2 normal = normalize(vec2(-dhdx, 1.0));

          // Direction from the ridge point toward the light (aspect-corrected).
          vec2 ridgePoint = toAspect(vec2(uv.x, h), aspect);
          vec2 toLight = normalize(lightAnchor - ridgePoint);
          float facing = max(dot(normal, toLight), 0.0);

          // Halo: brighten pixels just above the ridge, modulated by light and facing.
          if (uv.y >= h && uv.y < h + rimDepth[i]) {
            float ht = 1.0 - clamp((uv.y - h) / rimDepth[i], 0.0, 1.0);
            ht = ht * ht * ht * ht;
            float haloFactor = 0.05 + 2.0 * lightInfluence * facing;
            vec3 localHaloCol = mix(haloCol, hotCol, lightCore);
            col += localHaloCol * (rimBoost[i] * ht * haloFactor);
          }

          // Body: fill below the ridge with a depth-weighted warm lift.
          if (uv.y < h) {
            float depth = float(i) / 4.0;
            float depthWeight = 0.3 + 0.9 * depth;
            float gradDepth = clamp((h - uv.y) / 0.25, 0.0, 1.0);
            float gradMul = 0.7 + 0.3 * gradDepth;

            vec3 body = baseCol * gradMul;
            // Warm body-lift is purple-theme only — in OLED the body stays black
            // so only the ridge halos read as the "sun".
            float bodyLift = 1.0 - u_oled;
            body += haloCol * lightInfluence * 0.35 * depthWeight * bodyLift;
            body += hotCol * lightCore * 0.2 * depthWeight * bodyLift;
            col = body;
          }

          // Crisp ridge line — only on slopes facing the light.
          float lineFalloff = lightInfluence * facing;
          lineFalloff = lineFalloff * lineFalloff;
          float lineWidth = (0.3 + 1.2 * lineFalloff) / u_resolution.y;
          float lineSoftEdge = 1.5 / u_resolution.y;
          float dist = abs(uv.y - h);
          float lineMask = 1.0 - smoothstep(lineWidth, lineWidth + lineSoftEdge, dist);
          col = mix(col, lineCol, lineMask * lineFalloff);
        }

        gl_FragColor = vec4(col, 1.0);
        return;
      }

      // ===== Light / OLED themes: original metaballs =====
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

      for (int i = 0; i < NUM_BLOBS; i++) {
        vec2 offset = vec2(
          sin(t * 0.0003 + phase[i].x) * 0.14,
          cos(t * 0.00025 + phase[i].y) * 0.14
        );

        vec2 pos = toAspect(basePos[i] + offset, aspect);

        float influence = blob(uvAspect, pos, radius[i]);
        value += influence * weight[i];
      }

      // Slow-moving background variation
      float bgWave = sin(uv.x * 2.5 + t * 0.00008) * 0.5 + 0.5;
      bgWave *= sin(uv.y * 2.0 + t * 0.00006) * 0.5 + 0.5;
      float baseVariation = bgWave * 0.4;

      value = max(value, baseVariation * (1.0 - value * 0.8));
      value = clamp(value, 0.0, 0.99);

      float band = floor(value * u_numBands) / u_numBands;
      int bandIndex = int(band * u_numBands);

      vec3 color = u_colors[0];
      if (bandIndex == 1) color = u_colors[1];
      else if (bandIndex == 2) color = u_colors[2];
      else if (bandIndex == 3) color = u_colors[3];
      else if (bandIndex == 4) color = u_colors[4];
      else if (bandIndex == 5) color = u_colors[5];
      else if (bandIndex == 6) color = u_colors[6];
      else if (bandIndex == 7) color = u_colors[7];
      else if (bandIndex >= 8) color = u_colors[7];

      float vignette = 1.0 - smoothstep(0.5, 1.5, length(v_uv - 0.5) * 1.3);
      color *= 0.94 + 0.06 * vignette;

      gl_FragColor = vec4(color, 1.0);
    }
  `;

  const vertexShader = createShader(gl, gl.VERTEX_SHADER, vertexShaderSrc);
  const fragmentShader = createShader(
    gl,
    gl.FRAGMENT_SHADER,
    fragmentShaderSrc,
  );
  if (!vertexShader || !fragmentShader) return;

  const program = createProgram(gl, vertexShader, fragmentShader);
  if (!program) return;

  const positionBuffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]),
    gl.STATIC_DRAW,
  );

  const positionLocation = gl.getAttribLocation(program, "a_position");
  const timeLocation = gl.getUniformLocation(program, "u_time");
  const resolutionLocation = gl.getUniformLocation(program, "u_resolution");
  const numBandsLocation = gl.getUniformLocation(program, "u_numBands");
  const colorsLocation = gl.getUniformLocation(program, "u_colors");
  const isDarkLocation = gl.getUniformLocation(program, "u_isDark");
  const oledLocation = gl.getUniformLocation(program, "u_oled");
  const mouseLocation = gl.getUniformLocation(program, "u_mouse");

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

  // Mouse-driven sun anchor (normalized 0..1, y is up). Default matches the
  // fallback upper-right spot, so touch / no-mouse users get the static look.
  const DEFAULT_MOUSE = [0.95, 0.4];
  let targetMouse: number[] = [...DEFAULT_MOUSE];
  let currentMouse: number[] = [...DEFAULT_MOUSE];
  const MOUSE_THRESHOLD = 0.0005;

  function render() {
    if (!canvas || !gl) return;

    const now = performance.now();
    const deltaTime = now - lastFrameTime;
    lastFrameTime = now;

    // Decay speed back to target using zeno
    speed = zeno(speed, targetSpeed, deltaTime / 1000, 0.5);

    // Interpolate colors towards target
    currentColors = zeno(currentColors, targetColors, deltaTime / 1000, 18.0);

    // Interpolate the sun toward the mouse cursor (or fall back to default)
    currentMouse = zeno(currentMouse, targetMouse, deltaTime / 1000, 20.0);

    elapsedTime += deltaTime * speed;

    gl.useProgram(program);

    gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
    gl.enableVertexAttribArray(positionLocation);
    gl.vertexAttribPointer(positionLocation, 2, gl.FLOAT, false, 0, 0);

    gl.uniform1f(timeLocation, elapsedTime);
    gl.uniform2f(resolutionLocation, canvas.width, canvas.height);
    gl.uniform1f(numBandsLocation, numBands);
    gl.uniform3fv(colorsLocation, currentColors);
    gl.uniform1f(
      isDarkLocation,
      currentTheme === "dark" || currentTheme === "oled" ? 1.0 : 0.0,
    );
    gl.uniform1f(oledLocation, currentTheme === "oled" ? 1.0 : 0.0);
    gl.uniform2f(mouseLocation, currentMouse[0], currentMouse[1]);

    gl.drawArrays(gl.TRIANGLES, 0, 6);

    // Check if animation has fully settled (speed, colors, and mouse all converged)
    if (speed < SPEED_THRESHOLD) {
      let colorsSettled = true;
      for (let i = 0; i < currentColors.length; i++) {
        if (Math.abs(currentColors[i] - targetColors[i]) > COLOR_THRESHOLD) {
          colorsSettled = false;
          break;
        }
      }
      const mouseSettled =
        Math.abs(currentMouse[0] - targetMouse[0]) < MOUSE_THRESHOLD &&
        Math.abs(currentMouse[1] - targetMouse[1]) < MOUSE_THRESHOLD;
      if (colorsSettled && mouseSettled) {
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

  // Expose setMouse for main-thread cursor updates. Always cache the latest
  // position so a theme switch into dark/oled picks it up immediately, but
  // only wake the render loop when the active shader actually reads u_mouse.
  (
    self as typeof self & {
      setMouse?: (x: number, y: number) => void;
    }
  ).setMouse = (x: number, y: number) => {
    targetMouse = [x, y];
    if (currentTheme === "dark" || currentTheme === "oled") {
      ensureAnimating();
    }
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

    case "mouse": {
      const setMouse = (
        self as typeof self & {
          setMouse?: (x: number, y: number) => void;
        }
      ).setMouse;
      if (setMouse && event.data.x !== undefined && event.data.y !== undefined) {
        setMouse(event.data.x, event.data.y);
      }
      break;
    }

    default: {
      console.warn("[Worker] Unknown message type:", type);
      break;
    }
  }
});
