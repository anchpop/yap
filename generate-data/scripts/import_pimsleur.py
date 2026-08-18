#!/usr/bin/env python3
"""Extract lesson-scoped target/native sentence pairs from Pimsleur audio.

The source archive contains alternating English instruction and target-language
speech. Gemini can consume a complete lesson and return the target utterances
paired with the English prompts/translations that introduce them. Output is the
restricted-source layout consumed by generate-data.
"""

from __future__ import annotations

import argparse
import base64
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


def lesson_coordinates(path: Path, source: Path) -> tuple[int, int] | None:
    relative = path.relative_to(source)
    if any(part.lower() == "readings" for part in relative.parts):
        return None

    level = 1
    for part in relative.parts[:-1]:
        match = re.fullmatch(r"level[ _-]*(\d+)", part, flags=re.IGNORECASE)
        if match:
            level = int(match.group(1))
            break

    name = path.stem
    unit_patterns = (
        r"(?:^|[_ -])U(\d{1,2})(?:[_ -]|$)",
        r"(?:^|[_ -])Unit[ _-]?(\d{1,2})(?:[_ -]|$)",
        r"(?:^|[_ -])Lesson[ _-]?(\d{1,2})(?:[_ -]|$)",
    )
    for pattern in unit_patterns:
        match = re.search(pattern, name, flags=re.IGNORECASE)
        if match:
            return level, int(match.group(1))
    return None


def discover_lessons(source: Path) -> list[tuple[int, int, Path]]:
    lessons: dict[tuple[int, int], Path] = {}
    for path in source.rglob("*.mp3"):
        coordinates = lesson_coordinates(path, source)
        if coordinates is None:
            continue
        if coordinates in lessons:
            other = lessons[coordinates]
            raise RuntimeError(
                f"multiple audio files resolve to level {coordinates[0]}, "
                f"lesson {coordinates[1]}: {other} and {path}"
            )
        lessons[coordinates] = path
    return [(level, lesson, path) for (level, lesson), path in sorted(lessons.items())]


def output_path(root: Path, native_code: str, level: int, lesson: int) -> Path:
    return (
        root
        / "sentence-sources"
        / "pimsleur"
        / f"for_{native_code}"
        / f"level_{level}"
        / f"unit_{lesson:02d}"
        / "sentences.jsonl"
    )


def response_schema() -> dict:
    return {
        "name": "pimsleur_lesson",
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


def prompt(target_name: str, native_name: str) -> str:
    return f"""This is a Pimsleur lesson for {native_name} speakers learning {target_name}.

Extract, in chronological order, every {target_name} utterance the learner is
taught or expected to understand. Pair each with the corresponding {native_name}
meaning stated or clearly established by the instructor. Transcribe the
{target_name} in its normal native writing system and write a natural, faithful
{native_name} translation.

List each distinct target-language utterance once, even when the lesson drills
it repeatedly. A pair must contain one coherent utterance; split unrelated drill
alternatives instead of joining them. Do not include instructor directions or
meta-language such as “listen”, “repeat”, or “how do you say…”. Exclude all
backward-build pronunciation chunks, even when a chunk happens to be a real word
with a different meaning. Also exclude reading-lesson material, music, and speech
in another language. Do not invent a pair when the meaning is unclear. Keep
genuinely taught short words and phrases, not only full grammatical sentences."""


def transcribe(
    audio_path: Path,
    target_name: str,
    native_name: str,
    model: str,
    api_key: str,
    attempts: int,
) -> list[dict[str, str]]:
    encoded = base64.b64encode(audio_path.read_bytes()).decode("ascii")
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
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {"data": encoded, "format": "mp3"},
                    },
                    {"type": "text", "text": prompt(target_name, native_name)},
                ],
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
                timeout=900,
            )
            response.raise_for_status()
            content = response.json()["choices"][0]["message"]["content"]
            result = json.loads(content)
            sentences = [
                row
                for row in result["sentences"]
                if "fragment" not in row["native_language"].casefold()
                and not (
                    target_name.casefold() == "thai"
                    and re.search(r"[A-Za-z]", row["target_language"])
                )
            ]
            if not sentences:
                raise ValueError("model returned no sentence pairs")
            for row in sentences:
                if not row["target_language"].strip() or not row["native_language"].strip():
                    raise ValueError("model returned a blank target/native field")
            distinct = []
            seen_targets = set()
            for row in sentences:
                target = row["target_language"].strip()
                if target not in seen_targets:
                    distinct.append(row)
                    seen_targets.add(target)
            return distinct
        except Exception as error:  # noqa: BLE001 - retry all request/response failures
            last_error = error
            if attempt < attempts:
                time.sleep(min(60, 5 * 2 ** (attempt - 1)))
    raise RuntimeError(f"failed after {attempts} attempts: {last_error}") from last_error


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
    parser.add_argument("source", type=Path, help="course directory containing lesson MP3s")
    parser.add_argument("output", type=Path, help="generate-data/data/<target-code>")
    parser.add_argument("--target-name", required=True, help="language name used in the prompt")
    parser.add_argument("--native-name", default="English")
    parser.add_argument("--native-code", default="eng")
    parser.add_argument("--model", default="gemini-3.5-flash")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--attempts", type=int, default=4)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--limit", type=int, help="process only the first N pending lessons")
    args = parser.parse_args()

    api_key = os.environ.get("GEMINI_API_KEY")
    if not api_key:
        parser.error("GEMINI_API_KEY is not set")

    lessons = discover_lessons(args.source)
    pending = []
    for level, lesson, audio_path in lessons:
        destination = output_path(args.output, args.native_code, level, lesson)
        if args.force or not destination.exists():
            pending.append((level, lesson, audio_path, destination))
    if args.limit is not None:
        pending = pending[: args.limit]

    print(f"Discovered {len(lessons)} lessons; {len(pending)} pending", flush=True)
    failures = []

    def process(item: tuple[int, int, Path, Path]) -> tuple[int, int, int]:
        level, lesson, audio_path, destination = item
        rows = transcribe(
            audio_path,
            args.target_name,
            args.native_name,
            args.model,
            api_key,
            args.attempts,
        )
        write_jsonl(destination, rows)
        return level, lesson, len(rows)

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        future_to_item = {executor.submit(process, item): item for item in pending}
        for future in as_completed(future_to_item):
            level, lesson, audio_path, _ = future_to_item[future]
            try:
                _, _, count = future.result()
                print(f"level {level} lesson {lesson:02d}: {count} pairs", flush=True)
            except Exception as error:  # noqa: BLE001 - report each failed worker
                failures.append((level, lesson, audio_path, error))
                print(
                    f"level {level} lesson {lesson:02d}: ERROR: {error}",
                    file=sys.stderr,
                    flush=True,
                )

    if failures:
        print(f"{len(failures)} lesson(s) failed; rerun to resume", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
