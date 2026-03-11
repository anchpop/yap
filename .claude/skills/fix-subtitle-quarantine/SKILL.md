---
name: fix-subtitle-quarantine
description: Clean up quarantined movie subtitle files by fixing OCR errors, encoding issues, and merging split sentences. Run with a language code argument (e.g. /fix-quarantine fra). Use when the user wants to fix or clean quarantined subtitles.
---

# Fix Quarantined Subtitles

Process quarantined subtitle files one language at a time. The user will specify which language to process (e.g. `/fix-quarantine fra`).

# High level 

In  ./generate-data/data/*/sentence-sources/movies/subtitles-quarantine, i have "quarantined" movie subtitles. they are quarantined because they have a variety of issues - encoding problems, tag problems, OCR problems, subtitle group watermarks, and more. For each of these files, read the entire thing and fix all the errors by manually typing out a fixed version. In many cases, you will have to fix literally every line. In some cases, you will have to make judgement calls about how to fix an OCR issue, if the OCR converted a word into another word (especially common in italian). You should basically be rewriting every line. Yes, it's a decent amount of work, but it's worth it. Another objective is to combine lines into singlular sentences - in other words, if a sentence is split across multiple lines, rewrite it so that the sentence appears entirely in one line. (this may cause you to lose some timestamp info for the sentences that got merged - that is okay, we don't really use it for anything).Your goal is to generate basically high-quality subtitles from these medium-to-low-quality sources. It's okay to do some automated corrections first using bash or python, but you should always read every line of every file before moving on to the next, and be extra careful to not do automated corrections that make it worse rather than better (aka be conservative). If you do any scripting, copy the files first - i don't want you using git to revert them to a pre-script state. Once you've done that, the next step is to move the fixed-up file into the corresponding sentence-sources/movies/subtitles directory

# Lower level

## File Location & Format

Quarantined files are at:
```
./generate-data/data/{lang}/sentence-sources/movies/subtitles-quarantine/*.jsonl
```

Each file is named `{imdb_id}.jsonl` (e.g. `tt0230011.jsonl`). Each line is a JSON object:
```json
{"sentence":"The dialogue text.","start_ms":12000,"end_ms":15000}
```

Fields:
- `sentence`: The subtitle text (this is what you're fixing)
- `start_ms`: Start timestamp in milliseconds
- `end_ms`: End timestamp in milliseconds

## Process

For each `.jsonl` file in the quarantine directory for the specified language:

1. **Read the entire file** — every single line.
2. **If you want to do automated corrections first** (regex/script), **copy the file first** as a backup. Be conservative — only automate corrections you're 100% sure won't make things worse. Do NOT use git to revert failed automated corrections; use your backup copy.
3. **Rewrite every line.** Go through each line and produce a corrected version. This means you are producing a complete new file — not patching individual lines.
4. **Merge split sentences.** If a sentence is spread across multiple consecutive lines, combine them into one line. Use the `start_ms` from the first line and `end_ms` from the last line of the merged group. Losing intermediate timestamp info is fine.
5. **Drop irrecoverable lines.** If a line is garbled beyond recognition (mojibake, wrong language entirely, meaningless fragments), drop it.
6. **Ensure punctuation.** Every sentence should end with appropriate punctuation (period, question mark, exclamation mark, ellipsis). This applies to all languages that use punctuation (i.e., all Latin-script languages). For languages that do not use punctuation, this obviously does not apply.
7. **Write the fixed file** back to the same path in the quarantine directory.
8. **Move the file to the subtitles directory** — now that they're fixed, they can go out of the quarantine.

## Known OCR Patterns by Language

These are the most common OCR errors. Fix all instances, but use judgment — some "corrections" can be wrong (e.g., Italian `lo` is a valid word meaning "the/him/it", don't blindly change it to `Io`).

### French
- `I'` + lowercase → `l'` + lowercase (e.g., `I'homme` → `l'homme`)
- `II` → `Il` (two capital I's → capital I + lowercase l)
- `ll` at start → `Il`
- t→r corruption: `c'esr` → `c'est`, `êrre` → `être`, `rour` → `tout`, `cerre` → `cette`, `conrre` → `contre`, `enrre` → `entre`, `aurre` → `autre`, `norre` → `notre`, `vorre` → `votre`

### English
- `l'm` → `I'm`, `l'd` → `I'd`, `l'll` → `I'll`
- `lt` → `It`, `ln` → `In`, `lf` → `If` (at word boundaries)
- t→r corruption: `rhe` → `the`, `rhar` → `that`, `rhis` → `this`, `rhere` → `there`, `rhey` → `they`, `whar` → `what`, `abour` → `about`, `jusr` → `just`, `righr` → `right`, `don'r` → `don't`

### Spanish
- t→r corruption: `esrá` → `está`, `esro` → `esto`, `riene` → `tiene`, `riempo` → `tiempo`, `rambién` → `también`, `conrra` → `contra`, `nuesrro` → `nuestro`, `orro` → `otro`
- Missing `¿` and `¡` — add them where appropriate

### German
- `lch` → `Ich`, `lhr` → `Ihr`, `lhm` → `Ihm`, `lhn` → `Ihn`, `lst` → `Ist`, `ln` → `In` (at word boundaries)
- t→r corruption: `nichr` → `nicht`, `nichrs` → `nichts`, `jerzt` → `jetzt`, `harre` → `hatte`, `birer` → `bitte`, `mussr` → `musst`, `kannsr` → `kannst`

### Italian
- `II` → `Il`, `ll` → `Il` (at word boundaries)
- `ln` → `In` (at word boundaries)
- `I'` + lowercase → `l'` + lowercase (same as French)
- t→r corruption: `rurro` → `tutto`, `rurra` → `tutta`, `quesr` → `quest`, `farro` → `fatto`, `derro` → `detto`, `sraro` → `stato`, `conrro` → `contro`, `nienre` → `niente`
- `e'e'` → `zz` (bizarre OCR: `solue'ione` → `soluzione`)
- **Accent-as-apostrophe encoding**: Some files use `e'` for `è`, `a'` for `à`, `u'` for `ù`, `i'` for `ì`, `o'` for `ò` throughout. Convert these when the apostrophe is at the end of a word (not when it's a real elision like `l'estate`). Note: `perché` and `poiché` use acute accent é, most others use grave.
- **Missing initial letters**: OCR sometimes drops the first character(s), especially P: `ensavo` → `Pensavo`, `assami` → `Passami`, `orca` → `Porca`, `erciò` → `Perciò`, `referirei` → `Preferirei`. Also `l segni` → `I segni`, `l miei` → `I miei`.
- **Missing spaces**: Words sometimes get merged: `statifortunati` → `stati fortunati`, `sultrucco` → `sul trucco`, `peresercitarmi` → `per esercitarmi`, `averparlato` → `aver parlato`
- **Stage directions**: Remove lines like `(RUMORE DI...)` or `(CANZONE "..." IN LONTANANZA)` — these are not dialogue
- **CAREFUL**: `lo` is a valid Italian word (article/pronoun meaning "the/him/it"). Do NOT blindly replace `lo` with `Io`. Only fix when context clearly indicates it should be the pronoun `Io` (I), e.g., at the start of a sentence where the speaker is referring to themselves.

### Portuguese
- Uppercase `I` for lowercase `l`: `eIe` → `ele`, `paIavra` → `palavra`, `Iugar` → `lugar`, `reaImente` → `realmente`
- `-Io` → `-lo` (clitics), `á-Io` → `á-lo`, `ê-Io` → `ê-lo`
- `Ihe` → `lhe`, `Ihes` → `lhes`, `úItim` → `últim`
- t→r corruption: `esrá` → `está`, `esre` → `este`, `rudo` → `tudo`, `conrra` → `contra`, `ourro` → `outro`, `denrro` → `dentro`, `enrão` → `então`

### Korean
- Korean subtitles don't have Latin OCR issues but may have encoding problems (Hangul Filler characters U+3164, zero-width spaces, etc.). Drop lines with garbled/non-Korean content.

## Other Issues to Fix

- **SSA/ASS tags**: Strip `{\an8}`, `{\i1}`, `{\i0}`, `{\fs...}`, `{\pos(...)}`, `{\c&...}`, `{\frz...}`, `\h` (hard space → regular space)
- **HTML entities**: `&amp;` → `&`, `&lt;` → `<`, `&gt;` → `>`, `&quot;` → `"`
- **SRT timecodes in text**: Lines containing `-->` are format errors — drop them
- **BOM characters** (U+FEFF): Remove
- **C1 control characters** (U+0080-U+009F): Remove or replace with the correct character if obvious
- **Invisible Unicode**: Remove LRM (U+200E), RLE (U+202B), zero-width spaces (U+200B), etc.
- **Music/HI markers**: Lines that are purely `♪`, `#`, `~`, `*` with no dialogue — drop them. Lines with `[text]` or `(text)` hearing-impaired annotations — remove the annotation but keep the dialogue if any.
- **Watermarks/credits**: Lines containing URLs (`www.`, `http`, `.com`, `.org`, `.net`), translator credits (e.g., "Traduzione: invisiblecatsub", "Sincronização: PowerPlay", "TRADUÇÃO TRX"), or subtitle group attributions — drop them
- **Backticks/acute accents as apostrophes**: `` ` `` or `´` → `'`
- **Double dots**: `..` → `...` (if clearly meant to be an ellipsis)
- **Greek homoglyphs**: Greek characters visually identical to Latin ones (copy-protection) — replace with the correct Latin character

## Important Notes

- Process one language at a time. The user specifies which.
- You must read every line of every file. Do not skip or skim.
- When in doubt about an OCR correction, prefer the reading that makes grammatical sense in context.
- Use subagents to process multiple files in parallel when possible.
- The files are encrypted with git-crypt, but they'll be readable in the working directory.

## What to do if the file is unsavable

If you're having to drop more than a couple lines, or the subtitles are in the wrong language, or there aren't enough line, or it's just unfixable, just leave the file as-is and move on to the next. A couple of the files may end up in this category, or a lot may - it depends on how friendly the subtitle gods are.