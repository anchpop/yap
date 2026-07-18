import os

import modal

# Production app name. The eval harness (scripts/compare_models.py) overrides
# this via WAV2VEC2_APP_NAME so model comparisons deploy to a *separate* app
# (wav2vec2-phoneme-eval) and never disturb the production endpoint that
# generate-data / yap-ai-backend depend on.
APP_NAME = os.environ.get("WAV2VEC2_APP_NAME", "wav2vec2-phoneme")
app = modal.App(APP_NAME)

# Container idle teardown. Contamination between back-to-back eval models is now
# guarded by the per-request deploy-marker check (the verifier rejects any
# response whose marker isn't the one just deployed), NOT by racing a 10s
# scaledown — so eval can keep its container warm through a long sequential
# verification run, avoiding mid-run cold-starts of the ~2GB backbone (which
# otherwise outrun Modal's web-endpoint timeout → 408s). Production keeps the
# longer window so user-facing requests rarely cold-start.
_IS_EVAL = APP_NAME != "wav2vec2-phoneme"
_SCALEDOWN_WINDOW = 180 if _IS_EVAL else 600

# Production champion (mel-sidechannel + MLP heads, degrade-augmented).
# Renamed on HF from lexide-pronunciation-vad-clean-sidechannel-degrade; the
# commit SHA below is preserved across the rename.
MODEL_ID = "anchpop/lexide-pronunciation"
# Frozen to an exact commit SHA so a force-push to the HF repo can't silently
# change the weights production serves. The SHA also feeds the image
# weights-version and the verifier cache key, so a re-pin forces a clean
# rebuild + fresh predictions. (The eval harness overrides this per-run.)
MODEL_REVISION = "00a661934cdd1179687b9500b2f08cc498964506"

# Unique per-deploy identifier. compare_models.py rewrites this on every
# deploy (same value it uses for the image cache-buster and cache subdir).
# The /predict endpoint echoes it back so the comparison harness can assert
# it's talking to the container it just deployed, not a stale warm one —
# this is a *correct* freshness check (exact marker match), unlike inferring
# freshness from whether phoneme predictions changed.
DEPLOY_MARKER = "default"

# 0=no stress, 1=primary (ˈ), 2=secondary (ˌ). Matches train/factorized_ctc.
STRESS_MARKS = {0: "", 1: "ˈ", 2: "ˌ"}

image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install("espeak-ng")
    .pip_install(
        "torch", "torchaudio", "transformers", "fastapi[standard]",
        "phonemizer", "huggingface_hub",
    )
    .run_commands(
        # Pre-pull both the backbone + the factorized_heads side-file so the
        # snapshot has everything on disk; otherwise cold start re-downloads.
        # The MODEL_WEIGHTS_VERSION echo IS part of the cached layer command,
        # so bumping it forces Modal to rebuild the image and re-download
        # weights when a new epoch is pushed to the model repo.
        "echo 'MODEL_WEIGHTS_VERSION=anchpop__lexide_pronunciation__00a661934cdd' && "
        "python -c \"from transformers import Wav2Vec2Model, Wav2Vec2Processor; "
        "from huggingface_hub import hf_hub_download; "
        f"Wav2Vec2Processor.from_pretrained('{MODEL_ID}', revision='{MODEL_REVISION}'); "
        f"Wav2Vec2Model.from_pretrained('{MODEL_ID}', revision='{MODEL_REVISION}'); "
        f"hf_hub_download('{MODEL_ID}', 'factorized_heads.pt', revision='{MODEL_REVISION}')\""
    )
)


def _build_simple_heads(hidden_size: int, vocab_size: int, num_stress_labels: int = 3):
    """Heads for the non-regularized variants (unified, unified-vad).

    Operate directly on the backbone's last_hidden_state (hidden_size dim).
    stress_head here is a small MLP — distinct from the regularized variant
    where it's a single Linear.
    """
    import torch.nn as nn

    nonblank_head = nn.Linear(hidden_size, 1)
    phoneme_head = nn.Linear(hidden_size, vocab_size)
    stress_head = nn.Sequential(
        nn.Linear(hidden_size, 256),
        nn.GELU(),
        nn.Dropout(0.1),
        nn.Linear(256, num_stress_labels),
    )
    return nonblank_head, phoneme_head, stress_head


def _build_head_from_state(state):
    """Reconstruct a head module matching a saved state_dict — either a plain
    ``nn.Linear`` ({weight, bias}) or a 2-layer MLP ``nn.Sequential`` (params at
    indices 0 and 3, i.e. Linear→GELU→Dropout→Linear). Shape-driven, so we never
    hard-code hidden dims; used by the mel-sidechannel variant whose heads are
    MLPs. See pronunciation/SIDECHANNEL_INFERENCE.md.
    """
    import torch.nn as nn

    if "weight" in state:
        w = state["weight"]
        return nn.Linear(w.shape[1], w.shape[0])
    w0, w3 = state["0.weight"], state["3.weight"]
    return nn.Sequential(
        nn.Linear(w0.shape[1], w0.shape[0]),
        nn.GELU(),
        nn.Dropout(0.1),
        nn.Linear(w3.shape[1], w3.shape[0]),
    )


def _build_regularized_modules(
    backbone_hidden: int,
    vocab_size: int,
    num_stress_labels: int,
    num_layers_total: int,
    head_base_dim: int,
    acoustic_dim: int,
    n_mels: int,
    num_mixtures: int,
):
    """Modules for the regularized variant (articulatory-aux-regularized).

    The forward goes:
      backbone (output_hidden_states=True)  →  (L+1) layer outputs of dim H
      layer_weights:(K, L+1) softmax+sum    →  K mixtures of dim H, concat → K*H
      raw waveform → log-mel → LayerNorm → Linear(n_mels → acoustic_dim) → A
      cat([K*H, A]) → shared_base (Linear → GELU → Dropout) → head_base_dim
      heads(head_base_dim) on that shared base.
    """
    import torch.nn as nn

    layer_weights = nn.Parameter(
        __import__("torch").zeros(num_mixtures, num_layers_total)
    )
    mel_norm = nn.LayerNorm(n_mels)
    mel_proj = nn.Linear(n_mels, acoustic_dim)
    shared_input = num_mixtures * backbone_hidden + acoustic_dim
    shared_base = nn.Sequential(
        nn.Linear(shared_input, head_base_dim),
        nn.GELU(),
        nn.Dropout(0.1),
    )
    nonblank_head = nn.Linear(head_base_dim, 1)
    phoneme_head = nn.Linear(head_base_dim, vocab_size)
    # In the regularized variant stress_head is a single Linear, not the
    # 2-layer MLP used in the simple variant.
    stress_head = nn.Linear(head_base_dim, num_stress_labels)
    return (
        layer_weights, mel_norm, mel_proj, shared_base,
        nonblank_head, phoneme_head, stress_head,
    )


@app.cls(
    gpu="T4",
    image=image,
    # Idle-container teardown, set per app above (_SCALEDOWN_WINDOW). Both eval
    # and production keep a multi-minute window so a long sequential run (eval)
    # or a sporadic user request (prod) doesn't cold-start the ~2GB backbone
    # mid-stream. Contamination between back-to-back eval models is guarded by
    # the per-request deploy-marker check, not by racing a short scaledown.
    scaledown_window=_SCALEDOWN_WINDOW,
    # Memory snapshot disabled. Modal's snapshot caches the loaded model
    # weights across redeploys; in practice it retains state from a prior
    # deploy of a *different* model checkpoint, so back-to-back comparisons
    # silently return the SAME model's predictions (we verified this by
    # diffing 403 cached predictions — 100% identical between two deploys
    # that should have served different architectures). Kept off for
    # production too: a stale snapshot serving the pre-re-pin model is a
    # correctness risk we won't take, and the long warm window above already
    # keeps cold starts rare. (Enabling it for prod is a future option for
    # faster cold starts, once snapshot-invalidation-on-re-pin is confirmed.)
    enable_memory_snapshot=False,
)
class Wav2Vec2Phoneme:
    @modal.enter()
    def load_model(self):
        # Catch load failures (e.g. a checkpoint whose head shapes don't match
        # this endpoint) and stash the traceback instead of crashing the
        # container. predict() then reports it, so the eval harness surfaces the
        # real error instead of mistaking a dead container for the
        # warm-container bug and spinning through redeploys.
        try:
            self._load_model_impl()
            self.load_error = None
        except Exception:
            import traceback
            self.load_error = traceback.format_exc()
            print(f"load_model FAILED:\n{self.load_error}", flush=True)

    def _load_model_impl(self):
        import torch
        import torch.nn as nn
        import torchaudio.transforms as T_audio
        from huggingface_hub import hf_hub_download
        from transformers import Wav2Vec2Model, Wav2Vec2Processor

        self.processor = Wav2Vec2Processor.from_pretrained(
            MODEL_ID, revision=MODEL_REVISION
        )

        # fp16 keeps the ~2GB safetensors backbone comfortably under T4 limits.
        self.backbone = Wav2Vec2Model.from_pretrained(
            MODEL_ID, revision=MODEL_REVISION, torch_dtype=torch.float16
        ).to("cuda")
        self.backbone.eval()
        backbone_hidden = self.backbone.config.hidden_size

        ckpt_path = hf_hub_download(MODEL_ID, "factorized_heads.pt", revision=MODEL_REVISION)
        ckpt = torch.load(ckpt_path, map_location="cpu", weights_only=False)
        self.blank_id = int(ckpt["blank_id"])
        # Tokens to mask out of the phoneme distribution. Old variants only
        # mask `blank_id`; the regularized variant ships an explicit list
        # (`masked_slots`) covering BOS/EOS/UNK/PAD.
        self.masked_slots = list(ckpt.get("masked_slots") or [self.blank_id])
        self.regularized = bool(ckpt.get("regularized_heads", False))
        # VAD-style variant with a log-mel side-channel concatenated onto
        # last_hidden_state, feeding 2-layer MLP heads (no layer mixing, no
        # shared base). Orthogonal to `regularized`. See
        # pronunciation/SIDECHANNEL_INFERENCE.md.
        self.mel_sidechannel = bool(ckpt.get("mel_sidechannel", False))

        if self.mel_sidechannel:
            # --- mel-sidechannel variant: heads run on
            # cat([last_hidden_state, mel_proj]); heads are MLPs (mlp_heads).
            acoustic_dim = int(ckpt.get("acoustic_dim", 64))
            n_mels = int(ckpt.get("n_mels", 80))
            nonblank_head = _build_head_from_state(ckpt["nonblank_head"])
            phoneme_head = _build_head_from_state(ckpt["phoneme_head"])
            stress_head = _build_head_from_state(ckpt["stress_head"])
            nonblank_head.load_state_dict(ckpt["nonblank_head"])
            phoneme_head.load_state_dict(ckpt["phoneme_head"])
            stress_head.load_state_dict(ckpt["stress_head"])
            mel_norm = nn.LayerNorm(n_mels)
            mel_norm.load_state_dict(ckpt["mel_norm"])
            mel_proj = nn.Linear(n_mels, acoustic_dim)
            mel_proj.load_state_dict(ckpt["mel_proj"])
            self.nonblank_head = nonblank_head.to("cuda").eval()
            self.phoneme_head = phoneme_head.to("cuda").eval()
            self.stress_head = stress_head.to("cuda").eval()
            self.mel_norm = mel_norm.to("cuda").eval()
            self.mel_proj = mel_proj.to("cuda").eval()
            # Matches train/factorized_ctc MelSpectrogram (center=False).
            self.mel_spec = T_audio.MelSpectrogram(
                sample_rate=16000, n_fft=400, win_length=400,
                hop_length=320, n_mels=n_mels, center=False,
            ).to("cuda")
        elif not self.regularized:
            # --- Plain / VAD-style variant: heads operate on last_hidden_state.
            nonblank_head, phoneme_head, stress_head = _build_simple_heads(
                hidden_size=backbone_hidden,
                vocab_size=ckpt["vocab_size"],
                num_stress_labels=ckpt.get("num_stress_labels", 3),
            )
            nonblank_head.load_state_dict(ckpt["nonblank_head"])
            phoneme_head.load_state_dict(ckpt["phoneme_head"])
            stress_head.load_state_dict(ckpt["stress_head"])
            self.nonblank_head = nonblank_head.to("cuda").eval()
            self.phoneme_head = phoneme_head.to("cuda").eval()
            self.stress_head = stress_head.to("cuda").eval()
        else:
            # --- Regularized variant: multi-layer weighted sum + log-mel
            # side-channel → shared Linear→GELU base → heads.
            num_layers_total = self.backbone.config.num_hidden_layers + 1  # +1 for embedding output
            num_mixtures = ckpt["layer_weights"].shape[0]
            head_base_dim = int(ckpt.get("head_base_dim", 768))
            acoustic_dim = int(ckpt.get("acoustic_dim", 64))
            n_mels = int(ckpt.get("n_mels", 80))
            (
                layer_weights, mel_norm, mel_proj, shared_base,
                nonblank_head, phoneme_head, stress_head,
            ) = _build_regularized_modules(
                backbone_hidden=backbone_hidden,
                vocab_size=ckpt["vocab_size"],
                num_stress_labels=ckpt.get("num_stress_labels", 3),
                num_layers_total=num_layers_total,
                head_base_dim=head_base_dim,
                acoustic_dim=acoustic_dim,
                n_mels=n_mels,
                num_mixtures=num_mixtures,
            )
            with torch.no_grad():
                layer_weights.copy_(ckpt["layer_weights"])
            mel_norm.load_state_dict(ckpt["mel_norm"])
            mel_proj.load_state_dict(ckpt["mel_proj"])
            shared_base.load_state_dict(ckpt["shared_base"])
            nonblank_head.load_state_dict(ckpt["nonblank_head"])
            phoneme_head.load_state_dict(ckpt["phoneme_head"])
            stress_head.load_state_dict(ckpt["stress_head"])
            # Keep in fp32 — log/sigmoid math at the heads + 393-way softmax
            # is sensitive to precision, and these modules are tiny.
            self.layer_weights = nn.Parameter(layer_weights.to("cuda")).requires_grad_(False)
            self.mel_norm = mel_norm.to("cuda").eval()
            self.mel_proj = mel_proj.to("cuda").eval()
            self.shared_base = shared_base.to("cuda").eval()
            self.nonblank_head = nonblank_head.to("cuda").eval()
            self.phoneme_head = phoneme_head.to("cuda").eval()
            self.stress_head = stress_head.to("cuda").eval()
            # log-mel spectrogram. Matches train/factorized_ctc:
            # n_fft=400, win_length=400, hop_length=320, n_mels=80, center=False.
            self.mel_spec = T_audio.MelSpectrogram(
                sample_rate=16000, n_fft=400, win_length=400,
                hop_length=320, n_mels=n_mels, center=False,
            ).to("cuda")

        # Warmup forward pass so JIT/CUDA initialization is captured in the
        # snapshot (covers both backbone and head paths).
        dummy = self.processor(
            [0.0] * 16000, sampling_rate=16000, return_tensors="pt", padding=True
        )
        with torch.no_grad():
            self._forward(dummy.input_values.to("cuda").to(torch.float16))

    def _compute_head_input_regularized(self, input_values):
        """Build the (1, T, head_base_dim) shared base feature for the
        regularized variant. See pronunciation/train/src/factorized_ctc.py
        FactorizedCTCModel._compute_head_input for the reference impl.
        """
        import torch
        import torch.nn.functional as F

        out = self.backbone(input_values, output_hidden_states=True)
        # tuple of (L+1) tensors, each (B, T, backbone_hidden), fp16
        hidden_states = out.hidden_states

        weights = F.softmax(self.layer_weights.float(), dim=1)  # (K, L+1)
        K = weights.shape[0]

        # Streaming weighted sum per mixture to avoid materializing a giant
        # (L+1, B, T, H) intermediate. Cast each layer to fp32 for accumulation.
        mixtures = []
        for k in range(K):
            w_k = weights[k]
            mix_k = sum(
                w * h.float() for w, h in zip(w_k, hidden_states)
            )
            mixtures.append(mix_k)
        hidden = torch.cat(mixtures, dim=-1)  # (B, T, K * backbone_hidden)

        # Mel spectrogram off the raw waveform (fp32, MelSpec dislikes fp16).
        with torch.autocast(device_type="cuda", enabled=False):
            mel = self.mel_spec(input_values.float())  # (B, n_mels, T_mel)
        mel = mel.transpose(1, 2)  # (B, T_mel, n_mels)

        # Both encoder + mel run at ~50 fps via hop=320 but edge frames can
        # differ by ±1. Pad-or-truncate the mel side to match encoder T.
        T_target = hidden.shape[1]
        if mel.shape[1] > T_target:
            mel = mel[:, :T_target, :]
        elif mel.shape[1] < T_target:
            mel = F.pad(mel, (0, 0, 0, T_target - mel.shape[1]))

        mel = torch.log(mel + 1e-6)
        mel = self.mel_norm(mel)                   # LayerNorm over n_mels
        mel_proj = self.mel_proj(mel)              # (B, T, acoustic_dim)

        combined = torch.cat([hidden, mel_proj], dim=-1)  # (B, T, 9664)
        return self.shared_base(combined)                  # (B, T, head_base_dim)

    def _compute_head_input_sidechannel(self, input_values):
        """Build the (1, T, H + acoustic_dim) head input for the mel-sidechannel
        variant: backbone last_hidden_state concatenated with a projected
        per-frame log-mel side-channel computed straight off the waveform. See
        pronunciation/SIDECHANNEL_INFERENCE.md (`_compute_head_input`).
        """
        import torch
        import torch.nn.functional as F

        out = self.backbone(input_values)
        hidden = out.last_hidden_state.float()       # (1, T, H), fp32

        # Mel off the raw waveform (fp32; MelSpec dislikes fp16/autocast).
        with torch.autocast(device_type="cuda", enabled=False):
            mel = self.mel_spec(input_values.float())  # (1, n_mels, T_mel)
        mel = mel.transpose(1, 2)                       # (1, T_mel, n_mels)

        # Encoder + mel both ~50 fps (hop=320) but edges differ by ±1.
        T_target = hidden.shape[1]
        if mel.shape[1] > T_target:
            mel = mel[:, :T_target, :]
        elif mel.shape[1] < T_target:
            mel = F.pad(mel, (0, 0, 0, T_target - mel.shape[1]))

        mel = torch.log(mel + 1e-6)
        mel = self.mel_norm(mel)                        # LayerNorm over n_mels
        mel_proj = self.mel_proj(mel)                   # (1, T, acoustic_dim)
        return torch.cat([hidden, mel_proj], dim=-1)    # (1, T, H + acoustic_dim)

    def _forward(self, input_values):
        """Return (combined_log_probs, stress_logits, p_nonblank) — all (1, T, *).

        Branches on the head-input variant:
        - simple: heads run on backbone's last_hidden_state.
        - regularized: heads run on a 768-dim shared base built from 5
          weighted-layer mixtures plus a log-mel side-channel.
        - mel-sidechannel: heads run on last_hidden_state concatenated with a
          projected log-mel side-channel (MLP heads).
        """
        import torch
        import torch.nn.functional as F

        if self.regularized:
            h = self._compute_head_input_regularized(input_values)  # (1, T, 768), fp32
        elif self.mel_sidechannel:
            h = self._compute_head_input_sidechannel(input_values)  # (1, T, H+A), fp32
        else:
            out = self.backbone(input_values)
            h = out.last_hidden_state.float()                       # (1, T, H), fp32

        l_nb = self.nonblank_head(h).squeeze(-1)                    # (1, T)
        l_ph = self.phoneme_head(h).clone()                         # (1, T, V)
        # Mask special tokens (blank, BOS, EOS, UNK depending on variant) out
        # of the phoneme distribution so they can't win argmax. blank gets
        # its log-prob filled back in below from the nonblank head.
        for slot in self.masked_slots:
            l_ph[..., slot] = float("-inf")

        log_p_blank = F.logsigmoid(-l_nb)                           # (1, T)
        log_p_nonblank = F.logsigmoid(l_nb)                         # (1, T)
        log_p_phonemes = log_p_nonblank.unsqueeze(-1) + F.log_softmax(l_ph, dim=-1)
        log_probs = log_p_phonemes.clone()
        log_probs[..., self.blank_id] = log_p_blank                 # (1, T, V)

        stress_logits = self.stress_head(h)                         # (1, T, 3)
        p_nonblank = torch.sigmoid(l_nb)                            # (1, T)
        return log_probs, stress_logits, p_nonblank

    def _label(self, token_id: int) -> str:
        return "<blank>" if token_id == self.blank_id else self.processor.decode(token_id)

    def _decode_with_confidence(
        self, log_probs, stress_logits, top_k: int = 3
    ) -> list[dict]:
        """Greedy CTC decode + per-emitted-phoneme top-k.

        Groups consecutive frames that predict the same token, averages their
        probabilities across the group, and returns one entry per collapsed
        phoneme (skipping blanks). The `stress` field is the stress head's
        prediction at the *first* frame of each emitted group (the model's
        stress call for that segment).
        """
        import torch

        probs = log_probs[0].exp()                          # (T, V)
        predicted_ids = probs.argmax(dim=-1)                # (T,)
        stress_ids = stress_logits[0].argmax(dim=-1)        # (T,)

        results = []
        prev_id = None
        group_probs = []
        group_stress = None
        for t in range(len(predicted_ids)):
            tid = predicted_ids[t].item()
            if tid != prev_id:
                if prev_id is not None and prev_id != self.blank_id and group_probs:
                    results.append(
                        self._aggregate_group(group_probs, group_stress, top_k)
                    )
                group_probs = [probs[t]]
                group_stress = stress_ids[t].item()
                prev_id = tid
            else:
                group_probs.append(probs[t])
        if prev_id is not None and prev_id != self.blank_id and group_probs:
            results.append(self._aggregate_group(group_probs, group_stress, top_k))
        return results

    def _aggregate_group(self, frame_probs, stress: int, top_k: int) -> dict:
        import torch

        avg_probs = torch.stack(frame_probs).mean(dim=0)  # (V,)
        top_k = min(top_k, avg_probs.numel())
        top_values, top_indices = torch.topk(avg_probs, top_k)
        labels = [self._label(idx.item()) for idx in top_indices]
        # Embed the predicted stress on the chosen phoneme so existing
        # callers that just read `phoneme` get IPA-with-stress for free.
        # Alternatives stay bare since stress is predicted separately and
        # doesn't vary across phoneme alternatives at a given position.
        chosen_with_stress = STRESS_MARKS[int(stress)] + labels[0]
        return {
            "phoneme": chosen_with_stress,
            "stress": int(stress),
            "confidence": round(top_values[0].item(), 4),
            "top_k": [
                {"phoneme": label, "probability": round(prob.item(), 4)}
                for label, prob in zip(labels, top_values)
            ],
        }

    def _frames_topk(
        self, log_probs, stress_logits, p_nonblank, top_k: int
    ) -> list[dict]:
        """Per-frame top-k (no CTC collapse, blanks included).

        Each entry: {"frame", "stress", "p_nonblank", "top_k":
        [{"phoneme","probability"}]}. `p_nonblank` is the raw sigmoid output
        of the nonblank head — diagnostic for whether VAD-style training is
        making the model over-emit through silence (compare to unified to
        see if VAD-v2's nonblank fires high where unified's fires low).
        """
        probs = log_probs[0].exp()                          # (T, V)
        stress_ids = stress_logits[0].argmax(dim=-1)        # (T,)
        nb = p_nonblank[0]                                  # (T,)
        top_k = min(top_k, probs.shape[-1])
        top_values, top_indices = probs.topk(top_k, dim=-1)

        frames = []
        for t in range(probs.shape[0]):
            entries = [
                {
                    "phoneme": self._label(top_indices[t, r].item()),
                    "probability": round(top_values[t, r].item(), 4),
                }
                for r in range(top_k)
            ]
            frames.append(
                {
                    "frame": t,
                    "stress": int(stress_ids[t].item()),
                    "p_nonblank": round(nb[t].item(), 4),
                    "top_k": entries,
                }
            )
        return frames

    @modal.method()
    def transcribe_phonemes(
        self,
        audio_samples: list[float],
        sample_rate: int = 16000,
        top_k: int = 3,
        return_frames: bool = False,
    ) -> dict:
        import torch
        import torchaudio.functional as F

        if sample_rate != 16000:
            samples_tensor = torch.tensor(audio_samples).unsqueeze(0)
            samples_tensor = F.resample(samples_tensor, sample_rate, 16000)
            audio_samples = samples_tensor.squeeze(0).tolist()

        inputs = self.processor(
            audio_samples, sampling_rate=16000, return_tensors="pt", padding=True
        )
        input_values = inputs.input_values.to("cuda").to(torch.float16)

        with torch.no_grad():
            log_probs, stress_logits, p_nonblank = self._forward(input_values)

        out = {
            "phonemes": self._decode_with_confidence(log_probs, stress_logits, top_k=top_k)
        }
        if return_frames:
            out["frames"] = self._frames_topk(
                log_probs, stress_logits, p_nonblank, top_k=top_k
            )
        return out

    @modal.fastapi_endpoint(method="POST")
    def predict(self, request: dict) -> dict:
        # If the model failed to load, report the stashed traceback. The
        # marker_only probe (eval harness) gets it as a 200 JSON so it can abort
        # with the real trace; a NORMAL prediction request gets a 503 so
        # production callers (yap-ai-backend, the verifier) hit their
        # status-error path instead of parsing a phonemes-less 200.
        if self.load_error is not None:
            if request.get("marker_only"):
                return {"load_error": self.load_error, "deploy_marker": DEPLOY_MARKER}
            from fastapi import HTTPException
            raise HTTPException(
                status_code=503,
                detail={"load_error": self.load_error, "deploy_marker": DEPLOY_MARKER},
            )
        # Freshness check: `{"marker_only": true}` short-circuits inference
        # and returns just this container's deploy marker, so the comparison
        # harness can confirm it's hitting the container it just deployed
        # (not a stale warm one) without paying for a full forward pass.
        if request.get("marker_only"):
            return {"deploy_marker": DEPLOY_MARKER, "model_id": MODEL_ID,
                    "model_revision": MODEL_REVISION}
        audio = request["audio"]
        sample_rate = int(request.get("sample_rate", 16000))
        top_k = min(max(int(request.get("top_k", 3)), 1), 100)
        return_frames = bool(request.get("return_frames", False))
        result = self.transcribe_phonemes.local(
            audio, sample_rate, top_k, return_frames
        )
        # Stamp every prediction with the deploy marker so the verifier can
        # reject (and refuse to cache) responses served by a stale/contaminated
        # container — not just the one-shot marker_only probe.
        result["deploy_marker"] = DEPLOY_MARKER
        return result
