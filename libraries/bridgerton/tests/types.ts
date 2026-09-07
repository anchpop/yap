import { Counter, Snapshot, type Card, type ReviewState, type Term } from '../generated/node/bridge_fixture';
const counter = new Counter();
const term: Term = { text: '語', gloss: undefined };
const state: ReviewState = { Learning: { steps: 1, due: undefined } };
const card: Card = { id: 0, term, alternatives: [], state, tags: [], starred: false };
const result: Card = counter.revise_card(card, state);
const asynchronous: Promise<Card> = counter.card_later(card, new AbortController().signal);
const collection: (Card | null)[] = counter.echo_cards([null, result]);
const nested: Promise<(Card | null)[][]> = counter.echo_nested([[null, card], []]);
const alias: Card = counter.echo_alias(card);
const optional: Card | null | undefined = counter.maybe_card();
const cards: Card[] = counter.cards;
// @ts-expect-error collection getters aren't functions
counter.cards();
// @ts-expect-error nested collection elements must be cards or null
counter.echo_nested([[true]]);
// @ts-expect-error payload is required for this variant
const missing: ReviewState = 'Learning';
// @ts-expect-error unknown field type
counter.revise_card({ ...card, starred: 'yes' }, state);
// @ts-expect-error generated methods must not silently return any
const wrong: number = counter.sample_card();
// @ts-expect-error async method returns a Promise
const synchronous: Card = counter.card_later(card, new AbortController().signal);
void [asynchronous, collection, missing, wrong, synchronous];

const snapshot: Snapshot = counter.snapshot();
const laterSnapshot: Promise<Snapshot> = counter.snapshot_later();
const factory: Promise<Counter> = Counter.create(42);
const factoryOnly: Promise<Snapshot> = Snapshot.create(91);
const doubled: number = snapshot.doubled;
// @ts-expect-error getters are properties, not methods
snapshot.doubled();
// @ts-expect-error inferred object returns must not silently become any
const wrongObject: Card = counter.snapshot();
// @ts-expect-error explicitly skipped methods remain Rust-only
snapshot.rust_only(1);
// @ts-expect-error this object exposes a factory, not a public constructor
new Snapshot();
void [snapshot, laterSnapshot, factory, factoryOnly, wrongObject];

const wire = counter.echo_wire_state({ value: { kind: 'in_progress', payload: { completedCount: 3 } } });
const wireId: number = counter.echo_wire_id(42);
const optionalWire: typeof wire | undefined = counter.maybe_wire_state(null);
void optionalWire;
// @ts-expect-error Serde determines the tagged enum's web spelling
counter.echo_wire_state({ value: { InProgress: { completed: 3 } } });
// @ts-expect-error only the explicitly serde-transparent type is unwrapped
counter.echo_wire_state({ kind: 'not_started' });
void [wire, wireId];

import { echo_terms, echo_numbers } from '../generated/node/bridge_fixture';
const aliasedTerms: Promise<Term[] | undefined> = echo_terms();
const aliasedNumbers: Uint32Array = echo_numbers(new Uint32Array([42]));
// @ts-expect-error alias retains its element type
void echo_terms([false]);
void [aliasedTerms, aliasedNumbers];

import { generic_values, configured_values, type Envelope, type ConfiguredEnvelope } from '../generated/node/bridge_fixture';
const genericSerde: Envelope<(number | undefined)[]> = generic_values({value: [42, undefined]});
const configuredSerde: ConfiguredEnvelope<(bigint | null)[]> = configured_values({value: [42n, null]});
// @ts-expect-error serde vectors in records are arrays, not typed arrays
const wrongSerde: Envelope<Uint32Array> = generic_values({value: [42]});
void [genericSerde, configuredSerde, wrongSerde];
