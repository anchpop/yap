"""Unsupervised polysemy candidate mining over a whole corpus.

For every lemma/POS with >= MIN_OCC occurrences: embed all occurrences
(capped sample), split with 2-means, and score how bimodal the lemma's
embedding cloud is (cosine silhouette, filtered by cluster balance).
High score = candidate polysemy. Dumps ranked candidates with exemplar
sentences per side for LLM judging.

Usage: uv run python discover.py <model> <layer> <corpus.jsonl> <out.json>
       (model "modal" = the deployed bge-m3@L17 Modal endpoint, ~100x faster)
"""

import json
import random
import sys
from concurrent.futures import ThreadPoolExecutor

import numpy as np
from sklearn.cluster import KMeans
from sklearn.metrics import silhouette_score
from sklearn.preprocessing import normalize

from senselib import Embedder, ModalEmbedder

MIN_OCC = 8
CAP = 60  # max occurrences embedded per lemma/POS
POS_KEEP = ("NOUN", "VERB", "ADJ")
EXEMPLARS = 8


def load_occurrences(corpus_path: str):
    """{(lemma, pos): [(sentence, span), ...]}"""
    occ = {}
    for line in open(corpus_path):
        d = json.loads(line)
        off = 0
        spans = []
        parts = []
        for t in d["tokens"]:
            spans.append((off, off + len(t["text"])))
            parts.append(t["text"] + t["whitespace"])
            off += len(t["text"]) + len(t["whitespace"])
        text = "".join(parts)
        for t, span in zip(d["tokens"], spans):
            if t["pos"] in POS_KEEP:
                occ.setdefault((t["lemma"].lower(), t["pos"]), []).append((text, span))
    return {k: v for k, v in occ.items() if len(v) >= MIN_OCC}


def main():
    model_name, layer, corpus_path, out_path = (
        sys.argv[1],
        int(sys.argv[2]),
        sys.argv[3],
        sys.argv[4],
    )
    occ = load_occurrences(corpus_path)
    rng = random.Random(0)
    for k, v in occ.items():
        if len(v) > CAP:
            occ[k] = rng.sample(v, CAP)
    total = sum(len(v) for v in occ.values())
    print(f"{len(occ)} lemma/POS, {total} occurrences to embed", file=sys.stderr)

    emb = ModalEmbedder() if model_name == "modal" else Embedder(model_name)
    done = 0

    def mine(entry):
        nonlocal done
        (lemma, pos), items = entry
        X = normalize(
            emb.embed_spans([s for s, _ in items], [sp for _, sp in items], layer=layer)
        )
        done += 1
        if done % 50 == 0:
            print(f"  {done}/{len(occ)}", file=sys.stderr)
        km = KMeans(2, n_init=10, random_state=0).fit(X)
        labels = km.labels_
        sizes = np.bincount(labels, minlength=2)
        balance = sizes.min() / len(labels)
        if sizes.min() < 3:
            return None
        sil = float(silhouette_score(X, labels, metric="cosine"))
        # exemplars: nearest to each centroid
        cents = normalize(km.cluster_centers_)
        sides = []
        for c in range(2):
            idx = np.where(labels == c)[0]
            sims = X[idx] @ cents[c]
            best = idx[np.argsort(-sims)][:EXEMPLARS]
            sides.append(
                [
                    {"sentence": items[i][0], "span": list(items[i][1])}
                    for i in best.tolist()
                ]
            )
        return {
            "lemma": lemma,
            "pos": pos,
            "n": len(items),
            "silhouette": sil,
            "balance": float(balance),
            "clusters": sides,
        }

    # Network-bound against the endpoint, so thread it; the local path keeps
    # a single worker (CPU torch gains nothing from thread contention).
    workers = 8 if model_name == "modal" else 1
    with ThreadPoolExecutor(max_workers=workers) as ex:
        results = [r for r in ex.map(mine, sorted(occ.items())) if r is not None]

    results.sort(key=lambda r: -r["silhouette"])
    json.dump(results, open(out_path, "w"), indent=1)
    print(f"\ntop candidates by silhouette (balance>=0.15 shown first):")
    shown = 0
    for r in results:
        if r["balance"] < 0.15:
            continue
        a = r["clusters"][0][0]["sentence"][:60]
        b = r["clusters"][1][0]["sentence"][:60]
        print(
            f"{r['lemma']}/{r['pos']:<5} n={r['n']:>3} sil={r['silhouette']:.3f} "
            f"bal={r['balance']:.2f} | {a!r} vs {b!r}"
        )
        shown += 1
        if shown >= 50:
            break


if __name__ == "__main__":
    main()
