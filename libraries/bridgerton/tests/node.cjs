const assert = require('node:assert/strict');
const { Counter, Snapshot } = require('../generated/node/bridge_fixture.js');

(async () => {
  const api = require('../generated/node/bridge_fixture.js');
  assert.equal(api.echo_text('語 🦀'), '語 🦀');
  assert.equal(await api.text_later('owned across suspension'), 'owned across suspension');
  assert.equal(await api.sum_later(new Uint32Array([1, 2, 3])), 6);
  assert.deepEqual(api.conditional_record({present: 42}), {present: 42});
  assert.deepEqual(api.conditional_enum({Tuple: 7}), {Tuple: 7});
  assert.deepEqual(api.keyword_value({type: "keyword"}), {type: "keyword"});
  assert.deepEqual(api.generic_values({value: [42, undefined]}), {value: [42, undefined]});
  assert.deepEqual(api.configured_values({value: [18446744073709551615n, null]}), {value: [18446744073709551615n, null]});
  const terms = [{text: '語', gloss: undefined}];
  assert.deepEqual(await api.echo_terms(terms), terms);
  assert.equal(await api.echo_terms(), undefined);
  assert.deepEqual(api.echo_numbers(new Uint32Array([0, 4294967295])), new Uint32Array([0, 4294967295]));
  const selected = new api.Selection();
  assert.equal(selected.selected(), 7);
  assert.equal(selected.hidden, undefined);
  selected.free();
  const created = await Counter.create(42);
  assert.equal(created.value(), 42);
  created.free();
  const counter = new Counter();
  assert.equal(counter.platform_value(), 'web');
  assert.equal(counter.conditional_getter, 17);
  for (const method of ['nonexistent', 'nested_disabled', 'conditional_skip']) {
    assert.equal(counter[method], undefined);
  }
  console.log(await require('./values.js').testValues(counter));
  assert.equal(counter.live_counters(), 1);
  let observed;
  counter.observe(value => {
    observed = value;
    assert.equal(counter.value(), value);
    counter.clear_observer();
  });
  assert.equal(counter.add(7), 7);
  assert.equal(observed, 7);
  assert.equal(counter.label(), 'Yap 語 — 7');
  const snapshot = counter.snapshot();
  const laterSnapshot = await counter.snapshot_later();
  const made = await Snapshot.create(91);
  assert.ok(snapshot instanceof Snapshot && laterSnapshot instanceof Snapshot);
  assert.equal(snapshot.value(), 7);
  assert.equal(laterSnapshot.value(), 7);
  assert.equal(made.value(), 91);
  assert.equal(made.doubled, 182);
  assert.equal(made.rust_only, undefined);
  snapshot.free();
  laterSnapshot.free();
  made.free();

  assert.throws(() => counter.fail(), error => String(error).includes('日本語'));

  const token = new AbortController();
  assert.equal(await counter.add_later(3, 10, token.signal), 10);
  assert.equal(counter.active_operations(), 0);

  const cancelledToken = new AbortController();
  const pending = counter.add_later(100, 60, cancelledToken.signal);
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal(counter.active_operations(), 1);
  cancelledToken.abort();
  cancelledToken.abort();
  await assert.rejects(pending, error => String(error).includes('operation aborted'));
  assert.equal(counter.active_operations(), 0);
  assert.equal(counter.value(), 10);

  await Promise.all(Array.from({ length: 24 }, (_, i) => counter.add_later(1, i % 5, token.signal)));
  assert.equal(counter.value(), 34);
  assert.equal(counter.active_operations(), 0);

  const temporary = new Counter();
  assert.equal(counter.live_counters(), 2);
  temporary.free();
  assert.equal(counter.live_counters(), 1);

  counter.free();
  console.log('PASS: real wasm-bindgen sync, reentrant callbacks, UTF-8, errors, async, cancellation, interleaving, and destruction');
})().catch(error => { console.error(error); process.exitCode = 1; });
