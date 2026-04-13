import modal

app = modal.App("wav2vec2-phoneme")

image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install("torch", "transformers", "fastapi[standard]")
    .run_commands(
        "python -c \"from transformers import Wav2Vec2ForCTC, Wav2Vec2Processor; "
        "Wav2Vec2Processor.from_pretrained('facebook/wav2vec2-lv-60-espeak-cv-ft'); "
        "Wav2Vec2ForCTC.from_pretrained('facebook/wav2vec2-lv-60-espeak-cv-ft')\""
    )
)


@app.cls(gpu="T4", image=image, container_idle_timeout=300)
class Wav2Vec2Phoneme:
    @modal.enter()
    def load_model(self):
        import torch
        from transformers import Wav2Vec2ForCTC, Wav2Vec2Processor

        self.processor = Wav2Vec2Processor.from_pretrained(
            "facebook/wav2vec2-lv-60-espeak-cv-ft"
        )
        self.model = Wav2Vec2ForCTC.from_pretrained(
            "facebook/wav2vec2-lv-60-espeak-cv-ft"
        ).to("cuda")
        self.model.eval()

    def _decode_with_confidence(self, logits, top_k: int = 3) -> list[dict]:
        """CTC-decode logits and return per-phoneme confidence + top-k alternatives.

        Groups consecutive frames that predict the same token, averages their
        probabilities, and returns one entry per collapsed phoneme (skipping blanks).
        """
        import torch

        probs = torch.softmax(logits[0], dim=-1)  # (frames, vocab_size)
        predicted_ids = torch.argmax(probs, dim=-1)  # (frames,)

        blank_id = self.processor.tokenizer.pad_token_id

        # Group consecutive identical predictions
        phoneme_results = []
        prev_id = None
        frame_group_probs = []

        for frame_idx in range(len(predicted_ids)):
            token_id = predicted_ids[frame_idx].item()

            if token_id != prev_id:
                # Emit previous group if it wasn't blank
                if prev_id is not None and prev_id != blank_id and frame_group_probs:
                    phoneme_results.append(self._aggregate_group(frame_group_probs, top_k))
                frame_group_probs = [probs[frame_idx]]
                prev_id = token_id
            else:
                frame_group_probs.append(probs[frame_idx])

        # Don't forget the last group
        if prev_id is not None and prev_id != blank_id and frame_group_probs:
            phoneme_results.append(self._aggregate_group(frame_group_probs, top_k))

        return phoneme_results

    def _aggregate_group(self, frame_probs, top_k: int) -> dict:
        """Average probabilities across frames in a CTC group, return top-k."""
        import torch

        avg_probs = torch.stack(frame_probs).mean(dim=0)  # (vocab_size,)
        top_values, top_indices = torch.topk(avg_probs, top_k)

        labels = [self.processor.decode(idx.item()) for idx in top_indices]
        return {
            "phoneme": labels[0],
            "confidence": round(top_values[0].item(), 4),
            "top_k": [
                {"phoneme": label, "probability": round(prob.item(), 4)}
                for label, prob in zip(labels, top_values)
            ],
        }

    @modal.method()
    def transcribe_phonemes(
        self, audio_samples: list[float], sample_rate: int = 16000, top_k: int = 3
    ) -> list[dict]:
        import torch

        inputs = self.processor(
            audio_samples, sampling_rate=sample_rate, return_tensors="pt", padding=True
        )
        input_values = inputs.input_values.to("cuda")

        with torch.no_grad():
            logits = self.model(input_values).logits

        return self._decode_with_confidence(logits, top_k=top_k)

    @modal.web_endpoint(method="POST")
    def predict(self, request: dict) -> dict:
        """HTTP endpoint accepting {"audio": [float, ...], "sample_rate": int, "top_k": int}."""
        audio = request["audio"]
        sample_rate = request.get("sample_rate", 16000)
        top_k = request.get("top_k", 3)
        results = self.transcribe_phonemes.local(audio, sample_rate, top_k)
        return {"phonemes": results}
