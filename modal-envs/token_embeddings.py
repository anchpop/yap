"""Per-token contextual embeddings for sense discrimination (polysemy).

Serves subword-mean-pooled hidden-state vectors for requested char spans in
each sentence, from a fixed layer of a multilingual bidirectional encoder.
Chosen via experiments/polysemy probe sweeps (see memory + experiments dir):
one model + one layer shared by every course language.

Same deploy pattern as wav2vec2_phoneme.py: identity is baked into the image
env so the remote worker can't silently fall back to defaults, and every
response carries a deploy marker the caller can assert on.
"""

import os

import modal

APP_NAME = os.environ.get("TOKEN_EMBED_APP_NAME", "token-embeddings")
app = modal.App(APP_NAME)

MODEL_ID = os.environ.get("TOKEN_EMBED_MODEL_ID", "BAAI/bge-m3")
# Frozen to an exact commit SHA (same reasoning as wav2vec2_phoneme.py: a pin
# is only a pin if it can't move).
MODEL_REVISION = os.environ.get(
    "TOKEN_EMBED_MODEL_REVISION", "5617a9f61b028005a4858fdac845db406aefb181"
)
# Hidden-state layer to serve (0 = embedding layer). Fixed across languages.
LAYER = int(os.environ.get("TOKEN_EMBED_LAYER", "17"))
DEPLOY_MARKER = os.environ.get(
    "TOKEN_EMBED_DEPLOY_MARKER", f"{MODEL_REVISION[:12]}@L{LAYER}"
)

image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install("torch", "transformers", "fastapi[standard]", "huggingface_hub")
    # Bake resolved identity into the image (see wav2vec2_phoneme.py for why:
    # the container re-imports this module with its OWN environment).
    .env(
        {
            "TOKEN_EMBED_MODEL_ID": MODEL_ID,
            "TOKEN_EMBED_MODEL_REVISION": MODEL_REVISION,
            "TOKEN_EMBED_LAYER": str(LAYER),
            "TOKEN_EMBED_DEPLOY_MARKER": DEPLOY_MARKER,
        }
    )
    .run_commands(
        # Pre-pull weights so cold start doesn't re-download; the pin
        # interpolated into the command is itself the cache-buster.
        f"echo 'MODEL_WEIGHTS_VERSION={MODEL_ID}@{MODEL_REVISION}#{DEPLOY_MARKER}' && "
        "python -c \"from transformers import AutoModel, AutoTokenizer; "
        f"AutoTokenizer.from_pretrained('{MODEL_ID}', revision='{MODEL_REVISION}'); "
        f"AutoModel.from_pretrained('{MODEL_ID}', revision='{MODEL_REVISION}')\""
    )
)


@app.cls(gpu="L4", image=image, scaledown_window=300, max_containers=10)
class TokenEmbedder:
    @modal.enter()
    def load_model(self):
        try:
            import torch
            from transformers import AutoModel, AutoTokenizer

            self.tokenizer = AutoTokenizer.from_pretrained(
                MODEL_ID, revision=MODEL_REVISION
            )
            self.model = (
                AutoModel.from_pretrained(
                    MODEL_ID, revision=MODEL_REVISION, torch_dtype=torch.float16
                )
                .to("cuda")
                .eval()
            )
            # We only ever serve hidden_states[LAYER]; layers past it are pure
            # waste. hidden_states[k] is the output of encoder layer k-1
            # (index 0 is the embedding layer), so keeping the first LAYER
            # encoder layers leaves hidden_states[LAYER] bit-identical.
            self.model.encoder.layer = torch.nn.ModuleList(
                self.model.encoder.layer[:LAYER]
            )
            self.load_error = None
        except Exception:
            import traceback

            self.load_error = traceback.format_exc()
            print(f"load_model FAILED:\n{self.load_error}", flush=True)

    def _embed_batch(self, texts: list[str], spans: list[list[list[int]]]):
        """One forward pass; returns per-sentence list of pooled span vectors
        (fp16 torch tensors on CPU)."""
        import torch

        enc = self.tokenizer(
            texts,
            return_tensors="pt",
            padding=True,
            truncation=True,
            max_length=512,
            return_offsets_mapping=True,
        )
        offsets = enc.pop("offset_mapping")
        enc = {k: v.to("cuda") for k, v in enc.items()}
        with torch.no_grad():
            hidden = self.model(**enc, output_hidden_states=True).hidden_states[LAYER]
        hidden = hidden.float().cpu()  # (B, T, H) — pool in fp32, ship fp16
        out = []
        for j, sent_spans in enumerate(spans):
            offs = offsets[j].tolist()
            vecs = []
            for a, b in sent_spans:
                idx = [k for k, (x, y) in enumerate(offs) if x != y and x < b and y > a]
                if idx:
                    vecs.append(hidden[j, idx, :].mean(dim=0).to(torch.float16))
                else:
                    # Span fell entirely past truncation (or degenerate): all
                    # zeros, caller-detectable, never silently wrong.
                    vecs.append(
                        torch.zeros(hidden.shape[-1], dtype=torch.float16)
                    )
            out.append(vecs)
        return out

    @modal.fastapi_endpoint(method="POST")
    def embed(self, request: dict) -> dict:
        """{"sentences": [{"text": str, "spans": [[a, b], ...]}, ...]}
        → {"dim", "vectors": [base64(concat little-endian f16 per span)], ...}
        """
        if self.load_error is not None:
            if request.get("marker_only"):
                return {"load_error": self.load_error, "deploy_marker": DEPLOY_MARKER}
            from fastapi import HTTPException

            raise HTTPException(
                status_code=503,
                detail={"load_error": self.load_error, "deploy_marker": DEPLOY_MARKER},
            )
        if request.get("marker_only"):
            return {
                "deploy_marker": DEPLOY_MARKER,
                "model_id": MODEL_ID,
                "model_revision": MODEL_REVISION,
                "layer": LAYER,
            }

        import base64

        import torch

        sentences = request["sentences"]
        texts = [s["text"] for s in sentences]
        spans = [s["spans"] for s in sentences]
        vectors: list[str] = []
        dim = None
        BATCH = 256
        for i in range(0, len(texts), BATCH):
            for vecs in self._embed_batch(texts[i : i + BATCH], spans[i : i + BATCH]):
                if vecs:
                    dim = vecs[0].shape[-1]
                    flat = torch.stack(vecs).contiguous()
                    vectors.append(base64.b64encode(flat.numpy().tobytes()).decode())
                else:
                    vectors.append("")
        return {
            "dim": dim,
            "vectors": vectors,
            "model_id": MODEL_ID,
            "layer": LAYER,
            "deploy_marker": DEPLOY_MARKER,
        }
