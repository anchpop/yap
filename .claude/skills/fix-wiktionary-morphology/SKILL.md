---
name: fix-wiktionary-morphology
description: Fix a Wiktionary morphology parsing issue for a specific verb or word. The user describes the problem (e.g. missing conjugation form), and this skill walks through the standard workflow to diagnose and fix it. Run with a description of the issue (e.g. /fix-wiktionary-morphology "boire missing second person singular").
---

# Fix Wiktionary Morphology Parsing Issue

When a user reports that a word's morphology data is wrong or missing on the dictionary site (yap.town/d/), follow this standard workflow to diagnose and fix it.

## Architecture Overview

The morphology data flows through a multi-stage pipeline:

### Stage 1: Wiktionary HTML Parsing (`generate-data/src/wiktionary_conjugations.rs`)

This ~3400-line file contains language-specific modules that fetch and parse Wiktionary pages:

- **6 language modules**: `french` (~lines 129-843), `spanish` (~843-1478), `german` (~1478-2261), `portuguese` (~2261-2808), `italian` (~2808-3402), `english` (~3402+)
- Each module defines a conjugation struct (e.g., `FrenchVerbConjugation`) with fixed-size arrays for each tense (e.g., `indicative_present: [String; 6]` for je/tu/il/nous/vous/ils)
- Parsing uses the `scraper` crate with CSS selectors to extract forms from Wiktionary's HTML conjugation tables
- Key CSS selectors for French: `th.roa-indicative-left-rail`, `th.roa-subjunctive-left-rail`, `th.roa-imperative-left-rail`
- Each `<td>` cell in a tense row contains the conjugated form as an `<a>` link or `<strong class="selflink">`
- Noun gender parsing is also handled here (e.g., `parse_french_noun_gender`)
- **Caching**: Pages are cached to `.cache/wiktionary/{language}/{word}.html` with 100ms rate limiting
- **Reflexive verb fallback**: French reflexive verbs (e.g., "s'échapper") fall back to the base form ("échapper")

### Stage 2: Morphology Feature Generation (`generate-data/src/morphology_analysis.rs`)

Inside `pub mod wiktionary_morphology` (~line 462), each language has a `conjugation_to_morphology` function that converts parsed conjugation arrays into `BTreeMap<Heteronym<String>, Vec<Morphology>>`:

- Iterates through each position in the conjugation arrays
- Maps array indices to `Person` (First/Second/Third) and `Number` (Singular/Plural) using parallel arrays
- Each form gets a `Morphology` struct with: `gender`, `number`, `politeness`, `tense`, `person`, `case`, `mood`
- **Important**: When the same surface form appears at multiple array positions (e.g., "bois" at indices 0 and 1), both morphologies are pushed into the same `Vec` via the `add_morph` closure
- The `Morphology` type is defined in `language-utils/src/features.rs`

The pipeline:
1. `create_morphology()` tries Wiktionary first via `wiktionary_morphology::create_morphology_from_wiktionary()`
2. Falls back to LLM (GPT-4o/GPT-5) for words not on Wiktionary
3. Results are merged into a final morphology map

### Stage 3: Data Serialization (output to `out/`)

The morphology data gets serialized into language pack archives (`.rkyv` files) in `out/{lang}_for_{native}/`. These contain the `LanguagePack` struct with gram definitions that include `Vec<Morphology>` for each word entry.

### Stage 4: Dictionary Data Generation (`generate-dictionary-data/src/main.rs`)

This binary reads `.rkyv` language pack archives and builds `ConjugationTable`s for the dictionary pages:

- Builds a `conjugation_index: FxHashMap<(lemma, pos), Vec<(word, Morphology)>>` by iterating gram_frequencies
- **Critical**: Must iterate ALL morphologies from `dict.morphology`, not just `.first()`
- Deduplicates forms by exact `(word, morphology)` pairs via `seen_forms: HashSet<(&str, &Morphology)>`
- Converts `Morphology` enum variants to lowercase strings (e.g., `Person::First` → `"first"`)
- Only attaches a conjugation table if there are more than 1 unique form
- Output struct: `ConjugationForm { word, slug, tense, mood, person, number, gender, case }` (all optional strings except word)

### Stage 5: Static Dictionary Pages (`static-site/src/pages/d/[course]/[entry].astro`)

The Astro template renders conjugation tables as person×number grids:

- Groups forms by `(mood, tense)` via `groupVerbForms()`
- For each group, filters forms that have both `person` and `number` into `gridForms`
- Renders a table: rows = `['first', 'second', 'third']`, columns = `['singular', 'plural']`
- Uses `.find()` to locate the form matching each cell: `gridForms.find(f => f.person === p && f.number === n)`
- Missing cells render as `<span class="text-muted-foreground/30">—</span>`
- Forms with person/number link to their dictionary pages via `renderFormLink()`
- Remaining forms without person/number (infinitive, participles) render as a comma-separated list

## Example Test Data

Cached Wiktionary HTML pages for testing are in `generate-data/src/wiktionary-examples/{lang_code}/`:

- **French**: `boire.txt`, `aller.txt`, `avoir.txt`, `etre.txt`, `pouvoir.txt`, `s_echapper.txt`, `maison.txt`, `jour.txt`, `artiste.txt`
- **Spanish**: `ser.txt`, `tener.txt`, `beber.txt`, `moverse.txt`, `venir.txt`, `casa.txt`, `día.txt`, `estudiante.txt`
- **German**: `sein.txt`, `haben.txt`, `gehen.txt`, `trinken.txt`, `Frau.txt`, `Mann.txt`, `Kind.txt`, `Hund.txt`, `Kreativität.txt`, `Meisterwerk.txt`
- **Portuguese**: `ser.txt`, `ter.txt`, `fazer.txt`, `ir.txt`, `comer.txt`, `casa.txt`, `dia.txt`, `pessoa.txt`, `tempo.txt`
- **Italian**: `essere.txt`, `avere.txt`, `fare.txt`, `andare.txt`, `persona.txt`, `donna.txt`, `amica.txt`, `padre.txt`, `giorno.txt`
- **English**: `be.txt`, `have.txt`, `go.txt`, `drink.txt`, `make.txt`, `do.txt`, `eat.txt`, `say.txt`, `get.txt`, `take.txt`, `know.txt`, `think.txt`, `come.txt`, `want.txt`

## Workflow

### 1. Fetch the Wiktionary page and add it to examples

If there isn't already a cached example for the word, fetch the Wiktionary HTML page and save it:

```
generate-data/src/wiktionary-examples/{lang_code}/{word}.txt
```

You can fetch it with: `curl -s "https://en.wiktionary.org/wiki/{word}" > generate-data/src/wiktionary-examples/{lang_code}/{word}.txt`

### 2. Check the Wiktionary HTML parser

In `generate-data/src/wiktionary_conjugations.rs`, find the language module and its `parse_{language}_verb_conjugation` function. Key things to examine:

- Does the HTML have the expected conjugation table structure? (Check CSS class names — Wiktionary may change them)
- Are all `<td>` cells being found? The parser iterates `tense_row.children()` looking for `<td>` elements
- For each `<td>`, does it find the form via `<a>` link or `<strong class="selflink">`?
- Does the parser expect exactly N forms and bail if the count is wrong?

### 3. Check morphology generation

In `generate-data/src/morphology_analysis.rs`, find the language's `conjugation_to_morphology` function:

- The `persons` and `numbers` arrays define the mapping from array index to morphological features
- The `add_morph` closure inserts into a `BTreeMap<Heteronym, Vec<Morphology>>` — multiple morphologies for the same word are pushed to the same Vec
- Check all tense iterations (indicative present, imperfect, past historic, future, conditional, subjunctive present/imperfect, imperative)

### 4. Check dictionary data generation

In `generate-dictionary-data/src/main.rs`:

- Find where `conjugation_index` is populated (search for `conjugation_index.entry`)
- Make sure ALL entries in `dict.morphology` are iterated (not just `.first()`)
- Check the deduplication logic — it should only deduplicate exact `(word, morphology)` pairs, NOT deduplicate by word text alone

### 5. Check the Astro template

In `static-site/src/pages/d/[course]/[entry].astro`:

- The `gridForms.find()` call must be able to find the form for each person/number combination
- If data is correct but display is wrong, fix the template

### 6. Write tests

Always write tests that assert the exact expected output:

1. **Parser test** in `generate-data/src/wiktionary_conjugations.rs` (in the language's `tests` module): Assert that the parsed conjugation array contains the expected forms at the right indices.

2. **Morphology test** in `generate-data/src/morphology_analysis.rs` (add a `#[cfg(test)] mod tests` in the language module): Assert that the morphology map contains all expected person/number/tense/mood entries for the word.

3. **Integration test** in `generate-dictionary-data/src/main.rs` (in the `tests` module, marked `#[ignore]` since it needs `.rkyv` data files): Assert that the final `ConjugationTable` contains the expected forms with correct person/number.

### 7. Verify

```bash
# Parser and morphology tests (run from repo root)
cargo test --package generate-data {test_name}

# Integration test (needs .rkyv data files in out/)
cargo test --package generate-dictionary-data {test_name} -- --include-ignored

# Lint and format
cargo fmt
cargo clippy --package generate-data --package generate-dictionary-data
```

## Common Issues

- **Same surface form for multiple persons**: The word appears in multiple conjugation positions (e.g., French "bois" = 1st AND 2nd person singular). The parser handles this correctly (both cells in the HTML exist), but downstream code may incorrectly deduplicate or only take the first morphology entry.
- **Reflexive verbs**: French reflexive verbs like "s'échapper" need fallback to the base form "échapper" for Wiktionary lookup. The fetch code handles `s'` and `s\u{2019}` prefixes.
- **Defective verbs**: Some verbs lack certain forms (e.g., "pouvoir" has no imperative). The parser returns `None`/`Option` for these.
- **Auxiliary verbs**: Verbs like "être" and "avoir" are used as both `Verb` and `Aux` POS — morphology should be generated for both.
- **Noun gender**: Gender parsing uses `span.gender > abbr` with title attributes. Some nouns have gender "by sense" qualifiers (e.g., "artiste", "estudiante").
- **Compound tenses**: Only simple tenses are parsed. Compound tenses (passé composé, plus-que-parfait, etc.) are derived from auxiliary + past participle and aren't stored directly.
