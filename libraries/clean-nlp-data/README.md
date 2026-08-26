# clean-nlp-data

Cleans data for custom NLP training. The NLP model used by Yap (lexide) is trained from data in this repo. Generating that data requires python.

## Setup

Install the spaCy NLP models (run from the repo root):

```bash
cd ./generate-data/nlp && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/fr_dep_news_trf-3.8.0/fr_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/es_dep_news_trf-3.8.0/es_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ko_core_news_lg-3.8.0/ko_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/en_core_web_trf-3.8.0/en_core_web_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/de_dep_news_trf-3.8.0/de_dep_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/zh_core_web_trf-3.8.0/zh_core_web_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/it_core_news_lg-3.8.0/it_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/pt_core_news_lg-3.8.0/pt_core_news_lg-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ja_core_news_trf-3.8.0/ja_core_news_trf-3.8.0-py3-none-any.whl && \
  uv pip install https://github.com/explosion/spacy-models/releases/download/ru_core_news_lg-3.8.0/ru_core_news_lg-3.8.0-py3-none-any.whl
```

## Generate the data

```
cargo run --bin clean-nlp-data clean
```
