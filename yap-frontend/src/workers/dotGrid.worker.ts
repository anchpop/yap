// Draws the landing's dot grid (see lib/dot-grid-sim.ts) on an
// OffscreenCanvas: one WebGL point per dot, glowing the lamp's pink as it
// speeds up. Runs only while something moves.

import { DotGridSim } from "../lib/dot-grid-sim";
import type { ShaderTheme } from "../lib/shader-colors";

interface WorkerMessage {
  type: string;
  canvas?: OffscreenCanvas;
  theme?: ShaderTheme;
  width?: number;
  height?: number;
  devicePixelRatio?: number;
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
}

/** Dot radius, CSS px. */
const RADIUS = 1.5;

// Ink at rest; the lamp's pink as a dot speeds up.
const INK: Record<"light" | "dark", [number, number, number, number]> = {
  light: [0.19, 0.0, 0.19, 0.2],
  dark: [0.97, 0.94, 0.96, 0.22],
};
const LAMP: [number, number, number, number] = [0.89, 0.45, 0.87, 0.95];

const VERT = `
  precision mediump float;
  attribute vec2 a_pos;
  attribute vec2 a_vel;
  uniform vec2 u_size;
  uniform float u_point;
  varying float v_glow;
  void main() {
    vec2 clip = (a_pos / u_size) * 2.0 - 1.0;
    gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
    gl_PointSize = u_point;
    v_glow = smoothstep(20.0, 500.0, length(a_vel));
  }
`;
const FRAG = `
  precision mediump float;
  uniform vec4 u_ink;
  uniform vec4 u_lamp;
  uniform float u_point;
  varying float v_glow;
  void main() {
    // Antialiased disc, one device pixel of edge.
    float d = length(gl_PointCoord - 0.5) * u_point;
    float cover = clamp(u_point * 0.5 - d + 0.5, 0.0, 1.0);
    vec4 c = mix(u_ink, u_lamp, v_glow);
    gl_FragColor = vec4(c.rgb * c.a, c.a) * cover;
  }
`;

let canvas: OffscreenCanvas | null = null;
let gl: WebGLRenderingContext | null = null;
let program: WebGLProgram | null = null;
let posBuffer: WebGLBuffer | null = null;
let velBuffer: WebGLBuffer | null = null;
let sim: DotGridSim | null = null;
let dark = false;
let dpr = 1;
let frame: number | null = null;
let lastTime = 0;

function compile(type: number, src: string): WebGLShader {
  const shader = gl!.createShader(type)!;
  gl!.shaderSource(shader, src);
  gl!.compileShader(shader);
  if (!gl!.getShaderParameter(shader, gl!.COMPILE_STATUS))
    throw new Error(gl!.getShaderInfoLog(shader) ?? "shader");
  return shader;
}

function fit(width: number, height: number, devicePixelRatio: number) {
  if (!canvas || !gl) return;
  dpr = Math.min(devicePixelRatio, 2);
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  gl.viewport(0, 0, canvas.width, canvas.height);
  if (sim && sim.width === width && sim.height === height) return;
  sim = new DotGridSim(width, height);
  gl.bindBuffer(gl.ARRAY_BUFFER, posBuffer);
  gl.bufferData(gl.ARRAY_BUFFER, sim.pos.byteLength, gl.DYNAMIC_DRAW);
  gl.bindBuffer(gl.ARRAY_BUFFER, velBuffer);
  gl.bufferData(gl.ARRAY_BUFFER, sim.vel.byteLength, gl.DYNAMIC_DRAW);
}

function init(
  offscreen: OffscreenCanvas,
  theme: ShaderTheme,
  width: number,
  height: number,
  devicePixelRatio: number,
) {
  canvas = offscreen;
  dark = theme !== "light";
  gl = canvas.getContext("webgl", {
    alpha: true,
    premultipliedAlpha: true,
    antialias: false,
    depth: false,
    stencil: false,
    powerPreference: "low-power",
  });
  if (!gl) return;
  program = gl.createProgram()!;
  gl.attachShader(program, compile(gl.VERTEX_SHADER, VERT));
  gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS))
    throw new Error(gl.getProgramInfoLog(program) ?? "program");
  posBuffer = gl.createBuffer();
  velBuffer = gl.createBuffer();
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  fit(width, height, devicePixelRatio);
  wake();
}

function draw() {
  if (!gl || !program || !sim) return;
  gl.clearColor(0, 0, 0, 0);
  gl.clear(gl.COLOR_BUFFER_BIT);
  gl.useProgram(program);
  gl.uniform2f(gl.getUniformLocation(program, "u_size"), sim.width, sim.height);
  gl.uniform1f(gl.getUniformLocation(program, "u_point"), RADIUS * 2 * dpr);
  gl.uniform4fv(
    gl.getUniformLocation(program, "u_ink"),
    INK[dark ? "dark" : "light"],
  );
  gl.uniform4fv(gl.getUniformLocation(program, "u_lamp"), LAMP);
  const aPos = gl.getAttribLocation(program, "a_pos");
  const aVel = gl.getAttribLocation(program, "a_vel");
  gl.bindBuffer(gl.ARRAY_BUFFER, posBuffer);
  gl.bufferSubData(gl.ARRAY_BUFFER, 0, sim.pos);
  gl.enableVertexAttribArray(aPos);
  gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, velBuffer);
  gl.bufferSubData(gl.ARRAY_BUFFER, 0, sim.vel);
  gl.enableVertexAttribArray(aVel);
  gl.vertexAttribPointer(aVel, 2, gl.FLOAT, false, 0, 0);
  gl.drawArrays(gl.POINTS, 0, sim.count);
}

function tick(now: number) {
  frame = null;
  const dt = Math.min(1 / 30, Math.max(1 / 240, (now - lastTime) / 1000));
  lastTime = now;
  const moving = sim?.step(dt) ?? false;
  draw();
  if (moving) frame = requestAnimationFrame(tick);
}

/** Start the loop if it is not running, drawing right away so the grid is
 *  never blank while waiting on a frame. */
function wake() {
  if (frame !== null || !gl) return;
  lastTime = performance.now();
  tick(lastTime);
}

self.addEventListener("message", (event: MessageEvent<WorkerMessage>) => {
  const m = event.data;
  switch (m.type) {
    case "init":
      if (
        m.canvas &&
        m.theme &&
        m.width !== undefined &&
        m.height !== undefined &&
        m.devicePixelRatio !== undefined
      )
        init(m.canvas, m.theme, m.width, m.height, m.devicePixelRatio);
      break;
    case "resize":
      if (
        m.width !== undefined &&
        m.height !== undefined &&
        m.devicePixelRatio !== undefined
      ) {
        fit(m.width, m.height, m.devicePixelRatio);
        wake();
      }
      break;
    case "theme":
      if (m.theme) {
        dark = m.theme !== "light";
        wake();
      }
      break;
    case "pointer":
      if (
        sim &&
        m.x !== undefined &&
        m.y !== undefined &&
        m.vx !== undefined &&
        m.vy !== undefined
      ) {
        sim.splat(m.x, m.y, m.vx, m.vy);
        wake();
      }
      break;
    case "stop":
      if (frame !== null) cancelAnimationFrame(frame);
      frame = null;
      break;
  }
});
