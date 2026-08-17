"""Anchor-based sense assignment: the rebuild-stable path.

Sense centroids come from committed anchor sentences (here: the probe set);
each corpus occurrence is assigned to the nearest centroid by cosine.
Prints assignments with margin so we can judge confidence calibration.

Usage: uv run python assign_corpus.py <model> <layer>
"""

import json
import sys

import numpy as np
from sklearn.preprocessing import normalize

from cluster_corpus import occurrences
from senselib import Embedder, parse_braced


def main():
    model_name, layer = sys.argv[1], int(sys.argv[2])
    probes = json.load(open("probes_eng.json"))["words"]
    emb = Embedder(model_name)

    for w in probes:
        lemma, pos = w["lemma"], w["pos"]
        sense_names = list(w["senses"])
        centroids = []
        for sense in sense_names:
            sents, spans = [], []
            for ex in w["senses"][sense]:
                clean, span = parse_braced(ex)
                sents.append(clean)
                spans.append(span)
            X = normalize(emb.embed_spans(sents, spans)[:, layer, :])
            centroids.append(normalize(X.mean(axis=0, keepdims=True))[0])
        C = np.stack(centroids)  # (n_senses, hidden)

        occ = list(occurrences(lemma, pos))
        if not occ:
            continue
        X = normalize(emb.embed_spans([s for s, _ in occ], [sp for _, sp in occ])[:, layer, :])
        sims = X @ C.T  # (n_occ, n_senses)
        print(f"\n=== {lemma}/{pos} ===")
        order = np.argsort(-(sims.max(axis=1) - np.sort(sims, axis=1)[:, -2]))
        for i in order:
            s, (a, b) = occ[i]
            best = sims[i].argmax()
            margin = sims[i].max() - np.sort(sims[i])[-2]
            print(
                f"  {sense_names[best]:>10} (sim {sims[i].max():.2f}, margin {margin:+.2f}): "
                f"{s[:a]}[{s[a:b]}]{s[b:]}"
            )


if __name__ == "__main__":
    main()
