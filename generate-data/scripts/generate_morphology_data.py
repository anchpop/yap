#!/usr/bin/env python3
"""
Download morphological segmentation data from various sources and produce
raw canonical morpheme files per language.

Output format (canonical_morphemes.tsv):
    word<tab>morph1<tab>morph2<tab>morph3...

These are CANONICAL morphemes (dictionary forms), not surface substrings.
The Rust aligner in generate-data handles alignment to surface forms.

Sources:
- SIGMORPHON 2022: fra, spa, ita, eng, rus
- English Wiktionary (kaikki.org): deu, jpn, kor, hin
- Korean Wiktionary (kaikki.org): kor (supplementary)

Usage:
    python3 generate-data/scripts/generate_morphology_data.py
"""

import gzip
import html
import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DATA_DIR = REPO_ROOT / "generate-data" / "data"
CACHE_DIR = REPO_ROOT / ".cache" / "morphology"

SIGMORPHON_LANG_MAP = {
    "fra": "fra",
    "spa": "spa",
    "ita": "ita",
    "eng": "eng",
    "rus": "rus",
}


def ensure_cache_dir():
    CACHE_DIR.mkdir(parents=True, exist_ok=True)


def download_file(url: str, dest: Path) -> Path:
    if dest.exists():
        print(f"  cached: {dest.name}")
        return dest
    print(f"  downloading: {url}")
    subprocess.run(["curl", "-sL", "-o", str(dest), url], check=True)
    return dest


def clone_repo(url: str, dest: Path) -> Path:
    if dest.exists():
        print(f"  cached: {dest.name}")
        return dest
    print(f"  cloning: {url}")
    subprocess.run(["git", "clone", "--depth", "1", url, str(dest)], check=True)
    return dest


def process_sigmorphon() -> dict[str, list[tuple[str, list[str]]]]:
    """Returns {lang_code: [(word, [morph1, morph2, ...]), ...]}"""
    print("\n=== SIGMORPHON 2022 ===")
    sigmorphon_dir = CACHE_DIR / "2022SegmentationST"
    clone_repo("https://github.com/sigmorphon/2022SegmentationST.git", sigmorphon_dir)

    results = {}
    for sig_lang, our_lang in SIGMORPHON_LANG_MAP.items():
        if our_lang is None:
            continue
        print(f"  Processing {sig_lang} -> {our_lang}...")

        seen = set()
        entries = []
        for suffix in ["train", "dev", "test.gold"]:
            tsv_path = sigmorphon_dir / "data" / f"{sig_lang}.word.{suffix}.tsv"
            if not tsv_path.exists():
                continue
            with open(tsv_path) as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    parts = line.split("\t")
                    if len(parts) < 2:
                        continue
                    word = parts[0]
                    morphemes = parts[1].replace(" @@", "\t").split("\t")
                    if len(morphemes) <= 1:
                        continue
                    if word in seen:
                        continue
                    seen.add(word)
                    entries.append((word, morphemes))

        print(f"    {len(entries)} entries")
        results[our_lang] = entries

    return results


def process_wiktionary_en() -> dict[str, list[tuple[str, list[str]]]]:
    """Extract morphology templates from English Wiktionary for German, Japanese, Korean, Hindi."""
    print("\n=== English Wiktionary (kaikki.org) ===")
    dump_path = CACHE_DIR / "raw-wiktextract-data.jsonl.gz"
    download_file("https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz", dump_path)

    target_langs = {
        "German": "deu",
        "Japanese": "jpn",
        "Korean": "kor",
        "Hindi": "hin",
    }

    results: dict[str, list[tuple[str, list[str]]]] = {v: [] for v in target_langs.values()}
    seen: dict[str, set] = {v: set() for v in target_langs.values()}

    print("  Scanning dump (this may take a few minutes)...")
    with gzip.open(dump_path, "rt", encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            lang = d.get("lang")
            if lang not in target_langs:
                continue

            our_lang = target_langs[lang]
            word = d.get("word", "")
            if not word or word in seen[our_lang] or word.startswith(":") or word.startswith("*"):
                continue

            etpl = d.get("etymology_templates", []) or []
            morph_templates = [
                t for t in etpl
                if t.get("name", "") in ("affix", "prefix", "suffix", "compound", "confix", "com")
            ]
            if not morph_templates:
                continue

            t = morph_templates[0]
            args = t.get("args", {})
            morphemes = []
            for i in range(2, 10):
                m = args.get(str(i), "")
                if not m:
                    break
                m = m.strip("-")
                if m:
                    morphemes.append(m)

            if len(morphemes) >= 2:
                seen[our_lang].add(word)
                results[our_lang].append((word, morphemes))

    for lang, entries in results.items():
        print(f"  {lang}: {len(entries)} entries")

    return results


def process_wiktionary_ko() -> list[tuple[str, list[str]]]:
    """Extract compound/derivation data from Korean Wiktionary."""
    print("\n=== Korean Wiktionary (kaikki.org) ===")
    dump_path = CACHE_DIR / "ko-extract.jsonl.gz"
    download_file("https://kaikki.org/dictionary/downloads/ko/ko-extract.jsonl.gz", dump_path)

    entries = []
    seen = set()

    with gzip.open(dump_path, "rt", encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            lang = d.get("lang", "")
            if "한국어" not in lang and d.get("lang_code") != "ko":
                continue

            word = d.get("word", "")
            if word in seen or not word or word.startswith(":") or word.startswith("*"):
                continue

            etym_list = d.get("etymology_texts", []) or []
            for etym in etym_list:
                if "+" not in etym:
                    continue

                # Only use the first line of etymology (rest may be examples)
                etym = etym.split("\n")[0]
                # Remove all parenthetical annotations before splitting
                # This handles cases like "가감 + 저항기 (한자 加減 抵抗器)"
                etym_clean = re.sub(r"\s*\([^)]*\)", "", etym)

                parts = [p.strip() for p in etym_clean.split("+")]
                cleaned = []
                for p in parts:
                    p = p.lstrip("< ").strip()
                    if p:
                        cleaned.append(p)

                if len(cleaned) >= 2:
                    seen.add(word)
                    entries.append((word, cleaned))
                    break

    print(f"  Korean: {len(entries)} entries")
    return entries


def clean_morpheme(m: str) -> str:
    """Clean a single morpheme string."""
    # Decode HTML entities (&amp;beta; -> β)
    m = html.unescape(m)
    # Strip Wiktionary template tags like <alt:3>, <pos:...>, <t:...>
    m = re.sub(r"<[^>]*>", "", m)
    # Strip parenthetical annotations (including malformed ones with no closing paren)
    m = re.sub(r"\([^)]*\)?", "", m)
    # Strip curly braces
    m = re.sub(r"[{}]", "", m)
    # Strip language prefixes from Wiktionary: en:ABC -> ABC
    m = re.sub(r"^[a-z]{2,3}:", "", m)
    # Strip leading/trailing hyphens, whitespace, newlines
    m = m.strip("- \n\r\t")
    return m


def is_valid_entry(word: str, morphemes: list[str]) -> bool:
    """Filter out bad entries."""
    # Skip affixes (words starting with -)
    if word.startswith("-"):
        return False
    # Skip entries that look like example sentences or metadata
    if word.startswith(":") or word.startswith("*") or word.startswith("#"):
        return False
    # Skip if word contains ":*" anywhere (Korean Wiktionary example sentences)
    if ":*" in word:
        return False
    # Skip empty words
    if not word.strip():
        return False
    # Need at least 2 non-empty morphemes
    non_empty = [m for m in morphemes if m]
    if len(non_empty) < 2:
        return False
    return True


def write_canonical_tsv(lang_code: str, entries: list[tuple[str, list[str]]]):
    lang_dir = DATA_DIR / lang_code
    if not lang_dir.exists():
        print(f"  skipping {lang_code} (no data folder)")
        return

    out_path = lang_dir / "canonical_morphemes.tsv"
    # Sort and dedupe, cleaning as we go
    seen = set()
    unique = []
    skipped = 0
    for word, morphemes in sorted(entries):
        if word in seen:
            continue
        # Clean morphemes
        morphemes = [clean_morpheme(m) for m in morphemes]
        # Filter empty morphemes
        morphemes = [m for m in morphemes if m]
        if not is_valid_entry(word, morphemes):
            skipped += 1
            continue
        seen.add(word)
        unique.append((word, morphemes))

    with open(out_path, "w") as f:
        for word, morphemes in unique:
            # Final safety: skip any word that doesn't look like a real word
            if not word or word[0] in ":*#" or "\t" in word or "\n" in word:
                continue
            f.write(f"{word}\t{chr(9).join(morphemes)}\n")

    print(f"  wrote {out_path} ({len(unique)} entries, {skipped} skipped)")


def main():
    ensure_cache_dir()

    all_data: dict[str, list[tuple[str, list[str]]]] = {}

    # 1. SIGMORPHON
    sigmorphon = process_sigmorphon()
    for lang, entries in sigmorphon.items():
        all_data.setdefault(lang, []).extend(entries)

    # 2. English Wiktionary
    wikt_en = process_wiktionary_en()
    for lang, entries in wikt_en.items():
        all_data.setdefault(lang, []).extend(entries)

    # 3. Korean Wiktionary (supplements English Wiktionary)
    wikt_ko = process_wiktionary_ko()
    all_data.setdefault("kor", []).extend(wikt_ko)

    # Write output
    print("\n=== Writing canonical_morphemes.tsv files ===")
    for lang, entries in sorted(all_data.items()):
        write_canonical_tsv(lang, entries)

    print("\nDone! Next step: run the Rust aligner to produce morphology.tsv files.")


if __name__ == "__main__":
    main()
