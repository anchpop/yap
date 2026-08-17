"""Cluster real corpus occurrences of a lemma/POS by contextual embedding.

Usage: uv run python cluster_corpus.py <model> <layer> <lemma>/<POS> [more...]
e.g.:  uv run python cluster_corpus.py Alibaba-NLP/gte-modernbert-base 17 tear/NOUN bank/NOUN
"""

import json
import sys

import numpy as np
from scipy.cluster.hierarchy import fcluster, linkage
from sklearn.preprocessing import normalize

from senselib import Embedder

CORPUS = "/data/coding/yap/out/cleaned_eng.jsonl"


def occurrences(lemma: str, pos: str):
    """Yield (sentence, (start, end)) for each token matching lemma/POS."""
    for line in open(CORPUS):
        d = json.loads(line)
        # reconstruct char offsets from text+whitespace
        off = 0
        rebuilt = []
        tok_spans = []
        for t in d["tokens"]:
            tok_spans.append((off, off + len(t["text"])))
            rebuilt.append(t["text"] + t["whitespace"])
            off += len(t["text"]) + len(t["whitespace"])
        text = "".join(rebuilt)
        for t, span in zip(d["tokens"], tok_spans):
            if t["lemma"].lower() == lemma and t["pos"] == pos:
                yield text, span


def main():
    model_name, layer = sys.argv[1], int(sys.argv[2])
    targets = [a.split("/") for a in sys.argv[3:]]
    emb = Embedder(model_name)

    for lemma, pos in targets:
        occ = list(occurrences(lemma.lower(), pos))
        if len(occ) < 2:
            print(f"\n=== {lemma}/{pos}: only {len(occ)} occurrences, skipping ===")
            continue
        sents = [s for s, _ in occ]
        spans = [sp for _, sp in occ]
        X = emb.embed_spans(sents, spans)[:, layer, :]
        Xn = normalize(X)
        Z = linkage(Xn, method="average", metric="cosine")
        for thresh in (0.35, 0.45):
            labels = fcluster(Z, t=thresh, criterion="distance")
            print(f"\n=== {lemma}/{pos} ({len(occ)} occ), cosine-dist threshold {thresh} ===")
            for c in sorted(set(labels)):
                idx = [i for i, l in enumerate(labels) if l == c]
                print(f"  cluster {c} ({len(idx)}):")
                for i in idx:
                    s, (a, b) = occ[i]
                    print(f"    {s[:a]}[{s[a:b]}]{s[b:]}")


if __name__ == "__main__":
    main()
