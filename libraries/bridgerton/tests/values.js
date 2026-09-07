// Shared integration test in Node and Chromium. No runtime-specific dependencies.
async function testValues(counter) {
  const check = (condition, message) => { if (!condition) throw Error(message); };
  const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const rejects = fn => { let rejected = false; try { fn(); } catch { rejected = true; } check(rejected, 'invalid input must throw'); };
  const beforeBadCall = counter.live_counters();
  rejects(() => counter.consume_with_card({}, counter.optional_object(true)));
  check(counter.live_counters() === beforeBadCall, 'failed value decoding releases an already-transferred object');
  rejects(() => counter.fail_value_output());
  let valueFailure;
  try { await counter.fail_value_output_later(); } catch (error) { valueFailure = String(error); }
  check(valueFailure.includes('intentional Tsify serialization failure'), 'Tsify conversion failure is recoverable');
  counter.value(); // The receiver borrow was released on both conversion failures.
  const card = counter.sample_card();
  const wire = { value: { kind: 'in_progress', payload: { completedCount: 3 } } };
  check(equal(counter.echo_wire_state(wire), wire), 'Tsify preserves explicit Serde tags, names, and generic records');
  check(equal(counter.echo_wire_state({ value: { kind: 'not_started' } }), { value: { kind: 'not_started' } }), 'unit variant tag');
  check(counter.echo_wire_id(42) === 42, 'only serde transparent unwraps a record');
  check(counter.maybe_wire_state(null) === undefined && counter.maybe_wire_state(undefined) === undefined, 'optional generic outputs are undefined');
  check(equal(counter.maybe_wire_state(wire), wire), 'optional generic value');

  rejects(() => counter.fail_encoding());
  let encodingError;
  try { await counter.fail_encoding_later(); } catch (error) { encodingError = String(error); }
  check(encodingError === 'intentional encoding failure', 'async output conversion error');
  // Node and Chromium later free this Counter, which detects a stranded borrow.
  check(equal(counter.echo_alias(card), card), 'aliased value conversion');
  check(equal(counter.maybe_card(card), card), 'optional value input/output');
  check(counter.maybe_card(null) == null && counter.maybe_card(undefined) == null, 'nullable and omitted value');
  check(equal(counter.cards, [card]), 'collection getter');
  check(equal(await counter.echo_nested([[null, card], []]), [[null, card], []]), 'async nested nullable collections');
  check(card.term.text === '語 🦀' && card.term.gloss === 'language', 'nested UTF-8 record');
  check(equal(card.alternatives, [{ text: '言葉' }]), 'optional record');
  check(card.state === 'New' && equal(card.tags, ['日本語', '']), 'enum and array');
  const state = { Learning: { steps: 4294967295, due: undefined } };
  const revised = counter.revise_card(card, state);
  check(revised.id === card.id + 1 && equal(revised.state, state) && revised.starred, 'two value inputs');
  check(card.state === 'New' && !card.starred, 'owned copy');
  check(equal(counter.echo_cards([null, revised, card]), [null, revised, card]), 'nested collections');
  check(equal(counter.echo_cards([]), []), 'empty collection');
  for (const state of ['New', { Learning: { steps: 0, due: 'tomorrow' } }, { Known: '語' }, { Pair: [7, false] }]) {
    check(equal(counter.echo_state(state), state), 'enum payload forms');
  }
  check(counter.rename_card(card, 'a\0b').term.text === 'a\0b', 'NUL and String input');
  counter.observe_three((number, text, flag) => check(number === 42 && text === 'three' && flag, 'three-argument callback'));
  const indices = counter.echo_indices(new Uint32Array([0, 42, 4294967295]));
  check(indices instanceof Uint32Array && indices[2] === 4294967295, 'numeric vector ABI stays a typed array');
  const bytes = counter.echo_bytes(new Uint8Array([0, 128, 255]));
  check(bytes instanceof Uint8Array && bytes.join(',') === '0,128,255', 'byte buffer ABI');
  const nestedBytes = counter.nested_bytes();
  check(nestedBytes.every(bytes => bytes instanceof Uint8Array) && nestedBytes[0][2] === 255 && nestedBytes[1].length === 0, 'nested byte buffers');
  const moved = counter.optional_object(true);
  const alias = moved;
  check(counter.consume(moved) === 0, 'owned object input');
  rejects(() => alias.value());
  check(counter.consume_optional(undefined) == null, 'absent object input');
  check(counter.consume_optional(counter.optional_object(true)) === 0, 'optional object input');
  check(await counter.consume_later(counter.optional_object(true)) === 0, 'async owned object input');
  let savedObject;
  counter.emit_object(object => { savedObject = object; counter.value(); });
  check(savedObject.value() === 0, 'callback object retained');
  check(counter.consume(savedObject) === 0, 'callback object consumed');
  counter.emit_objects((first, label, last) => {
    check(label === 'objects' && counter.consume(first) === 0, 'mixed object callback and reentry');
    savedObject = last;
  });
  check(savedObject.value() === 0, 'second callback object retained');
  savedObject.free();
  const objects = counter.object_list();
  check(objects.map(value => value.value()).join(',') === '1,2,3', 'object array');
  objects.forEach(value => value.free());
  const present = counter.optional_object(true);
  check(present.value() === 0 && counter.optional_object(false) == null, 'optional object');
  present.free();
  const nestedObjects = counter.nested_objects();
  check(nestedObjects.length === 2 && nestedObjects[0] == null && nestedObjects[1].value() === 0, 'nested object array');
  nestedObjects[1].free();
  const laterObjects = await counter.objects_later();
  check(laterObjects.length === 3, 'async object array');
  laterObjects.forEach(value => value.free());
  check(JSON.stringify(counter.nested_values()) === '[["one"],[],["two"]]', 'nested value arrays');

  const controller = new AbortController();
  const signal = controller.signal;
  let listeners = 0;
  const add = signal.addEventListener.bind(signal);
  const remove = signal.removeEventListener.bind(signal);
  signal.addEventListener = (...args) => { listeners++; return add(...args); };
  signal.removeEventListener = (...args) => { listeners--; return remove(...args); };
  check(await counter.abortable_wait(1, signal) === false, 'normal signal completion');
  check(listeners === 0, 'completion removes abort listener');
  const waits = [counter.abortable_wait(60000, signal), counter.abortable_wait(60000, signal)];
  await new Promise(resolve => setTimeout(resolve, 5));
  controller.abort();
  check((await Promise.all(waits)).every(Boolean), 'browser controller aborts all waiters');
  check(listeners === 0, 'abort removes listeners');
  check(await counter.abortable_wait(60000, signal), 'pre-aborted signal');
  check(await counter.abortable_wait(1) === false, 'optional signal');

  const token = new AbortController();
  const pending = counter.card_later(revised, token.signal);
  // wasm-bindgen starts async exports on a microtask. Wait for deserialization
  // and the first Rust suspension before testing independence from the JS object.
  await Promise.resolve();
  check(counter.active_operations() === 1, 'Rust data future has started');
  revised.term.text = 'changed after call';
  const later = await pending;
  check(later.term.text === '語 🦀' && equal(later.state, { Known: 'remembered' }), 'suspended future owns decoded data');
  const cancelled = new AbortController();
  cancelled.abort();
  let error;
  try { await counter.card_later(card, cancelled.signal); } catch (e) { error = e; }
  check(error instanceof Error && error.message === 'operation aborted', 'data cancellation');
  // Errors are Error objects; typed errors carry a detail shaped like Swift's.
  try { counter.io_result(); } catch (e) { error = e; }
  check(error instanceof Error && error.detail.kind === 'NotFound' && error.message.includes('missing test pack'), 'io error detail');
  try { counter.source_result(); } catch (e) { error = e; }
  check(error instanceof Error && error.detail.type === 'Read' && error.detail.value === 'opaque source', 'typed error detail');
  try { counter.typed_result(true); } catch (e) { error = e; }
  check(error instanceof Error && error.detail.InvalidAnswer.term === '語' && error.detail.InvalidAnswer.attempts === 2, 'value error detail');
  try { counter.string_error(); } catch (e) { error = e; }
  check(error instanceof Error && error.message === 'plain error' && error.detail === undefined, 'string error');
  rejects(() => counter.revise_card({ ...card, term: { ...card.term, text: '' } }, 'New'));
  rejects(() => counter.echo_state({ Unknown: {} }));
  rejects(() => counter.echo_state({ Pair: ['wrong', true] }));
  rejects(() => counter.echo_cards([{ ...card, id: -1 }]));
  rejects(() => counter.echo_cards([{ ...card, starred: 'true' }]));
  rejects(() => counter.echo_cards([{ ...card, term: {} }]));
  check(equal(counter.echo_cards([{ ...card, extraUIProperty: true }]), [card]), 'extra JS properties are ignored');

  return 'PASS: generated JS records/enums, nested arrays/options, sync/async roundtrips, owned inputs, and malformed values';
}
if (typeof module !== 'undefined') module.exports = { testValues };
