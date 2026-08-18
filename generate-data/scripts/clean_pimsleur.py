#!/usr/bin/env python3
"""Clean lesson-scoped Pimsleur target/English sentence pairs with Gemini."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import requests

GEMINI_OPENAI_URL = (
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
)


def response_schema() -> dict:
    return {
        "name": "clean_pimsleur_lesson",
        "strict": True,
        "schema": {
            "type": "object",
            "properties": {
                "sentences": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "target_language": {"type": "string"},
                            "native_language": {"type": "string"},
                        },
                        "required": ["target_language", "native_language"],
                        "additionalProperties": False,
                    },
                }
            },
            "required": ["sentences"],
            "additionalProperties": False,
        },
    }


def prompt(target_name: str, target_code: str, rows: list[dict[str, str]]) -> str:
    script_rule = {
        "tha": (
            "Write Thai in Thai script. Arabic numerals are allowed, but remove "
            "Latin or Chinese transcription corruption."
        ),
        "zho-hant": (
            "Write Mandarin in consistent Traditional Chinese. Latin-script personal "
            "and place names are allowed, but English prompt words embedded by mistake "
            "are not."
        ),
    }.get(target_code, f"Use the normal writing system for {target_name}.")
    serialized = json.dumps(rows, ensure_ascii=False, separators=(",", ":"))
    return f"""Clean this lesson from a Pimsleur {target_name}-English corpus.

Preserve the chronological order and the meaning actually supported by each
input pair. Return only useful target-language words, phrases, and coherent
utterances with natural, faithful English translations.

Apply all of these rules:
- Remove exact/rephrased duplicates within the lesson.
- Remove instructor directions and meta-language such as “listen”, “repeat”,
  “say it this way”, dialogue introductions, and “how do you say …”.
- Remove backward-build syllables and pronunciation-only chunks, even when a
  chunk coincidentally spells an unrelated real word. Keep genuinely taught
  standalone vocabulary and particles.
- Split unrelated alternatives or consecutive drill prompts that were merged
  into one row. Do not split a natural multi-clause utterance or dialogue turn.
- Correct obvious target transcription errors and target/English mismatches,
  especially person, gender, negation, names, and numbers. Prefer the target
  utterance plus the supplied English as mutual evidence.
- If a correction is ambiguous, drop the row instead of inventing content.
- Do not add lesson content that is absent from the input.
- {script_rule}

Input pairs:
{serialized}"""


def clean_lesson(
    rows: list[dict[str, str]],
    target_name: str,
    target_code: str,
    model: str,
    api_key: str,
    attempts: int,
) -> list[dict[str, str]]:
    payload = {
        "model": model,
        "temperature": 0,
        "response_format": {
            "type": "json_schema",
            "json_schema": response_schema(),
        },
        "messages": [
            {
                "role": "user",
                "content": prompt(target_name, target_code, rows),
            }
        ],
    }

    last_error: Exception | None = None
    for attempt in range(1, attempts + 1):
        try:
            response = requests.post(
                GEMINI_OPENAI_URL,
                headers={"Authorization": f"Bearer {api_key}"},
                json=payload,
                timeout=600,
            )
            response.raise_for_status()
            content = response.json()["choices"][0]["message"]["content"]
            cleaned = json.loads(content)["sentences"]
            if not cleaned:
                raise ValueError("model removed the entire lesson")
            if len(cleaned) > len(rows) * 1.25:
                raise ValueError(
                    f"model expanded {len(rows)} input rows to {len(cleaned)} rows"
                )

            distinct = []
            seen_targets = set()
            for row in cleaned:
                target = row["target_language"].strip()
                native = row["native_language"].strip()
                if not target or not native:
                    continue
                if target_code == "tha" and re.search(r"[A-Za-z\u3400-\u9fff]", target):
                    continue
                if target in seen_targets:
                    continue
                distinct.append(
                    {"target_language": target, "native_language": native}
                )
                seen_targets.add(target)
            if not distinct:
                raise ValueError("no valid distinct pairs remained")
            return distinct
        except Exception as error:  # noqa: BLE001 - retry request/response failures
            last_error = error
            if attempt < attempts:
                time.sleep(min(60, 5 * 2 ** (attempt - 1)))
    raise RuntimeError(f"failed after {attempts} attempts: {last_error}") from last_error


def read_jsonl(path: Path) -> list[dict[str, str]]:
    return [json.loads(line) for line in path.read_text().splitlines()]


def write_jsonl(path: Path, rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
        for row in rows:
            json.dump(row, temporary, ensure_ascii=False, separators=(",", ":"))
            temporary.write("\n")
    temporary_path.replace(path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path, help="for_eng directory to clean")
    parser.add_argument("output", type=Path, help="destination for_eng directory")
    parser.add_argument("--target-name", required=True)
    parser.add_argument("--target-code", required=True)
    parser.add_argument("--model", default="gemini-3.5-flash")
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--attempts", type=int, default=4)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()

    api_key = os.environ.get("GEMINI_API_KEY")
    if not api_key:
        parser.error("GEMINI_API_KEY is not set")

    sources = sorted(args.source.glob("level_*/unit_*/sentences.jsonl"))
    pending = []
    for source in sources:
        relative = source.relative_to(args.source)
        destination = args.output / relative
        if args.force or not destination.exists():
            pending.append((source, destination))
    if args.limit is not None:
        pending = pending[: args.limit]

    print(f"Discovered {len(sources)} lessons; {len(pending)} pending", flush=True)
    failures = []

    def process(item: tuple[Path, Path]) -> tuple[Path, int, int]:
        source, destination = item
        rows = read_jsonl(source)
        cleaned = clean_lesson(
            rows,
            args.target_name,
            args.target_code,
            args.model,
            api_key,
            args.attempts,
        )
        write_jsonl(destination, cleaned)
        return source, len(rows), len(cleaned)

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        future_to_item = {executor.submit(process, item): item for item in pending}
        for future in as_completed(future_to_item):
            source, _ = future_to_item[future]
            relative = source.relative_to(args.source)
            try:
                _, before, after = future.result()
                print(f"{relative}: {before} -> {after}", flush=True)
            except Exception as error:  # noqa: BLE001 - report each failed worker
                failures.append((relative, error))
                print(f"{relative}: ERROR: {error}", file=sys.stderr, flush=True)

    if failures:
        print(f"{len(failures)} lesson(s) failed; rerun to resume", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
