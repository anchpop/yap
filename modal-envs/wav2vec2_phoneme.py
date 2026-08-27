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
# commit SHA below is preserved across the rename. WAV2VEC2_MODEL_ID points a
# run at a different HF repo (the eval harness compares repos this way).
MODEL_ID = os.environ.get("WAV2VEC2_MODEL_ID", "anchpop/lexide-pronunciation")
# Frozen to an exact commit SHA so a force-push to the HF repo can't silently
# change the weights production serves. The SHA also feeds the image
# weights-version and the verifier cache key, so a re-pin forces a clean
# rebuild + fresh predictions. Set WAV2VEC2_MODEL_REVISION to try a different
# checkpoint (the eval harness does this per-run); never point it at a branch
# name — an exact SHA is what makes the pin a pin.
MODEL_REVISION = os.environ.get(
    "WAV2VEC2_MODEL_REVISION", "edcbbbf43a7ff337f43d233a9d89566509715e63"
)

# Unique per-deploy identifier, echoed back by /predict so a caller can assert
# it's talking to the container it just deployed rather than a stale warm one —
# a *correct* freshness check (exact marker match), unlike inferring freshness
# from whether phoneme predictions changed. Defaults to the pinned revision so
# a re-pin always changes the marker; override per-deploy when two deploys
# share a revision and must still be told apart.
DEPLOY_MARKER = os.environ.get("WAV2VEC2_DEPLOY_MARKER", MODEL_REVISION[:12])

# 0=no stress, 1=primary (ˈ), 2=secondary (ˌ). Matches train/factorized_ctc.
STRESS_MARKS = {0: "", 1: "ˈ", 2: "ˌ"}

image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install("espeak-ng")
    .pip_install(
        "torch", "torchaudio", "transformers", "fastapi[standard]",
        "phonemizer", "huggingface_hub",
    )
    # Bake the resolved identity into the image. The three constants above are
    # read from the environment of whoever runs `modal deploy`, but the
    # container re-imports this module with its OWN environment — so without
    # this the remote worker would silently fall back to the defaults and serve
    # the production pin no matter what the deployer asked for. Baking them
    # here is what makes the values the local process chose survive the trip.
    # It also sits *before* run_commands, so changing any of them invalidates
    # the weights layer below and forces a genuinely fresh image.
    .env(
        {
            "WAV2VEC2_MODEL_ID": MODEL_ID,
            "WAV2VEC2_MODEL_REVISION": MODEL_REVISION,
            "WAV2VEC2_DEPLOY_MARKER": DEPLOY_MARKER,
        }
    )
    .run_commands(
        # Pre-pull both the backbone + the factorized_heads side-file so the
        # snapshot has everything on disk; otherwise cold start re-downloads.
        # The pin is interpolated straight into the cached layer command, so
        # re-pinning is itself the cache-buster: Modal sees a different command,
        # rebuilds the image, and re-downloads the weights. (A hand-maintained
        # version string here could drift from the pin.) DEPLOY_MARKER rides
        # along so the eval harness, which wants a guaranteed-fresh image even
        # when redeploying the *same* revision, gets one by varying the marker.
        f"echo 'MODEL_WEIGHTS_VERSION={MODEL_ID}@{MODEL_REVISION}#{DEPLOY_MARKER}' && "
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
    MLPs. See pronunciation/train/src/factorized_ctc.py.
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
    num_mixtures: int,
):
    """Modules for the regularized variant (articulatory-aux-regularized).

    The forward goes:
      backbone (output_hidden_states=True)  →  (L+1) layer outputs of dim H
      layer_weights:(K, L+1) softmax+sum    →  K mixtures of dim H, concat → K*H
      raw waveform → AcousticSidechannel (log-mel [+ low band]) → A
      cat([K*H, A]) → shared_base (Linear → GELU → Dropout) → head_base_dim
      heads(head_base_dim) on that shared base.
    """
    import torch.nn as nn

    layer_weights = nn.Parameter(
        __import__("torch").zeros(num_mixtures, num_layers_total)
    )
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
        layer_weights, shared_base,
        nonblank_head, phoneme_head, stress_head,
    )


# wav2vec2's conv feature extractor: 400-sample receptive field, 320-sample
# stride (50 fps). Copied from pronunciation/train/src/factorized_ctc.py, which
# is the reference implementation this must match bit-for-bit.
W2V2_RECEPTIVE_FIELD = 400
W2V2_STRIDE = 320
# Low-band linear-spectrogram channel: n_fft=2048 → 7.8125 Hz bins; bins
# [8, 78) span 62.5–601.6 Hz, bracketing adult F0.
LOWBAND_N_FFT = 2048
LOWBAND_BIN_LO = 8
LOWBAND_BIN_HI = 78
LOWBAND_BINS = LOWBAND_BIN_HI - LOWBAND_BIN_LO


def _build_sidechannel(ckpt):
    """Rebuild the checkpoint's acoustic side-channel.

    Port of `AcousticSidechannel` from the training repo. Two banks, both
    deterministic transforms of the waveform: a log-mel bank, plus (when
    `lowband_dim > 0`) a 2048-point low-band linear spectrogram restricted to
    the F0 range, which is what gives the tone / pitch-accent heads
    sub-semitone resolution.

    Handles both checkpoint layouts: the current one stores the whole module
    under `sidechannel`, while older ones stored bare `mel_norm`/`mel_proj`
    tensors with an implicit n_fft=400 mel-only bank.
    """
    import torch.nn as nn
    import torch.nn.functional as F
    import torchaudio.transforms as T_audio

    n_mels = int(ckpt.get("n_mels", 80))
    mel_proj_dim = int(ckpt.get("acoustic_dim", 64))
    mel_n_fft = int(ckpt.get("mel_n_fft", W2V2_RECEPTIVE_FIELD))
    lowband_dim = int(ckpt.get("lowband_dim", 0))

    class _Sidechannel(nn.Module):
        def __init__(self):
            super().__init__()
            self.mel_spec = T_audio.MelSpectrogram(
                sample_rate=16000, n_fft=mel_n_fft, win_length=mel_n_fft,
                hop_length=W2V2_STRIDE, n_mels=n_mels, center=False,
            )
            self.mel_norm = nn.LayerNorm(n_mels)
            self.mel_proj = nn.Linear(n_mels, mel_proj_dim)
            self.lowband_dim = lowband_dim
            if lowband_dim > 0:
                self.low_spec = T_audio.Spectrogram(
                    n_fft=LOWBAND_N_FFT, win_length=LOWBAND_N_FFT,
                    hop_length=W2V2_STRIDE, center=False, power=2.0,
                )
                self.low_norm = nn.LayerNorm(LOWBAND_BINS)
                self.low_proj = nn.Linear(LOWBAND_BINS, lowband_dim)
            else:
                self.low_spec = self.low_norm = self.low_proj = None
            self.out_dim = mel_proj_dim + lowband_dim

        def _bank(self, spec, norm, proj, input_values, T_target, dtype,
                  bin_lo=None, bin_hi=None):
            import torch
            # Symmetric pre-pad aligns this bank's window centers with
            # wav2vec2's conv receptive-field centers. Zero for the legacy
            # 400-point bank, so old checkpoints reproduce exactly.
            pad = (spec.n_fft - W2V2_RECEPTIVE_FIELD) // 2
            x = input_values.float()
            if pad:
                x = F.pad(x, (pad, pad))
            with torch.autocast(device_type=input_values.device.type, enabled=False):
                feats = spec(x)                                # (B, bins, T)
            if bin_lo is not None:
                feats = feats[:, bin_lo:bin_hi, :]
            feats = feats.transpose(1, 2)                      # (B, T, bins)
            if feats.shape[1] > T_target:
                feats = feats[:, :T_target, :]
            elif feats.shape[1] < T_target:
                feats = F.pad(feats, (0, 0, 0, T_target - feats.shape[1]))
            feats = torch.log(feats + 1e-6).to(dtype)
            return proj(norm(feats))

        def forward(self, input_values, T_target, dtype):
            import torch
            out = self._bank(self.mel_spec, self.mel_norm, self.mel_proj,
                             input_values, T_target, dtype)
            if self.low_spec is not None:
                low = self._bank(self.low_spec, self.low_norm, self.low_proj,
                                 input_values, T_target, dtype,
                                 bin_lo=LOWBAND_BIN_LO, bin_hi=LOWBAND_BIN_HI)
                out = torch.cat([out, low], dim=-1)
            return out

    module = _Sidechannel()
    if "sidechannel" in ckpt:
        module.load_state_dict(ckpt["sidechannel"])
    else:
        # Legacy layout: mel-only bank stored as two bare state dicts.
        module.mel_norm.load_state_dict(ckpt["mel_norm"])
        module.mel_proj.load_state_dict(ckpt["mel_proj"])
    return module


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
        # pronunciation/train/src/factorized_ctc.py (`AcousticSidechannel`).
        self.mel_sidechannel = bool(ckpt.get("mel_sidechannel", False))

        if self.mel_sidechannel:
            # --- mel-sidechannel variant: heads run on
            # cat([last_hidden_state, mel_proj]); heads are MLPs (mlp_heads).
            nonblank_head = _build_head_from_state(ckpt["nonblank_head"])
            phoneme_head = _build_head_from_state(ckpt["phoneme_head"])
            stress_head = _build_head_from_state(ckpt["stress_head"])
            nonblank_head.load_state_dict(ckpt["nonblank_head"])
            phoneme_head.load_state_dict(ckpt["phoneme_head"])
            stress_head.load_state_dict(ckpt["stress_head"])
            self.nonblank_head = nonblank_head.to("cuda").eval()
            self.phoneme_head = phoneme_head.to("cuda").eval()
            self.stress_head = stress_head.to("cuda").eval()
            self.sidechannel = _build_sidechannel(ckpt).to("cuda").eval()
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
            sidechannel = _build_sidechannel(ckpt)
            (
                layer_weights, shared_base,
                nonblank_head, phoneme_head, stress_head,
            ) = _build_regularized_modules(
                backbone_hidden=backbone_hidden,
                vocab_size=ckpt["vocab_size"],
                num_stress_labels=ckpt.get("num_stress_labels", 3),
                num_layers_total=num_layers_total,
                head_base_dim=head_base_dim,
                # The shared base consumes the *whole* side-channel, so its
                # input width tracks out_dim (mel + optional low band), not
                # the mel projection alone.
                acoustic_dim=sidechannel.out_dim,
                num_mixtures=num_mixtures,
            )
            with torch.no_grad():
                layer_weights.copy_(ckpt["layer_weights"])
            shared_base.load_state_dict(ckpt["shared_base"])
            nonblank_head.load_state_dict(ckpt["nonblank_head"])
            phoneme_head.load_state_dict(ckpt["phoneme_head"])
            stress_head.load_state_dict(ckpt["stress_head"])
            # Keep in fp32 — log/sigmoid math at the heads + 393-way softmax
            # is sensitive to precision, and these modules are tiny.
            self.layer_weights = nn.Parameter(layer_weights.to("cuda")).requires_grad_(False)
            self.shared_base = shared_base.to("cuda").eval()
            self.nonblank_head = nonblank_head.to("cuda").eval()
            self.phoneme_head = phoneme_head.to("cuda").eval()
            self.stress_head = stress_head.to("cuda").eval()
            self.sidechannel = sidechannel.to("cuda").eval()

        # Optional per-language auxiliary heads — Thai and Mandarin tone,
        # Japanese pitch accent — shipped by the multilingual checkpoints. They
        # run on the same head input as the phoneme head, so `_build_head_from_state`
        # gets their shapes from the saved state dict and this code stays
        # agnostic to which languages a checkpoint happens to cover. Older
        # checkpoints have neither key and simply get no aux heads.
        self.language_head_specs = dict(ckpt.get("language_head_specs") or {})
        self.language_heads = {}
        for name, state in (ckpt.get("language_heads") or {}).items():
            head = _build_head_from_state(state)
            head.load_state_dict(state)
            self.language_heads[name] = head.to("cuda").eval()

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

        acoustic = self.sidechannel(input_values, hidden.shape[1], hidden.dtype)
        combined = torch.cat([hidden, acoustic], dim=-1)  # (B, T, K*H + out_dim)
        return self.shared_base(combined)                  # (B, T, head_base_dim)

    def _compute_head_input_sidechannel(self, input_values):
        """Build the (1, T, H + acoustic_dim) head input for the mel-sidechannel
        variant: backbone last_hidden_state concatenated with the projected
        per-frame acoustic side-channel computed straight off the waveform. See
        pronunciation/train/src/factorized_ctc.py (`_compute_head_input`).
        """
        import torch
        import torch.nn.functional as F

        out = self.backbone(input_values)
        hidden = out.last_hidden_state.float()       # (1, T, H), fp32
        acoustic = self.sidechannel(input_values, hidden.shape[1], hidden.dtype)
        return torch.cat([hidden, acoustic], dim=-1)  # (1, T, H + out_dim)

    def _aux_heads_for(self, language: str | None) -> dict:
        """The aux heads that apply to `language`, keyed by their spec target
        ("tone", "pitch_accent"). Empty when the caller names no language or the
        checkpoint has no head for it, which is what keeps the response shape
        unchanged for the languages that never had tone supervision.
        """
        if not language:
            return {}
        return {
            spec["target"]: self.language_heads[name]
            for name, spec in self.language_head_specs.items()
            if spec.get("lang") == language and name in self.language_heads
        }

    def _forward(self, input_values, language: str | None = None):
        """Return (combined_log_probs, stress_logits, p_nonblank, aux_ids).

        The first three are (1, T, *); `aux_ids` maps each applicable aux
        target ("tone", "pitch_accent") to its per-frame argmax, (T,).

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
        aux_ids = {
            target: head(h)[0].argmax(dim=-1)                       # (T,)
            for target, head in self._aux_heads_for(language).items()
        }
        return log_probs, stress_logits, p_nonblank, aux_ids

    def _frame_matrix(self, log_probs) -> dict:
        """The full per-frame log-prob matrix, losslessly enough to rescore
        *any* phoneme sequence later without touching a GPU.

        `frames` (top-k) is a diagnostic view: it truncates to at most 100 of
        392 vocab entries, so a target phoneme that fell outside the top-k at
        some frame has no probability at all and CTC cannot be evaluated
        exactly. Storing the whole matrix instead makes the stored artifact
        target-agnostic — the expensive part (the forward pass) is done once,
        and any future question about any candidate transcription is then a
        cheap local computation.

        fp16 is well inside what this needs: these are log-probs used in a
        sum-exp, and fp16's ~3 decimal digits are far below the model's own
        uncertainty. Shipped zlib-compressed because a log-prob matrix is
        mostly near-identical very-negative values and compresses ~10x.
        `vocab` is included so the row order can never be misread later.
        """
        import base64, zlib

        # torch's own buffer, not numpy's — numpy is not in this image (torch
        # imports it lazily and warns when absent), and going through it would
        # add a dependency for a byte copy we can do directly.
        mat = log_probs[0].detach().to("cpu").half().contiguous()
        payload = zlib.compress(bytes(mat.untyped_storage()), 6)
        # Row labels come from the tokenizer's own vocab, NOT `_label`
        # (`decode`). The two disagree on 78 of 461 entries — decode renders
        # `<pad>` as `<blank>` and collapses some doubled forms — so labeling
        # rows with decode output produces a matrix whose indices can't be
        # looked up by the same token strings `_score_target` uses. A consumer
        # rescoring offline would then silently index the wrong rows.
        vocab = self.processor.tokenizer.get_vocab()
        by_id = {i: tok for tok, i in vocab.items()}
        return {
            "shape": list(mat.shape),                       # (T, V)
            "dtype": "float16",
            "encoding": "zlib+base64",
            "blank_id": self.blank_id,
            "vocab": [by_id.get(i, self._label(i)) for i in range(mat.shape[1])],
            "data": base64.b64encode(payload).decode(),
        }

    def _score_target(self, log_probs, target_phonemes: list) -> dict:
        """How well does the audio support *this specific* phoneme sequence?

        Greedy decode + edit distance answers a different question than the one
        we actually care about: it asks "what did the model think it heard, and
        does that string match?", which throws away the distribution and turns
        every soft disagreement (a phoneme the model split 60/40, a
        transcription-convention mismatch) into a hard edit. CTC scores the
        real question directly — the total probability of *all* frame
        alignments that spell the target — so mass the model put on the right
        answer still counts even when it lost the argmax.

        Returned as a likelihood ratio against the model's own free decode:
        `ratio = (logP(target) - logP(free)) / len(target)`, i.e. log-odds per
        phoneme of the claimed sentence versus the best explanation the model
        can offer for this audio. 0 means the target IS the model's preferred
        reading; more negative means the audio increasingly fails to support it.
        Scale-free across clip lengths, and needs no per-language equivalence
        table to be meaningful.
        """
        import torch
        import torch.nn.functional as F

        vocab = self.processor.tokenizer.get_vocab()
        ids, oov = [], []
        for tok in target_phonemes:
            if tok in vocab:
                ids.append(vocab[tok])
            else:
                oov.append(tok)
        if not ids:
            return {"error": "no target phoneme was in the model vocab", "oov": oov}

        lp = log_probs.transpose(0, 1).float()              # (T, 1, V), CTC layout
        T = lp.shape[0]

        def score(seq: list) -> float:
            if not seq or len(seq) > T:
                return float("nan")
            loss = F.ctc_loss(
                lp,
                torch.tensor([seq], dtype=torch.long, device=lp.device),
                torch.tensor([T], dtype=torch.long),
                torch.tensor([len(seq)], dtype=torch.long),
                blank=self.blank_id, reduction="none", zero_infinity=True,
            )
            return float(-loss[0].item())                   # loss is -logP

        # The model's own reading of this audio: greedy path, CTC-collapsed.
        # Its CTC score is the reference the target is measured against.
        pred = log_probs[0].argmax(dim=-1).tolist()
        free = []
        for i, tok in enumerate(pred):
            if tok != self.blank_id and (i == 0 or tok != pred[i - 1]):
                free.append(tok)

        logp_target = score(ids)
        logp_free = score(free) if free else float("nan")
        ratio = (
            (logp_target - logp_free) / len(ids)
            if logp_free == logp_free and logp_target == logp_target
            else float("nan")
        )
        return {
            "logp_target": logp_target,
            "logp_target_per_phoneme": logp_target / len(ids),
            "logp_free": logp_free,
            "ratio": ratio,
            "target_len": len(ids),
            "free_len": len(free),
            "oov": oov,
        }

    def _label(self, token_id: int) -> str:
        return "<blank>" if token_id == self.blank_id else self.processor.decode(token_id)

    def _decode_with_confidence(
        self, log_probs, stress_logits, aux_ids: dict, top_k: int = 3
    ) -> list[dict]:
        """Greedy CTC decode + per-emitted-phoneme top-k.

        Groups consecutive frames that predict the same token, averages their
        probabilities across the group, and returns one entry per collapsed
        phoneme (skipping blanks). Every frame-level label head — `stress`, plus
        whichever aux heads apply to the request's language — is read at the
        *first* frame of each emitted group (that head's call for the segment).
        """
        probs = log_probs[0].exp()                          # (T, V)
        predicted_ids = probs.argmax(dim=-1)                # (T,)
        # All the per-frame label heads, decoded the same way.
        label_heads = {"stress": stress_logits[0].argmax(dim=-1), **aux_ids}

        results = []
        prev_id = None
        group_probs = []
        group_labels = {}
        for t in range(len(predicted_ids)):
            tid = predicted_ids[t].item()
            if tid != prev_id:
                if prev_id is not None and prev_id != self.blank_id and group_probs:
                    results.append(
                        self._aggregate_group(group_probs, group_labels, top_k)
                    )
                group_probs = [probs[t]]
                group_labels = {k: int(v[t].item()) for k, v in label_heads.items()}
                prev_id = tid
            else:
                group_probs.append(probs[t])
        if prev_id is not None and prev_id != self.blank_id and group_probs:
            results.append(self._aggregate_group(group_probs, group_labels, top_k))
        return results

    def _aggregate_group(self, frame_probs, labels_at_onset: dict, top_k: int) -> dict:
        import torch

        avg_probs = torch.stack(frame_probs).mean(dim=0)  # (V,)
        top_k = min(top_k, avg_probs.numel())
        top_values, top_indices = torch.topk(avg_probs, top_k)
        labels = [self._label(idx.item()) for idx in top_indices]
        # Embed the predicted stress on the chosen phoneme so existing
        # callers that just read `phoneme` get IPA-with-stress for free.
        # Alternatives stay bare since stress is predicted separately and
        # doesn't vary across phoneme alternatives at a given position.
        chosen_with_stress = STRESS_MARKS[labels_at_onset["stress"]] + labels[0]
        return {
            "phoneme": chosen_with_stress,
            "confidence": round(top_values[0].item(), 4),
            "top_k": [
                {"phoneme": label, "probability": round(prob.item(), 4)}
                for label, prob in zip(labels, top_values)
            ],
            # `stress` plus any aux target ("tone", "pitch_accent") for the
            # requested language.
            **labels_at_onset,
        }

    def _frames_topk(
        self, log_probs, stress_logits, p_nonblank, aux_ids: dict, top_k: int
    ) -> list[dict]:
        """Per-frame top-k (no CTC collapse, blanks included).

        Each entry: {"frame", "stress", "p_nonblank", "top_k":
        [{"phoneme","probability"}]}, plus any aux target ("tone",
        "pitch_accent") that applies to the request's language. `p_nonblank` is
        the raw sigmoid output of the nonblank head — diagnostic for whether
        VAD-style training is making the model over-emit through silence
        (compare to unified to see if VAD-v2's nonblank fires high where
        unified's fires low).
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
                    **{k: int(v[t].item()) for k, v in aux_ids.items()},
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
        language: str | None = None,
        target_phonemes: list | None = None,
        return_frame_matrix: bool = False,
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
            log_probs, stress_logits, p_nonblank, aux_ids = self._forward(
                input_values, language
            )

        out = {
            "phonemes": self._decode_with_confidence(
                log_probs, stress_logits, aux_ids, top_k=top_k
            )
        }
        if target_phonemes:
            out["target_score"] = self._score_target(log_probs, target_phonemes)
        if return_frame_matrix:
            out["frame_matrix"] = self._frame_matrix(log_probs)
        if return_frames:
            out["frames"] = self._frames_topk(
                log_probs, stress_logits, p_nonblank, aux_ids, top_k=top_k
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
                    "model_revision": MODEL_REVISION,
                    # So a caller can discover which languages this checkpoint
                    # has aux (tone / pitch-accent) heads for.
                    "language_head_specs": self.language_head_specs}
        audio = request["audio"]
        sample_rate = int(request.get("sample_rate", 16000))
        top_k = min(max(int(request.get("top_k", 3)), 1), 100)
        return_frames = bool(request.get("return_frames", False))
        target_phonemes = request.get("target_phonemes") or None
        return_frame_matrix = bool(request.get("return_frame_matrix", False))
        # Opt-in: names which language's aux heads to run, e.g. "tha", "jpn",
        # "zho-hans". Omitted (or unknown) means phonemes + stress only.
        language = request.get("language")
        result = self.transcribe_phonemes.local(
            audio, sample_rate, top_k, return_frames, language, target_phonemes,
            return_frame_matrix,
        )
        # Stamp every prediction with the deploy marker so the verifier can
        # reject (and refuse to cache) responses served by a stale/contaminated
        # container — not just the one-shot marker_only probe.
        result["deploy_marker"] = DEPLOY_MARKER
        return result
