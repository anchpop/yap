"""LLM adjudication of mined polysemy candidates via `claude -p`.

Reads discover.py output, asks an LLM per candidate whether the 2-means
split reflects genuinely distinct senses (worth separate vocabulary
entries / translations) or just usage variation, and to name the senses.

Usage: uv run python judge.py candidates_eng.json verdicts_eng.json [top_k]
"""

import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

MODEL = "sonnet"
MIN_BALANCE = 0.15

PROMPT = """\
You are auditing a language-learning vocabulary. The word "{lemma}" ({pos}) \
appears in a sentence corpus. An embedding model split its occurrences into \
two clusters. Decide whether the clusters correspond to GENUINELY DISTINCT \
MEANINGS of "{lemma}" — distinct enough that a learner would treat them as \
different vocabulary items and they would usually receive different \
translations in another language (homonyms like bank=money/river, or strong \
polysemy like paper=material/newspaper). Mere topic or register variation \
of one meaning does NOT count. Idioms count as distinct if the word's \
meaning inside the idiom is opaque.

Cluster A:
{a}

Cluster B:
{b}

The target word is marked «like this». Some lines are bare dictionary \
fragments with little context; weigh full sentences more.

Reply with STRICT JSON only, no markdown fence:
{{"distinct": true/false, "confidence": 0.0-1.0, \
"sense_a": "2-5 word gloss of cluster A's sense", \
"sense_b": "2-5 word gloss of cluster B's sense", \
"note": "one short sentence"}}
If not distinct, sense_a/sense_b should both gloss the single shared sense.\
"""


def mark(ex):
    s, (a, b) = ex["sentence"], ex["span"]
    return f"- {s[:a]}«{s[a:b]}»{s[b:]}"


def judge(cand):
    a = "\n".join(mark(e) for e in cand["clusters"][0])
    b = "\n".join(mark(e) for e in cand["clusters"][1])
    prompt = PROMPT.format(lemma=cand["lemma"], pos=cand["pos"], a=a, b=b)
    r = subprocess.run(
        ["claude", "-p", "--model", MODEL],
        input=prompt,
        capture_output=True,
        text=True,
        timeout=300,
    )
    out = r.stdout.strip()
    if out.startswith("```"):
        out = out.strip("`").removeprefix("json").strip()
    try:
        verdict = json.loads(out)
    except json.JSONDecodeError:
        verdict = {"error": out[:500] or r.stderr[:500]}
    return {
        "lemma": cand["lemma"],
        "pos": cand["pos"],
        "n": cand["n"],
        "silhouette": cand["silhouette"],
        "balance": cand["balance"],
        **verdict,
    }


def main():
    in_path, out_path = sys.argv[1], sys.argv[2]
    top_k = int(sys.argv[3]) if len(sys.argv) > 3 else 60
    cands = [c for c in json.load(open(in_path)) if c["balance"] >= MIN_BALANCE][:top_k]
    print(f"judging {len(cands)} candidates with {MODEL}", file=sys.stderr)

    with ThreadPoolExecutor(max_workers=8) as ex:
        verdicts = list(ex.map(judge, cands))

    json.dump(verdicts, open(out_path, "w"), indent=1)
    distinct = [v for v in verdicts if v.get("distinct")]
    print(f"\n{len(distinct)}/{len(verdicts)} judged distinct:\n")
    for v in sorted(distinct, key=lambda v: -v.get("confidence", 0)):
        print(
            f"  {v['lemma']}/{v['pos']:<5} conf={v.get('confidence', 0):.2f} "
            f"sil={v['silhouette']:.3f}  A: {v.get('sense_a')}  B: {v.get('sense_b')}"
        )
    errs = [v for v in verdicts if "error" in v]
    if errs:
        print(f"\n{len(errs)} errors, e.g. {errs[0]['error'][:200]}")


if __name__ == "__main__":
    main()
