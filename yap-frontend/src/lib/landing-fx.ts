/**
 * The landing hero's snowfall, drawn on a transparent canvas over the photo:
 * a 3D cloud of flakes filling the frame and slowly turning around the lamp
 * post, lit violet near the lamp, fogged with distance, blurred when close,
 * and hidden where it passes behind the post or the lamp head (point
 * sprites).
 *
 * `mountFx` starts the effect and returns a cleanup. The loop only runs while
 * the canvas is on screen and the tab is visible; under
 * `prefers-reduced-motion` a single frame is drawn instead.
 */

/** Where the lamp post sits in the photo, as fractions of the photo itself
 *  (not of the canvas): measure once from the image file and it stays right
 *  under any crop or viewport, because the effect recomputes the photo's
 *  `object-fit: cover` box from the canvas size at draw time. The canvas must
 *  cover the same box as the photo. */
export interface OrbitGeometry {
  /** The photo's width / height. */
  aspect: number;
  /** The photo's `object-position`, as fractions. */
  position: [number, number];
  postTop: [number, number];
  postBottom: [number, number];
  /** Half the post's thickness at its top and at its bottom, as fractions
   *  of the photo's width. */
  postHalfWidth: [number, number];
  head: [number, number];
  headRadius: [number, number];
  count: number;
}

const ORBIT_VERT = `
attribute float rf, th0, vt, y0, vy, sz, seed, cloud;
uniform vec2 res; uniform float t; uniform float dpr; uniform float lamp;
uniform vec4 post; uniform vec2 postHW; uniform vec2 headC; uniform vec2 headR; uniform float photoW;
varying float vAlpha; varying vec3 vCol; varying float vSoft;
void main(){
  // Flakes live on a cylinder around the post, sized to the canvas so the
  // cloud always fills the frame. Far from the post they drift at a steady
  // speed; close in they share one slow turn, so the lamp has a visible eddy.
  float r = rf * res.x;
  float om = vt / max(r, 0.09 * res.x);
  float th = th0 + om * t;
  // Flakes recycle well outside the frame and fade across the margin, so
  // none pops into view even when perspective pulls it toward the centre.
  float H = res.y;
  float M = 0.5 * H;
  float y = mod(y0 + vy * t, H + 2.0 * M) - M;
  float edge = smoothstep(-M, -0.5 * M, y) * (1.0 - smoothstep(H + 0.5 * M, H + M, y));
  float sway = (6.0 + seed * 14.0) * sin(t * (0.5 + seed * 0.5) + seed * 6.28);
  float x3 = r * cos(th) + sway;
  float z3 = r * sin(th) * 0.5;
  float k = clamp((y - post.w) / (post.y - post.w), 0.0, 1.0);
  float axisX = mix(post.z, post.x, k);
  float f = 1.3 * res.x;
  float scale = f / (f - z3);
  float cy = res.y * 0.5;
  float sx = axisX + x3 * scale;
  float sy = cy + (y - cy) * scale;
  float hide = 0.0;
  if (z3 < 0.0) {
    float kk = clamp((sy - post.w) / (post.y - post.w), 0.0, 1.0);
    float px = mix(post.z, post.x, kk);
    float hw = mix(postHW.x, postHW.y, kk) * photoW;
    if (sy > post.y && sy < post.w && abs(sx - px) < hw) hide = 1.0;
    vec2 e = (vec2(sx, sy) - headC) / headR;
    if (dot(e, e) < 1.0) hide = 1.0;
  }
  float depth = clamp(0.5 + z3 / (1.13 * res.x), 0.0, 1.0);
  float fog = mix(0.3, 1.0, pow(depth, 1.4));
  // The shade throws the light down in a widening cone that fades with
  // distance; above the head only a small halo escapes.
  // Measured on screen, so the cone hangs under the head as drawn whatever
  // the perspective did to the flake; depth still counts as distance.
  vec3 rel = vec3(sx - headC.x, sy - headC.y, z3) / photoW;
  float down = rel.y;
  float side = length(rel.xz);
  float cone = 0.03 + max(down, 0.0) * 0.6;
  float inside = 1.0 - smoothstep(cone * 0.6, cone, side);
  float reach = down > 0.0 ? 1.0 / (1.0 + (down * down) / (0.3 * 0.3)) : exp(down / 0.02);
  float halo = 0.6 * exp(-dot(rel, rel) / (2.0 * 0.05 * 0.05));
  float glow = 0.3 * exp(-dot(rel, rel) / (2.0 * 0.16 * 0.16));
  float lit = lamp * max(max(inside * reach, halo), glow);
  vec3 cool = mix(vec3(0.97, 0.96, 1.0), vec3(0.78, 0.74, 0.92), lamp);
  vec3 warm = vec3(0.98, 0.62, 0.96);
  vCol = mix(cool, warm, clamp(lit * 1.4, 0.0, 1.0));
  // The closest flakes go soft and pale like bokeh.
  float near = smoothstep(1.15, 1.75, scale);
  // The dense cloud around the post is only there where the lamp lights it,
  // so it blooms under the head and thins out beyond the light (and by day).
  float cloudAlpha = mix(1.0, mix(0.1, 1.0, clamp(lit * 1.3, 0.0, 1.0)), cloud);
  // Unlit flakes are a faint mist; the lamp is what makes them show. By day
  // there is no lamp, so the mist itself carries a little more.
  float base = mix(0.55, 0.4, lamp);
  vAlpha = edge * fog * (base + 0.65 * lit) * (0.75 + 0.25 * seed) * mix(1.0, 0.3, near) * cloudAlpha;
  vSoft = mix(mix(0.3, 0.6, depth), 0.95, near);
  float size = min(sz * scale * (1.0 + 0.3 * lit), 6.0) * dpr;
  gl_PointSize = hide > 0.5 ? 0.0 : size;
  gl_Position = vec4(sx / res.x * 2.0 - 1.0, 1.0 - sy / res.y * 2.0, 0.0, 1.0);
}`;

const ORBIT_FRAG = `
precision mediump float;
varying float vAlpha; varying vec3 vCol; varying float vSoft;
void main(){
  vec2 p = gl_PointCoord * 2.0 - 1.0;
  float d = length(p);
  float a = smoothstep(1.0, 1.0 - vSoft, d) * vAlpha;
  gl_FragColor = vec4(vCol * a, a);
}`;

const ORBIT_ATTRIBUTES = [
  "rf",
  "th0",
  "vt",
  "y0",
  "vy",
  "sz",
  "seed",
  "cloud",
] as const;

function compile(gl: WebGLRenderingContext, type: number, src: string) {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("createShader failed");
  gl.shaderSource(shader, src);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? "shader compile failed");
  }
  return shader;
}

function link(gl: WebGLRenderingContext, vert: string, frag: string) {
  const prog = gl.createProgram();
  if (!prog) throw new Error("createProgram failed");
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, vert));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, frag));
  gl.linkProgram(prog);
  gl.useProgram(prog);
  return prog;
}

/** Deterministic PRNG so the snow cloud looks the same on every visit. */
function mulberry(seed: number) {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Resize the drawing buffer to the canvas' CSS size; returns the dpr used. */
function fitCanvas(gl: WebGLRenderingContext, canvas: HTMLCanvasElement) {
  const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
  const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
  // (aliased: the lint config forbids assigning to a parameter's properties)
  const el = canvas;
  if (el.width !== w || el.height !== h) {
    el.width = w;
    el.height = h;
  }
  gl.viewport(0, 0, w, h);
  return dpr;
}

type Draw = (time: number) => void;

/** Whether the lamp is lit (1) or off (0), from the canvas's `--fx-lamp`
 *  custom property, so the page's theme CSS decides. Unset means lit. */
function lampOn(canvas: HTMLCanvasElement) {
  const v = getComputedStyle(canvas).getPropertyValue("--fx-lamp").trim();
  return v === "" ? 1 : parseFloat(v);
}

/** The rectangle the photo's pixels occupy in canvas CSS px: the canvas box
 *  under `object-fit: cover` at the geometry's aspect and `object-position`. */
function photoBox(geom: OrbitGeometry, canvas: HTMLCanvasElement) {
  const cw = canvas.clientWidth;
  const ch = canvas.clientHeight;
  const s = Math.max(cw / geom.aspect, ch);
  const w = s * geom.aspect;
  const h = s;
  return {
    x: (cw - w) * geom.position[0],
    y: (ch - h) * geom.position[1],
    w,
    h,
  };
}

function buildOrbit(
  gl: WebGLRenderingContext,
  canvas: HTMLCanvasElement,
  geom: OrbitGeometry,
): Draw {
  const prog = link(gl, ORBIT_VERT, ORBIT_FRAG);
  const n = geom.count;
  const stride = ORBIT_ATTRIBUTES.length;
  const data = new Float32Array(n * stride);
  const rnd = mulberry(7);
  for (let i = 0; i < n; i++) {
    const o = i * stride;
    // Two populations, interleaved so any prefix of the buffer keeps the
    // mix: a broad fall across the whole frame, and a dense cloud hugging
    // the post where the lamp lights it.
    const cloud = i % 20 < 11 ? 1 : 0;
    data[o + 0] = cloud
      ? 0.015 + Math.pow(rnd(), 1.4) * 0.42
      : 0.03 + Math.pow(rnd(), 0.9) * 1.1; // radius, in canvas widths
    data[o + 1] = rnd() * 6.2832; // starting angle
    data[o + 2] = 10 + rnd() * 28; // drift speed around the post, px/s
    data[o + 3] = rnd() * 4000; // starting height, px (wrapped per frame)
    data[o + 4] = 16 + rnd() * 34; // fall speed, px/s
    data[o + 5] = 0.9 + Math.pow(rnd(), 3) * 3.2; // base size, px (mostly tiny)
    data[o + 6] = rnd();
    data[o + 7] = cloud;
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  ORBIT_ATTRIBUTES.forEach((name, i) => {
    const loc = gl.getAttribLocation(prog, name);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 1, gl.FLOAT, false, stride * 4, i * 4);
  });
  const u = (name: string) => gl.getUniformLocation(prog, name);
  const uRes = u("res");
  const uT = u("t");
  const uDpr = u("dpr");
  const uPost = u("post");
  const uPostHW = u("postHW");
  const uHeadC = u("headC");
  const uHeadR = u("headR");
  const uLamp = u("lamp");
  const uPhotoW = u("photoW");
  return (time) => {
    const dpr = fitCanvas(gl, canvas);
    const box = photoBox(geom, canvas);
    const px = (f: [number, number]) => [
      box.x + f[0] * box.w,
      box.y + f[1] * box.h,
    ];
    const [ptx, pty] = px(geom.postTop);
    const [pbx, pby] = px(geom.postBottom);
    const [hx, hy] = px(geom.head);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.uniform2f(uRes, canvas.clientWidth, canvas.clientHeight);
    gl.uniform1f(uT, time);
    gl.uniform1f(uDpr, dpr);
    gl.uniform4f(uPost, ptx, pty, pbx, pby);
    gl.uniform2f(uPostHW, geom.postHalfWidth[0], geom.postHalfWidth[1]);
    gl.uniform2f(uHeadC, hx, hy);
    gl.uniform1f(uLamp, lampOn(canvas));
    gl.uniform1f(uPhotoW, box.w);
    gl.uniform2f(
      uHeadR,
      geom.headRadius[0] * box.w,
      geom.headRadius[1] * box.h,
    );
    // The count is tuned for a desktop hero; smaller canvases draw a
    // prefix so the snow keeps the same density per screen area.
    const area = canvas.clientWidth * canvas.clientHeight;
    const share = Math.min(1, Math.max(0.25, area / (1600 * 1000)));
    gl.drawArrays(gl.POINTS, 0, Math.round(n * share));
  };
}

/** With `?fxdebug` in the URL, a 2D overlay draws the geometry the snow is
 *  using (photo box, post, head, lit cone) over the hero, follows the pointer
 *  with its position in photo fractions, and on click drops a numbered pin
 *  and logs the fraction, so the constants can be read straight off the
 *  page. Returns a teardown. */
function mountFxDebug(canvas: HTMLCanvasElement, geom: OrbitGeometry) {
  const el = document.createElement("canvas");
  el.style.cssText =
    "position:absolute;inset:0;width:100%;height:100%;z-index:10;cursor:crosshair";
  canvas.insertAdjacentElement("afterend", el);
  const ctx = el.getContext("2d");
  if (!ctx) return () => el.remove();
  const pins: [number, number][] = [];
  let pointer: [number, number] | null = null;

  const toFrac = (e: PointerEvent): [number, number] => {
    const box = photoBox(geom, canvas);
    const r = el.getBoundingClientRect();
    return [
      +((e.clientX - r.left - box.x) / box.w).toFixed(4),
      +((e.clientY - r.top - box.y) / box.h).toFixed(4),
    ];
  };
  const render = () => {
    const dpr = window.devicePixelRatio || 1;
    const cw = el.clientWidth;
    const ch = el.clientHeight;
    el.width = Math.round(cw * dpr);
    el.height = Math.round(ch * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cw, ch);
    const box = photoBox(geom, canvas);
    const px = (f: [number, number]) =>
      [box.x + f[0] * box.w, box.y + f[1] * box.h] as [number, number];
    ctx.lineWidth = 1;
    ctx.font = "12px ui-monospace, monospace";
    // Photo box.
    ctx.strokeStyle = "rgba(255,255,255,0.35)";
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(box.x, box.y, box.w, box.h);
    ctx.setLineDash([]);
    // Post, with its half-width at each end.
    const [tx, ty] = px(geom.postTop);
    const [bx, by] = px(geom.postBottom);
    const hwT = geom.postHalfWidth[0] * box.w;
    const hwB = geom.postHalfWidth[1] * box.w;
    ctx.strokeStyle = "#39ff88";
    ctx.beginPath();
    ctx.moveTo(tx - hwT, ty);
    ctx.lineTo(bx - hwB, by);
    ctx.lineTo(bx + hwB, by);
    ctx.lineTo(tx + hwT, ty);
    ctx.closePath();
    ctx.stroke();
    // Head ellipse.
    const [hx, hy] = px(geom.head);
    ctx.strokeStyle = "#ffe14d";
    ctx.beginPath();
    ctx.ellipse(
      hx,
      hy,
      geom.headRadius[0] * box.w,
      geom.headRadius[1] * box.h,
      0,
      0,
      Math.PI * 2,
    );
    ctx.stroke();
    // Lit cone at the post's depth: side = 0.03 + down * 0.6 (photo widths).
    ctx.strokeStyle = "rgba(255,120,220,0.8)";
    ctx.beginPath();
    for (const sgn of [-1, 1]) {
      ctx.moveTo(hx + sgn * 0.03 * box.w, hy);
      const down = (ch - hy) / box.w;
      ctx.lineTo(hx + sgn * (0.03 + down * 0.6) * box.w, ch);
    }
    ctx.stroke();
    // Pins and pointer readout.
    ctx.fillStyle = "#fff";
    pins.forEach((f, i) => {
      const [x, y] = px(f);
      ctx.beginPath();
      ctx.arc(x, y, 4, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillText(`${i + 1} [${f[0]}, ${f[1]}]`, x + 8, y - 6);
    });
    if (pointer) {
      const [x, y] = px(pointer);
      ctx.fillText(`[${pointer[0]}, ${pointer[1]}]`, x + 12, y + 16);
    }
  };
  const onMove = (e: PointerEvent) => {
    pointer = toFrac(e);
    render();
  };
  const onClick = (e: PointerEvent) => {
    const f = toFrac(e);
    pins.push(f);
    console.warn("fxdebug pin", JSON.stringify(f), JSON.stringify(pins));
    render();
  };
  el.addEventListener("pointermove", onMove);
  el.addEventListener("pointerdown", onClick);
  const ro = new ResizeObserver(render);
  ro.observe(el);
  render();
  return () => {
    ro.disconnect();
    el.remove();
  };
}

export function mountFx(
  canvas: HTMLCanvasElement,
  geom: OrbitGeometry,
): () => void {
  const gl = canvas.getContext("webgl", {
    alpha: true,
    premultipliedAlpha: true,
    antialias: false,
  });
  if (!gl) return () => {};

  let draw: Draw;
  try {
    draw = buildOrbit(gl, canvas, geom);
  } catch (e) {
    console.warn("landing fx", e);
    return () => {};
  }
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  const unmountDebug = new URLSearchParams(location.search).has("fxdebug")
    ? mountFxDebug(canvas, geom)
    : () => {};

  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(2.0);
    return unmountDebug;
  }

  // First frame right away, then animate only while on screen and the tab
  // is visible.
  draw(0);
  let frame = 0;
  let visible = false;
  const start = performance.now();
  const loop = (now: number) => {
    draw((now - start) / 1000);
    frame = requestAnimationFrame(loop);
  };
  const sync = () => {
    const shouldRun = visible && document.visibilityState === "visible";
    if (shouldRun && !frame) frame = requestAnimationFrame(loop);
    if (!shouldRun && frame) {
      cancelAnimationFrame(frame);
      frame = 0;
    }
  };
  const observer = new IntersectionObserver((entries) => {
    visible = entries.some((e) => e.isIntersecting);
    sync();
  });
  observer.observe(canvas);
  document.addEventListener("visibilitychange", sync);

  // Note: the context is deliberately not lost here. React StrictMode mounts
  // twice on the same element, and a lost context cannot be reused.
  return () => {
    unmountDebug();
    observer.disconnect();
    document.removeEventListener("visibilitychange", sync);
    if (frame) cancelAnimationFrame(frame);
  };
}
