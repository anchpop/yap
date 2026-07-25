"""spaCy sentence analysis for the gold-labeling pipeline (clean-nlp-data).

Parses sentences into tokens (text/whitespace/lemma/pos/morph) as NlpAnalyzedSentence
JSONL. Historical note: this used to also run a MultiwordTermDetector (dependency +
phrase matchers built from a terms file — 215k transformer parses for eng, hours of
CPU); nothing ever consumed those detections downstream, so the subsystem was deleted
and `multiword_terms` is always emitted empty. The terms-file CLI argument is kept for
interface compatibility and ignored.
"""
import json
import sys

import spacy
from tqdm import tqdm

# Model mapping for different languages
MODEL_MAPPING = {
    "fra": {
        "small": "fr_core_news_sm",
        "large": "fr_dep_news_trf"
    },
    "spa": {
        "small": "es_core_news_sm",
        "large": "es_dep_news_trf"
    },
    "kor": {
        "small": "ko_core_news_sm",
        "large": "ko_core_news_lg"
    },
    "eng": {
        "small": "en_core_web_sm",
        "large": "en_core_web_trf"
    },
    "deu": {
        "small": "de_core_news_sm",
        "large": "de_dep_news_trf"
    },
    "zho": {
        "small": "zh_core_web_trf",
        "large": "zh_core_web_trf"
    },
    "ita": {
        "small": "it_core_news_lg",
        "large": "it_core_news_lg"
    },
    "por": {
        "small": "pt_core_news_lg",
        "large": "pt_core_news_lg"
    },
    "jpn": {
        "small": "ja_core_news_trf",
        "large": "ja_core_news_trf"
    },
    "rus": {
        "small": "ru_core_news_lg",
        "large": "ru_core_news_lg"
    },
}

use_big_model = True


def load_model(language_code: str):
    models = MODEL_MAPPING.get(language_code)
    if not models:
        raise ValueError(f"Unsupported language code: {language_code}")
    model_name = models["large"] if use_big_model else models["small"]
    nlp = spacy.load(model_name)
    print(f"Pipeline components: {nlp.pipe_names}")
    return nlp


def process_sentences(sentences_file: str, terms_file: str, output_file: str, language_code: str):
    """Parse sentences from JSONL and write analyzed records (terms_file is ignored)."""
    del terms_file  # kept in the CLI for compatibility; detections were never consumed
    nlp = load_model(language_code)

    with open(sentences_file, "r", encoding="utf-8") as f:
        sentences = [json.loads(line) for line in f if line.strip()]
    print(f"Found {len(sentences)} sentences to process")

    batch_size = 1000
    with open(output_file, "w", encoding="utf-8") as outfile:
        for i in tqdm(range(0, len(sentences), batch_size), desc="Processing", unit="batch"):
            batch = sentences[i : i + batch_size]
            for sentence, doc in zip(batch, nlp.pipe(batch, batch_size=100)):
                record = {
                    "sentence": sentence,
                    # Always empty — see module docstring.
                    "multiword_terms": {"high_confidence": [], "low_confidence": []},
                    "doc": [
                        {
                            "text": token.text,
                            "whitespace": token.whitespace_,
                            "lemma": token.lemma_,
                            "pos": token.pos_,
                            "morph": token.morph.to_dict(),
                            "dep": token.dep_,
                            "head": token.head.i,
                        }
                        for token in doc
                    ],
                    "entities": [(ent.text, ent.label_) for ent in doc.ents],
                }
                outfile.write(json.dumps(record, ensure_ascii=False) + "\n")

    print(f"\nProcessing complete! Output written to {output_file}")


def main():
    if len(sys.argv) != 5:
        print("Usage: python main.py <language_code> <sentences.jsonl> <multiword_terms.txt> <output.jsonl>")
        print("Language code should be ISO 639-3 (e.g., 'fra' for French, 'spa' for Spanish)")
        sys.exit(1)

    language_code = sys.argv[1]
    sentences_file = sys.argv[2]
    terms_file = sys.argv[3]
    output_file = sys.argv[4]

    process_sentences(sentences_file, terms_file, output_file, language_code)


if __name__ == "__main__":
    main()
