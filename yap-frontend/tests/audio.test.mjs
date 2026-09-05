import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

const source = ts.transpileModule(
  readFileSync(new URL("../src/lib/utils.ts", import.meta.url), "utf8"),
  { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } },
).outputText;
const flush = () => new Promise((resolve) => setImmediate(resolve));
const bytes = { bytes: new Uint8Array([1, 2, 3]) };

function playback({ load = async () => bytes, playError } = {}) {
  const elements = [];
  const invalidated = [];
  const revoked = [];
  const fetched = [];
  let authCalls = 0;
  class Audio {
    constructor(src) { this.src = src; this.playCalls = 0; elements.push(this); }
    set src(value) {
      this.url = value;
      if (!value) this.onerror?.();
    }
    pause() {}
    play() {
      this.playCalls++;
      return playError ? Promise.reject(playError) : Promise.resolve();
    }
  }
  const exports = {};
  vm.runInNewContext(source, {
    exports, Audio, Blob, DOMException, Error,
    console: { error() {} },
    URL: {
      createObjectURL: () => `blob:${elements.length}`,
      revokeObjectURL: (url) => revoked.push(url),
    },
    require: (path) => {
      if (path === "./pure") return {};
      assert.ok(path.endsWith("/pkg"));
      return {
        get_audio: (...args) => { fetched.push("cached"); return load(...args); },
        get_temp_audio: (...args) => { fetched.push("temporary"); return load(...args); },
        invalidate_audio_cache: async (request) => { invalidated.push(request); },
      };
    },
  });
  return {
    play: (options) => exports.playAudio({}, undefined, () => authCalls++, options),
    elements, invalidated, revoked, fetched,
    authCalls: () => authCalls,
  };
}

test("cached and temporary playback release their audio resources on completion", async () => {
  const p = playback();
  for (const temporary of [false, true]) {
    const done = p.play({ temporary });
    await flush();
    const audio = p.elements.at(-1);
    assert.equal(audio.playCalls, 1);
    audio.onended();
    await done;
    assert.equal(audio.onended, null);
    assert.equal(audio.onerror, null);
  }
  assert.deepEqual(p.fetched, ["cached", "temporary"]);
  assert.equal(p.revoked.length, 2);
  assert.equal(p.invalidated.length, 0);
});

test("only cached playback failures invalidate the cache; autoplay denial does not", async () => {
  for (const [temporary, name, expected] of [
    [false, "NotSupportedError", 1],
    [true, "NotSupportedError", 0],
    [false, "NotAllowedError", 0],
  ]) {
    const p = playback({ playError: new DOMException("play failed", name) });
    await assert.rejects(p.play({ temporary }), { name });
    assert.equal(p.invalidated.length, expected);
    assert.equal(p.revoked.length, 1);
  }
});

test("abort during audio preparation prevents playback without invalidating the clip", async () => {
  const ready = Promise.withResolvers();
  const controller = new AbortController();
  const p = playback();
  const rejected = assert.rejects(p.play({
    signal: controller.signal,
    onAudioElement: () => ready.promise,
  }), { name: "AbortError" });
  await flush();
  controller.abort();
  await rejected;
  ready.resolve();
  await flush();
  assert.equal(p.elements[0].playCalls, 0);
  assert.equal(p.invalidated.length, 0);
  assert.equal(p.revoked.length, 1);
});

test("late results from a superseded download cannot interrupt newer playback", async () => {
  for (const fails of [false, true]) {
    const oldDownload = Promise.withResolvers();
    let calls = 0;
    const p = playback({ load: () => ++calls === 1 ? oldDownload.promise : Promise.resolve(bytes) });
    const interrupted = assert.rejects(p.play(), { name: "AbortError" });
    const current = p.play();
    await interrupted;
    await flush();
    if (fails) oldDownload.reject(new Error("old download failed"));
    else oldDownload.resolve(bytes);
    await flush();
    assert.equal(p.elements.length, 1);
    assert.equal(p.elements[0].playCalls, 1);
    p.elements[0].onended();
    await current;
    assert.equal(p.invalidated.length, 0);
  }
});

test("interrupting active playback rejects once and detaches its abort listener", async () => {
  const controller = new AbortController();
  const p = playback();
  const interrupted = assert.rejects(p.play({ signal: controller.signal }), { name: "AbortError" });
  await flush();
  const current = p.play();
  await interrupted;
  await flush();
  controller.abort();
  assert.equal(p.revoked.length, 1);
  assert.equal(p.invalidated.length, 0);
  p.elements[1].onended();
  await current;
  assert.equal(p.revoked.length, 2);
});

test("download authentication failures reach the caller without invalidating cached audio", async () => {
  const p = playback({ load: async () => { throw "HTTP 400"; } });
  await assert.rejects(p.play(), (error) => error === "HTTP 400");
  assert.equal(p.authCalls(), 1);
  assert.equal(p.elements.length, 0);
  assert.equal(p.invalidated.length, 0);
});
