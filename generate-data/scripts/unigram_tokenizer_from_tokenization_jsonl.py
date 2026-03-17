#!/usr/bin/env python3
"""
Word-level unigram tokenizer over generate-data's tokenization JSONL format.

Reads lines shaped like:
{"sentence":"...","tokens":[{"text":{"text":"Rocket"},"whitespace":"","pos":"NOUN",...}, ...]}

This version intentionally ignores whitespace and capitalization and just
segments the lowercased token text stream.
"""

import argparse
import json
import math
import random
import time
from collections import Counter


CONTENT_POS = {
    "ADJ",
    "ADV",
    "AUX",
    "INTJ",
    "NOUN",
    "NUM",
    "SCONJ",
    "VERB",
    "X",
}
PROPER_NOUN_POS = "PROPN"


def tokenize_words_from_record(record):
    words = []
    for token in record["tokens"]:
        text = token["text"]["text"].strip()
        if not text:
            continue
        lemma_info = token.get("lemma")
        if isinstance(lemma_info, dict):
            lemma = lemma_info.get("lemma")
        else:
            lemma = None
        pos = token.get("pos")
        words.append(
            {
                "text": text.lower(),
                "lemma": (lemma or text).lower(),
                "pos": pos,
                "key": f"{text.lower()}\t{(lemma or text).lower()}\t{pos or ''}",
            }
        )
    return words


def load_corpus(path, min_len):
    sentences = []
    all_words = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            words = tokenize_words_from_record(record)
            sentences.append(record["sentence"])
            if len(words) >= min_len:
                all_words.append(words)
    return sentences, all_words


def is_content_word(token):
    return token["pos"] in CONTENT_POS


def is_proper_noun(token):
    return token["pos"] == PROPER_NOUN_POS


def is_valid_multigram(tokens):
    if len(tokens) < 2:
        return True
    if not is_content_word(tokens[0]) or not is_content_word(tokens[-1]):
        return False
    if any(is_proper_noun(token) for token in tokens):
        return False
    return True


def build_ngram_counts(sentences_words, max_n=5, constrained=False):
    counts = Counter()
    for words in sentences_words:
        n_words = len(words)
        for n in range(1, max_n + 1):
            for i in range(n_words - n + 1):
                token_slice = words[i : i + n]
                if constrained and not is_valid_multigram(token_slice):
                    continue
                counts[tuple(token["key"] for token in token_slice)] += 1
    return counts


def build_vocab(ngram_counts, multi_budget, min_freq=3):
    unigrams = {k: v for k, v in ngram_counts.items() if len(k) == 1}
    multigrams = {
        k: v for k, v in ngram_counts.items() if len(k) >= 2 and v >= min_freq
    }
    sorted_multi = sorted(multigrams.items(), key=lambda x: -x[1])[:multi_budget]

    vocab = dict(unigrams)
    for ngram, freq in sorted_multi:
        vocab[ngram] = freq
    return vocab


def viterbi(words, costs, max_n=5):
    n = len(words)
    if n == 0:
        return [], 0.0

    best_cost = [float("inf")] * (n + 1)
    best_cost[0] = 0.0
    backpointer = [0] * (n + 1)

    for i in range(n):
        if best_cost[i] == float("inf"):
            continue
        for length in range(1, min(max_n, n - i) + 1):
            token = tuple(words[i : i + length])
            if token in costs:
                new_cost = best_cost[i] + costs[token]
                if new_cost < best_cost[i + length]:
                    best_cost[i + length] = new_cost
                    backpointer[i + length] = i

    tokens = []
    pos = n
    while pos > 0:
        start = backpointer[pos]
        tokens.append(tuple(words[start:pos]))
        pos = start
    tokens.reverse()

    return tokens, best_cost[n]


def best_alternative_cost(seq, costs, max_n):
    n = len(seq)
    if n <= 1:
        return None

    best_cost = [float("inf")] * (n + 1)
    best_cost[0] = 0.0

    for i in range(n):
        if math.isinf(best_cost[i]):
            continue
        for length in range(1, min(max_n, n - i) + 1):
            end = i + length
            if i == 0 and end == n:
                continue
            token = tuple(seq[i:end])
            cost = costs.get(token)
            if cost is None:
                continue
            new_cost = best_cost[i] + cost
            if new_cost < best_cost[end]:
                best_cost[end] = new_cost

    return None if math.isinf(best_cost[n]) else best_cost[n]


def costs_from_counts(counts, vocab):
    total = sum(counts.values())
    if total == 0:
        return {k: 30.0 for k in vocab}

    costs = {}
    for tok in vocab:
        freq = counts.get(tok, 0)
        costs[tok] = -math.log2(freq / total) if freq > 0 else 30.0
    return costs


def costs_from_raw_freq(vocab):
    total = sum(vocab.values())
    return {k: -math.log2(v / total) for k, v in vocab.items()}


def blend_costs(prob_costs, alpha):
    if alpha == 0.0:
        return dict(prob_costs)
    return {
        tok: alpha * 1.0 + (1.0 - alpha) * prob_cost
        for tok, prob_cost in prob_costs.items()
    }


def segment_corpus(all_words, costs, max_n):
    return [
        viterbi([token["key"] for token in words], costs, max_n=max_n)[0]
        for words in all_words
    ]


def count_token_usage(segmentations):
    counts = Counter()
    for toks in segmentations:
        for tok in toks:
            counts[tok] += 1
    return counts


def train_unigram_simple(all_words, vocab, max_n, n_iters=10, alpha=0.0, verbose=True):
    costs = costs_from_raw_freq(vocab)
    if alpha > 0:
        costs = blend_costs(costs, alpha)

    segmentations = None

    for it in range(n_iters):
        t0 = time.time()
        segmentations = segment_corpus(all_words, costs, max_n=max_n)
        counts = count_token_usage(segmentations)
        total_tokens = sum(counts.values())
        total_cost = sum(sum(costs[t] for t in toks) for toks in segmentations)
        multi_used = sum(1 for t in counts if len(t) >= 2 and counts[t] > 0)
        elapsed = time.time() - t0

        if verbose:
            print(
                f"  EM iter {it + 1}: "
                f"{total_tokens} tokens, "
                f"{total_cost:.0f} total bits, "
                f"{multi_used} multi-word used, "
                f"{elapsed:.1f}s"
            )

        new_prob_costs = costs_from_counts(counts, vocab)
        new_costs = blend_costs(new_prob_costs, alpha) if alpha > 0 else new_prob_costs

        deltas = [
            abs(new_costs[k] - costs[k])
            for k in vocab
            if costs[k] < 29 and new_costs[k] < 29
        ]
        max_delta = max(deltas) if deltas else 0.0
        costs = new_costs

        if max_delta < 0.01:
            if verbose:
                print(f"  Converged (max delta: {max_delta:.4f})")
            break

    return costs, segmentations, counts


def rust_like_prune(
    all_words,
    vocab,
    target_multi_budget,
    max_n,
    n_iters=10,
    alpha=0.0,
    shrinking_factor=0.75,
    verbose=True,
):
    current_vocab = dict(vocab)
    protected = {tok for tok in current_vocab if len(tok) == 1}
    target_vocab_size = len(protected) + target_multi_budget

    while True:
        start_size = len(current_vocab)
        costs, segmentations, counts = train_unigram_simple(
            all_words,
            current_vocab,
            max_n=max_n,
            n_iters=n_iters,
            alpha=alpha,
            verbose=verbose,
        )

        if len(current_vocab) <= target_vocab_size:
            return costs, segmentations

        losses = []
        for tok in current_vocab:
            if tok in protected:
                losses.append((tok, float("inf")))
                continue

            expected = counts.get(tok, 0)
            if expected <= 0:
                losses.append((tok, float("-inf")))
                continue

            alt_cost = best_alternative_cost(tok, costs, max_n=max_n)
            piece_cost = costs[tok]
            if alt_cost is None:
                losses.append((tok, float("inf")))
                continue

            approx_loss = expected * (alt_cost - piece_cost)
            losses.append((tok, approx_loss))

        losses.sort(key=lambda item: item[1])
        iter_target = math.ceil(len(current_vocab) * shrinking_factor)
        iter_target = max(iter_target, target_vocab_size)
        remove_count = len(current_vocab) - iter_target
        to_remove = {tok for tok, loss in losses if loss != float("inf")}
        to_remove = set(list(tok for tok, loss in losses if loss != float("inf"))[:remove_count])

        for tok in to_remove:
            current_vocab.pop(tok, None)

        removed = start_size - len(current_vocab)
        if verbose:
            print(f"  Prune round: removed {removed}, vocab now {len(current_vocab)}")
        if removed == 0:
            return costs, segmentations


def main():
    parser = argparse.ArgumentParser(
        description="Word-level Unigram Tokenizer over target_language_sentences_tokenization.jsonl"
    )
    parser.add_argument("corpus")
    parser.add_argument("--multi-budget", type=int, default=10000)
    parser.add_argument("--em-iters", type=int, default=10)
    parser.add_argument("--alpha", type=float, default=0.0)
    parser.add_argument("--min-freq", type=int, default=3)
    parser.add_argument("--max-n", type=int, default=5)
    parser.add_argument("--min-sent-len", type=int, default=3)
    parser.add_argument("--show-examples", type=int, default=10)
    parser.add_argument("--constrained", action="store_true")
    parser.add_argument("--rust-prune", action="store_true")
    parser.add_argument("--shrinking-factor", type=float, default=0.75)
    parser.add_argument("--initial-multi-budget", type=int)
    args = parser.parse_args()

    print("Loading corpus...")
    sentences, all_words = load_corpus(args.corpus, args.min_sent_len)
    print(f"  {len(sentences)} sentences total, {len(all_words)} with >= {args.min_sent_len} tokens")

    print("Building n-gram counts...")
    ngram_counts = build_ngram_counts(
        all_words,
        max_n=args.max_n,
        constrained=args.constrained,
    )

    initial_multi_budget = (
        args.initial_multi_budget
        if args.initial_multi_budget is not None
        else (args.multi_budget * 4 if args.rust_prune else args.multi_budget)
    )
    print(
        f"Building vocabulary (target multi-word budget: {args.multi_budget}, initial multi-word budget: {initial_multi_budget})..."
    )
    vocab = build_vocab(ngram_counts, initial_multi_budget, min_freq=args.min_freq)
    n_uni = sum(1 for k in vocab if len(k) == 1)
    n_multi = sum(1 for k in vocab if len(k) >= 2)
    print(f"  {len(vocab)} total ({n_uni} unigrams, {n_multi} multi-word)")

    print(
        f"\nTraining (alpha={args.alpha}, constrained={args.constrained}, rust_prune={args.rust_prune})..."
    )
    if args.rust_prune:
        costs, segmentations = rust_like_prune(
            all_words,
            vocab,
            target_multi_budget=args.multi_budget,
            max_n=args.max_n,
            n_iters=args.em_iters,
            alpha=args.alpha,
            shrinking_factor=args.shrinking_factor,
        )
    else:
        costs, segmentations, _ = train_unigram_simple(
            all_words,
            vocab,
            max_n=args.max_n,
            n_iters=args.em_iters,
            alpha=args.alpha,
        )

    total_words = sum(len(words) for words in all_words)
    total_tokens = sum(len(toks) for toks in segmentations)
    total_bits = sum(sum(costs[t] for t in toks) for toks in segmentations)
    usage = count_token_usage(segmentations)
    multi_usage = {t: c for t, c in usage.items() if len(t) >= 2}
    multi_pct = sum(multi_usage.values()) / total_tokens * 100 if total_tokens else 0.0

    print(f"\n{'=' * 60}")
    print("Results")
    print(f"{'=' * 60}")
    print(f"  Total words:    {total_words}")
    print(f"  Total tokens:   {total_tokens}")
    print(f"  Compression:    {total_words / total_tokens:.3f}x")
    print(f"  Total bits:     {total_bits:.0f}")
    print(f"  Bits/word:      {total_bits / total_words:.2f}")
    print(f"  Bits/token:     {total_bits / total_tokens:.2f}")
    print(f"  Multi-word tokens used: {len(multi_usage)}")
    print(f"  Multi-word token usage: {multi_pct:.1f}% of all token positions")

    print("\n  Top 30 multi-word tokens by usage:")
    for tok, count in sorted(multi_usage.items(), key=lambda x: -x[1])[:30]:
        label = " ".join(part.split("\t", 1)[0] for part in tok)
        pmi_parts = sum(costs.get((w,), 30.0) for w in tok)
        pmi = pmi_parts - costs.get(tok, 30.0)
        print(f"    {label:35s}  used={count:>5d}  PMI={pmi:>+.1f}")

    if args.show_examples > 0 and all_words:
        print("\n  Example segmentations:")
        random.seed(42)
        for idx in random.sample(range(len(all_words)), min(args.show_examples, len(all_words))):
            toks = segmentations[idx]
            print(
                "    "
                + " | ".join(" ".join(part.split('\t', 1)[0] for part in t) for t in toks)
            )


if __name__ == "__main__":
    main()
