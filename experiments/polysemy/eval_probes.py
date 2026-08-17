"""Layer sweep on the gold-labeled probe set.

For each hidden-state layer, per word: leave-one-out 1-NN sense accuracy
(cosine), KMeans ARI vs gold senses, and within-sense minus cross-sense
mean cosine gap. Usage: uv run python eval_probes.py <model_name>
"""

import json
import sys

import numpy as np
from sklearn.cluster import KMeans
from sklearn.metrics import adjusted_rand_score
from sklearn.preprocessing import normalize

from senselib import Embedder, parse_braced


def loo_knn_acc(X: np.ndarray, y: np.ndarray) -> float:
    Xn = normalize(X)
    sim = Xn @ Xn.T
    np.fill_diagonal(sim, -np.inf)
    return float((y[sim.argmax(axis=1)] == y).mean())


def cosine_gap(X: np.ndarray, y: np.ndarray) -> float:
    Xn = normalize(X)
    sim = Xn @ Xn.T
    n = len(y)
    same = np.equal.outer(y, y) & ~np.eye(n, dtype=bool)
    diff = ~np.equal.outer(y, y)
    return float(sim[same].mean() - sim[diff].mean())


def kmeans_ari(X: np.ndarray, y: np.ndarray) -> float:
    k = len(set(y.tolist()))
    pred = KMeans(k, n_init=10, random_state=0).fit_predict(normalize(X))
    return float(adjusted_rand_score(y, pred))


def main():
    model_name = sys.argv[1]
    probes_path = sys.argv[2] if len(sys.argv) > 2 else "probes_eng.json"
    probes = json.load(open(probes_path))["words"]

    emb = Embedder(model_name)
    words = []  # (lemma, X (n, layers+1, h), y (n,))
    for w in probes:
        sents, spans, labels = [], [], []
        for si, (sense, examples) in enumerate(w["senses"].items()):
            for ex in examples:
                clean, span = parse_braced(ex)
                sents.append(clean)
                spans.append(span)
                labels.append(si)
        X = emb.embed_spans(sents, spans)
        words.append((w["lemma"], X, np.array(labels)))
        print(f"embedded {w['lemma']}: {X.shape}", file=sys.stderr)

    n_layers = words[0][1].shape[1]
    print(f"\n{model_name}")
    print(f"{'layer':>5} {'1nn-acc':>8} {'kmeans-ari':>10} {'cos-gap':>8}   worst-word (1nn)")
    results = {}
    for layer in range(n_layers):
        accs, aris, gaps = [], [], []
        per_word = {}
        for lemma, X, y in words:
            Xl = X[:, layer, :]
            a = loo_knn_acc(Xl, y)
            accs.append(a)
            aris.append(kmeans_ari(Xl, y))
            gaps.append(cosine_gap(Xl, y))
            per_word[lemma] = a
        worst = min(per_word, key=per_word.get)
        results[layer] = (np.mean(accs), np.mean(aris), np.mean(gaps), per_word)
        print(
            f"{layer:>5} {np.mean(accs):>8.3f} {np.mean(aris):>10.3f} {np.mean(gaps):>8.3f}"
            f"   {worst} ({per_word[worst]:.2f})"
        )

    best = max(results, key=lambda l: results[l][0])
    print(f"\nbest layer by 1nn-acc: {best}")
    print("per-word at best layer:")
    for lemma, acc in sorted(results[best][3].items(), key=lambda kv: kv[1]):
        print(f"  {lemma:>8}: 1nn-acc {acc:.2f}")

    json.dump(
        {
            "model": model_name,
            "layers": {
                str(l): {"acc": r[0], "ari": r[1], "gap": r[2], "per_word_acc": r[3]}
                for l, r in results.items()
            },
        },
        open(
            f"results_{model_name.replace('/', '_')}_{probes_path.removesuffix('.json')}.json",
            "w",
        ),
        indent=1,
    )


if __name__ == "__main__":
    main()
