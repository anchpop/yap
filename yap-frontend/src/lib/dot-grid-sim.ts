// A lattice of dots that a pointer can swish around like a fluid, and that
// settles back into a lattice when left alone. Pure arithmetic in CSS px;
// the worker draws it.
//
// The fluid is a velocity field on the lattice itself. A pointer splats its
// own velocity into the field, a few Jacobi steps make the field divergence
// free (that is what turns a swipe into a pair of eddies), and the field
// decays. Each dot is dragged along by the field and pulled by a spring to
// the lattice point it owns. Dots and points are matched one to one, and the
// matching is improved every frame by swapping neighbours where that shortens
// the way home, so a dot carried around an eddy settles into the nearest
// free point instead of flying back to where it started.

/** Lattice pitch, CSS px. */
export const SPACING = 26;
/** Spring pull to the owned point (1/s²). */
const STIFFNESS = 22;
/** How quickly a dot takes on the fluid's velocity (1/s). */
const COUPLING = 8;
/** The matching scores a dot by where it will be this far ahead (s), so a
 *  dot in flight claims the point it is heading for. */
const LOOKAHEAD = 0.25;
/** Damping on top of the coupling, so the spring is about critical. */
const EXTRA_DAMPING = 2 * Math.sqrt(STIFFNESS) - COUPLING;
/** The fluid's decay (1/s) and diffusion (1/s). */
const FIELD_DECAY = 1.1;
const FIELD_DIFFUSION = 2.5;
/** Pointer splat: gaussian radius in CSS px, and how much of the pointer's
 *  velocity a cell takes on per pointer sample. */
const SPLAT_SIGMA = 60;
const SPLAT_TAKE = 0.3;
const SPLAT_MAX_SPEED = 1400;
const PROJECTION_STEPS = 24;
/** Below these, everything is snapped home and the sim reports rest. */
const REST_SPEED = 1.5;
const REST_DISTANCE = 0.1;
const REST_FIELD = 0.8;
/** A swap has to gain this many px² to happen, so ties do not flicker. */
const SWAP_MARGIN = 1;

export class DotGridSim {
  readonly width: number;
  readonly height: number;
  readonly cols: number;
  readonly rows: number;
  readonly count: number;
  private readonly originX: number;
  private readonly originY: number;
  /** Dot positions and velocities, interleaved x/y. */
  readonly pos: Float32Array;
  readonly vel: Float32Array;
  /** dot → lattice point and back. */
  readonly target: Int32Array;
  private readonly owner: Int32Array;
  /** The fluid on the lattice, px/s, plus projection scratch. */
  private fieldX: Float32Array;
  private fieldY: Float32Array;
  private scratchX: Float32Array;
  private scratchY: Float32Array;
  private readonly divergence: Float32Array;
  private pressure: Float32Array;
  private pressure2: Float32Array;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.cols = Math.max(2, Math.floor(width / SPACING) + 1);
    this.rows = Math.max(2, Math.floor(height / SPACING) + 1);
    this.originX = (width - (this.cols - 1) * SPACING) / 2;
    this.originY = (height - (this.rows - 1) * SPACING) / 2;
    const n = (this.count = this.cols * this.rows);
    this.pos = new Float32Array(n * 2);
    this.vel = new Float32Array(n * 2);
    this.target = new Int32Array(n);
    this.owner = new Int32Array(n);
    this.fieldX = new Float32Array(n);
    this.fieldY = new Float32Array(n);
    this.scratchX = new Float32Array(n);
    this.scratchY = new Float32Array(n);
    this.divergence = new Float32Array(n);
    this.pressure = new Float32Array(n);
    this.pressure2 = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      this.target[i] = i;
      this.owner[i] = i;
      this.pos[i * 2] = this.pointX(i);
      this.pos[i * 2 + 1] = this.pointY(i);
    }
  }

  pointX(cell: number) {
    return this.originX + (cell % this.cols) * SPACING;
  }
  pointY(cell: number) {
    return this.originY + Math.floor(cell / this.cols) * SPACING;
  }

  /** Push a pointer's velocity into the fluid around it. */
  splat(x: number, y: number, pointerX: number, pointerY: number) {
    const { cols, rows, originX, originY, fieldX, fieldY } = this;
    const speed = Math.hypot(pointerX, pointerY);
    const cap = speed > SPLAT_MAX_SPEED ? SPLAT_MAX_SPEED / speed : 1;
    const vx = pointerX * cap;
    const vy = pointerY * cap;
    const reach = SPLAT_SIGMA * 3;
    const c0 = Math.max(0, Math.floor((x - reach - originX) / SPACING));
    const c1 = Math.min(cols - 1, Math.ceil((x + reach - originX) / SPACING));
    const r0 = Math.max(0, Math.floor((y - reach - originY) / SPACING));
    const r1 = Math.min(rows - 1, Math.ceil((y + reach - originY) / SPACING));
    const inv = -1 / (2 * SPLAT_SIGMA * SPLAT_SIGMA);
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const i = r * cols + c;
        const dx = this.pointX(i) - x;
        const dy = this.pointY(i) - y;
        const w = SPLAT_TAKE * Math.exp((dx * dx + dy * dy) * inv);
        fieldX[i] += (vx - fieldX[i]) * w;
        fieldY[i] += (vy - fieldY[i]) * w;
      }
    }
  }

  /** Advance by `dt` seconds. Returns whether anything is still moving;
   *  when nothing is, the dots have been snapped onto their points. */
  step(dt: number): boolean {
    const alive = this.fieldAlive();
    if (alive) {
      this.project();
      this.relax(dt);
    } else {
      // Below notice is gone: a whisper of flow would otherwise hold the
      // dots a hair off their points forever.
      this.fieldX.fill(0);
      this.fieldY.fill(0);
    }
    const moving = this.integrate(dt);
    this.rematch();
    if (!alive && !moving) this.settle();
    return alive || moving;
  }

  /** Neighbours of a cell, with the edges as walls. */
  private around(i: number, c: number, r: number) {
    const { cols, rows } = this;
    return [
      c > 0 ? i - 1 : i,
      c < cols - 1 ? i + 1 : i,
      r > 0 ? i - cols : i,
      r < rows - 1 ? i + cols : i,
    ] as const;
  }

  /** Make the field divergence free. */
  private project() {
    const { cols, rows, fieldX, fieldY, divergence } = this;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        const [l, rt, u, d] = this.around(i, c, r);
        divergence[i] = (fieldX[rt] - fieldX[l] + fieldY[d] - fieldY[u]) * 0.5;
      }
    }
    this.pressure.fill(0);
    for (let step = 0; step < PROJECTION_STEPS; step++) {
      const p = this.pressure;
      const q = this.pressure2;
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const i = r * cols + c;
          const [l, rt, u, d] = this.around(i, c, r);
          q[i] = (p[l] + p[rt] + p[u] + p[d] - divergence[i]) * 0.25;
        }
      }
      this.pressure = q;
      this.pressure2 = p;
    }
    const p = this.pressure;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        const [l, rt, u, d] = this.around(i, c, r);
        fieldX[i] -= (p[rt] - p[l]) * 0.5;
        fieldY[i] -= (p[d] - p[u]) * 0.5;
      }
    }
  }

  /** Blur the field a little and let it die down. */
  private relax(dt: number) {
    const { cols, rows, fieldX, fieldY, scratchX, scratchY } = this;
    const blend = Math.min(1, FIELD_DIFFUSION * dt);
    const keep = Math.exp(-FIELD_DECAY * dt);
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        const [l, rt, u, d] = this.around(i, c, r);
        const ax = (fieldX[l] + fieldX[rt] + fieldX[u] + fieldX[d]) * 0.25;
        const ay = (fieldY[l] + fieldY[rt] + fieldY[u] + fieldY[d]) * 0.25;
        scratchX[i] = (fieldX[i] + (ax - fieldX[i]) * blend) * keep;
        scratchY[i] = (fieldY[i] + (ay - fieldY[i]) * blend) * keep;
      }
    }
    this.fieldX = scratchX;
    this.fieldY = scratchY;
    this.scratchX = fieldX;
    this.scratchY = fieldY;
  }

  /** Move every dot: dragged by the fluid where it stands, sprung to its
   *  point. Returns whether any dot is still moving. */
  private integrate(dt: number): boolean {
    const { cols, rows, originX, originY, pos, vel, target, fieldX, fieldY } =
      this;
    let moving = false;
    const damping = COUPLING + EXTRA_DAMPING;
    for (let i = 0; i < this.count; i++) {
      const x = pos[i * 2];
      const y = pos[i * 2 + 1];
      // Bilinear sample of the field at the dot.
      const fx = Math.min(Math.max((x - originX) / SPACING, 0), cols - 1.001);
      const fy = Math.min(Math.max((y - originY) / SPACING, 0), rows - 1.001);
      const c = Math.floor(fx);
      const r = Math.floor(fy);
      const tx = fx - c;
      const ty = fy - r;
      const i00 = r * cols + c;
      const w00 = (1 - tx) * (1 - ty);
      const w10 = tx * (1 - ty);
      const w01 = (1 - tx) * ty;
      const w11 = tx * ty;
      const flowX =
        fieldX[i00] * w00 +
        fieldX[i00 + 1] * w10 +
        fieldX[i00 + cols] * w01 +
        fieldX[i00 + cols + 1] * w11;
      const flowY =
        fieldY[i00] * w00 +
        fieldY[i00 + 1] * w10 +
        fieldY[i00 + cols] * w01 +
        fieldY[i00 + cols + 1] * w11;

      const homeX = this.pointX(target[i]);
      const homeY = this.pointY(target[i]);
      let vx = vel[i * 2];
      let vy = vel[i * 2 + 1];
      vx += (STIFFNESS * (homeX - x) + COUPLING * flowX - damping * vx) * dt;
      vy += (STIFFNESS * (homeY - y) + COUPLING * flowY - damping * vy) * dt;
      const nx = x + vx * dt;
      const ny = y + vy * dt;
      vel[i * 2] = vx;
      vel[i * 2 + 1] = vy;
      pos[i * 2] = nx;
      pos[i * 2 + 1] = ny;
      if (
        !moving &&
        (Math.abs(vx) > REST_SPEED ||
          Math.abs(vy) > REST_SPEED ||
          Math.abs(nx - homeX) > REST_DISTANCE ||
          Math.abs(ny - homeY) > REST_DISTANCE)
      )
        moving = true;
    }
    return moving;
  }

  /** Squared distance from where a dot is heading to a point. */
  farness(dot: number, cell: number) {
    const dx =
      this.pos[dot * 2] + this.vel[dot * 2] * LOOKAHEAD - this.pointX(cell);
    const dy =
      this.pos[dot * 2 + 1] +
      this.vel[dot * 2 + 1] * LOOKAHEAD -
      this.pointY(cell);
    return dx * dx + dy * dy;
  }

  /** Give two neighbouring points each other's dots where that brings the
   *  pair nearer home. */
  private trySwap(cellA: number, cellB: number) {
    const { owner, target } = this;
    const a = owner[cellA];
    const b = owner[cellB];
    const now = this.farness(a, cellA) + this.farness(b, cellB);
    const swapped = this.farness(a, cellB) + this.farness(b, cellA);
    if (swapped + SWAP_MARGIN < now) {
      owner[cellA] = b;
      owner[cellB] = a;
      target[a] = cellB;
      target[b] = cellA;
    }
  }

  /** One sweep of local improvements to the matching. */
  private rematch() {
    const { cols, rows } = this;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        if (c + 1 < cols) this.trySwap(i, i + 1);
        if (r + 1 < rows) {
          this.trySwap(i, i + cols);
          if (c + 1 < cols) this.trySwap(i, i + cols + 1);
          if (c > 0) this.trySwap(i, i + cols - 1);
        }
      }
    }
  }

  private fieldAlive(): boolean {
    const { fieldX, fieldY } = this;
    for (let i = 0; i < this.count; i++) {
      if (Math.abs(fieldX[i]) > REST_FIELD || Math.abs(fieldY[i]) > REST_FIELD)
        return true;
    }
    return false;
  }

  private settle() {
    this.fieldX.fill(0);
    this.fieldY.fill(0);
    this.vel.fill(0);
    for (let i = 0; i < this.count; i++) {
      this.pos[i * 2] = this.pointX(this.target[i]);
      this.pos[i * 2 + 1] = this.pointY(this.target[i]);
    }
  }
}
