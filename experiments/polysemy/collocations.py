"""Collocation discovery from the polysemy miner's cluster splits.

The 2-means splits in discover.py output separate word-in-fixed-expression
from word-used-freely at least as often as they separate true senses (the
embeddings absorb phrase meaning). This second judge lane mines that: for
each candidate, ask an LLM whether either cluster is dominated by fixed
multiword expressions of the target word, extract them in canonical form,
and classify opacity (opaque = learner needs it as its own vocabulary item).
Discovered expressions are cross-checked against the deterministic
detectors' inventory (target_language_multiword_terms.txt) to flag novel
ones.

Usage: uv run python collocations.py candidates_fra.json \
           ../../out/fra/target_language_multiword_terms.txt \
           collocations_fra.json [top_k]
"""

import json
import subprocess
import sys
import unicodedata
from concurrent.futures import ThreadPoolExecutor

MODEL = "sonnet"
MIN_BALANCE = 0.15

PROMPT = """\
You are mining a sentence corpus for FIXED MULTIWORD EXPRESSIONS (collocations, \
idioms, compounds, fixed phrases). The word "{lemma}" ({pos}) appears in the \
corpus; an embedding model split its occurrences into two clusters. Often one \
cluster is dominated by the word occurring inside one or more fixed \
expressions.

Cluster A:
{a}

Cluster B:
{b}

The target word is marked «like this». For each fixed multiword expression of \
"{lemma}" that accounts for several lines in either cluster, report it. Do NOT \
report free combinations (adjective+noun that just happen to co-occur), \
inflectional variants, or expressions appearing only once.

Classify each expression's opacity for a French learner:
- "opaque": meaning not derivable from the parts; must be learned as its own \
vocabulary item (e.g. "au courant" = informed).
- "semi": partly compositional but conventionalized; a learner benefits from \
learning it as a unit (e.g. "sans doute" = probably).
- "transparent": fully compositional; the parts suffice (e.g. "guitare \
électrique").

Reply with STRICT JSON only, no markdown fence. Escape any double quotes \
inside strings:
{{"expressions": [{{"expression": "canonical citation form", \
"gloss": "2-6 word English gloss", "opacity": "opaque|semi|transparent", \
"cluster": "A|B", "lines": <how many lines it accounts for>}}], \
"note": "one short sentence"}}
If neither cluster is driven by fixed expressions, "expressions" is [].\
"""


def mark(ex):
    s, (a, b) = ex["sentence"], ex["span"]
    return f"- {s[:a]}«{s[a:b]}»{s[b:]}"


def normalize(s: str) -> str:
    s = unicodedata.normalize("NFC", s).lower().strip()
    return s.replace("’", "'").replace("ʼ", "'")


def ask(prompt: str) -> dict:
    for _ in range(2):  # one retry for malformed JSON
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
            return json.loads(out)
        except json.JSONDecodeError:
            continue
    return {"error": out[:500] or r.stderr[:500]}


def judge(cand):
    a = "\n".join(mark(e) for e in cand["clusters"][0])
    b = "\n".join(mark(e) for e in cand["clusters"][1])
    verdict = ask(PROMPT.format(lemma=cand["lemma"], pos=cand["pos"], a=a, b=b))
    return {
        "lemma": cand["lemma"],
        "pos": cand["pos"],
        "n": cand["n"],
        "silhouette": cand["silhouette"],
        **verdict,
    }


def main():
    cand_path, known_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    top_k = int(sys.argv[4]) if len(sys.argv) > 4 else 100
    known = {normalize(line) for line in open(known_path)}

    cands = [
        c for c in json.load(open(cand_path)) if c["balance"] >= MIN_BALANCE
    ][:top_k]
    print(f"mining {len(cands)} candidates for collocations with {MODEL}",
          file=sys.stderr)

    with ThreadPoolExecutor(max_workers=8) as ex:
        results = list(ex.map(judge, cands))

    for r in results:
        for e in r.get("expressions", []):
            e["known"] = normalize(e["expression"]) in known
    json.dump(results, open(out_path, "w"), indent=1, ensure_ascii=False)

    exprs = [
        (e, r) for r in results for e in r.get("expressions", [])
        if e.get("lines", 0) >= 2
    ]
    errs = [r for r in results if "error" in r]
    print(f"\n{len(exprs)} expressions from {len(results)} candidates "
          f"({len(errs)} judge errors)\n")
    for opacity in ("opaque", "semi", "transparent"):
        rows = [(e, r) for e, r in exprs if e.get("opacity") == opacity]
        rows.sort(key=lambda t: -t[1]["silhouette"])
        print(f"== {opacity} ({len(rows)}) ==")
        for e, r in rows:
            tag = "     " if e["known"] else "NEW  "
            print(f"  {tag}{e['expression']:<28} {e.get('gloss','')!s:<30} "
                  f"(from {r['lemma']}/{r['pos']}, sil={r['silhouette']:.2f})")
        print()


if __name__ == "__main__":
    main()
