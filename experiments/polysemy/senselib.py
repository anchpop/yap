"""Shared helpers: contextual token embeddings from a bidirectional encoder."""

import base64
import json
import re
import urllib.request

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


class ModalEmbedder:
    """Same embed_spans contract as Embedder (fixed-layer form only), served by
    the deployed token-embeddings Modal endpoint (bge-m3 @ layer 17, GPU) —
    parity with the local path is cosine 1.000, but ~100x faster than CPU."""

    URL = "https://anchpop--token-embeddings-tokenembedder-embed.modal.run"
    MARKER = "5617a9f61b02@L17"
    LAYER = 17

    def embed_spans(
        self,
        sentences: list[str],
        spans: list[tuple[int, int]],
        batch_size: int = 256,
        layer: int | None = None,
    ) -> np.ndarray:
        assert layer == self.LAYER, f"endpoint serves layer {self.LAYER} only"
        out = []
        for i in range(0, len(sentences), batch_size):
            payload = {
                "sentences": [
                    {"text": t, "spans": [list(s)]}
                    for t, s in zip(
                        sentences[i : i + batch_size], spans[i : i + batch_size]
                    )
                ]
            }
            req = urllib.request.Request(
                self.URL,
                json.dumps(payload).encode(),
                {"Content-Type": "application/json"},
            )
            resp = json.loads(urllib.request.urlopen(req, timeout=600).read())
            assert resp["deploy_marker"] == self.MARKER, resp["deploy_marker"]
            for b64 in resp["vectors"]:
                vec = np.frombuffer(base64.b64decode(b64), dtype=np.float16)
                out.append(vec.astype(np.float32))
        return np.stack(out)


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
