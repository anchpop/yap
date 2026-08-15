"""Shared helpers: contextual token embeddings from a bidirectional encoder."""

import re

import numpy as np
import torch
from transformers import AutoModel, AutoTokenizer

BRACE_RE = re.compile(r"\{([^}]+)\}")


def parse_braced(sentence: str) -> tuple[str, tuple[int, int]]:
    """'He shed a {tear}.' -> ('He shed a tear.', (10, 14))"""
    m = BRACE_RE.search(sentence)
    assert m, sentence
    clean = sentence[: m.start()] + m.group(1) + sentence[m.end() :]
    return clean, (m.start(), m.start() + len(m.group(1)))


class Embedder:
    def __init__(self, model_name: str, device: str = "cpu"):
        self.tokenizer = AutoTokenizer.from_pretrained(model_name, trust_remote_code=True)
        self.model = (
            AutoModel.from_pretrained(model_name, trust_remote_code=True).to(device).eval()
        )
        self.device = device

    @torch.no_grad()
    def embed_spans(
        self,
        sentences: list[str],
        spans: list[tuple[int, int]],
        batch_size: int = 32,
        layer: int | None = None,
    ) -> np.ndarray:
        """Per-layer embeddings of the word at `span` in each sentence.

        Returns (n_sentences, n_layers+1, hidden) — mean-pooled over the
        subword tokens overlapping the char span, per hidden-state layer
        (layer 0 = embedding layer). With `layer` set, returns just that
        layer: (n_sentences, hidden).
        """
        out = []
        for i in range(0, len(sentences), batch_size):
            batch = sentences[i : i + batch_size]
            batch_spans = spans[i : i + batch_size]
            enc = self.tokenizer(
                batch,
                return_tensors="pt",
                padding=True,
                truncation=True,
                return_offsets_mapping=True,
            )
            offsets = enc.pop("offset_mapping")
            enc = {k: v.to(self.device) for k, v in enc.items()}
            hidden = self.model(**enc, output_hidden_states=True).hidden_states
            if layer is not None:
                hidden = (hidden[layer],)
            # (layers, batch, seq, hidden)
            hs = torch.stack(hidden, dim=0).cpu()
            for j, (start, end) in enumerate(batch_spans):
                tok_mask = [
                    k
                    for k, (a, b) in enumerate(offsets[j].tolist())
                    if a != b and a < end and b > start
                ]
                assert tok_mask, (batch[j], (start, end), offsets[j])
                vec = hs[:, j, tok_mask, :].mean(dim=1)  # (layers, hidden)
                out.append(vec.numpy())
        stacked = np.stack(out)  # (n, layers, hidden)
        return stacked[:, 0, :] if layer is not None else stacked
