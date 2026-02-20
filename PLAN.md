# Plan: Consolidate phrasebooks in ConsolidatedLanguageData

## Problem
`ConsolidatedLanguageData` has two phrasebook fields:
- `phrasebook: BTreeMap<String, PhrasebookEntry>` — MWE phrases (string-keyed)
- `gram_phrasebook: Vec<(Gram<String>, PhrasebookDefinitionEntry)>` — learned grams

Both are already unified in `phrase_data_map` in main.rs. We should just pass that through.

## Changes

### 1. `language-utils/src/lib.rs` — ConsolidatedLanguageData
Replace both fields with one:
```rust
pub phrasebook: BTreeMap<Gram<String>, PhrasebookDefinitionEntry>,
```
Update `intern()` to intern display strings from `entry.target_language_multi_word_term`.

### 2. `generate-data/src/main.rs`
- Remove separate `phrasebook: BTreeMap<String, PhrasebookEntry>` construction (~line 472-491)
- Derive unified phrasebook from `phrase_data_map` after it's built:
  ```rust
  let phrasebook: BTreeMap<Gram<String>, PhrasebookDefinitionEntry> = phrase_data_map
      .iter()
      .map(|(gram, data)| (gram.clone(), data.entry.clone()))
      .collect();
  ```
- Remove `gram_phrasebook` from ConsolidatedLanguageData construction

### 3. `language-utils/src/language_pack.rs` — LanguagePack::new()
Build both lookups from the single source:
- `phrasebook: BTreeMap<Spur, PhrasebookEntry>` — use `entry.target_language_multi_word_term` as key
- `gram_definitions` phrasebook portion — iterate `language_data.phrasebook` instead of `language_data.gram_phrasebook`

### No changes needed to consumers
`dictionary.rs`, `challenge.rs` etc. use `LanguagePack.phrasebook` which stays the same type.
