import json
import sys
from typing import List, Dict, Optional, Set, Tuple
import spacy
from spacy.matcher import DependencyMatcher, PhraseMatcher
from pathlib import Path
from tqdm import tqdm
from typing import NamedTuple

class Term(NamedTuple):
    term: str
    start_char: int
    end_char: int

class DetectedTerms(NamedTuple):
    high_confidence: List[Term]
    low_confidence: List[Term]

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

class MultiwordTermDetector:
    def __init__(self, terms_file: str, language_code: str):
        # Load spaCy model for the specified language
        models = MODEL_MAPPING.get(language_code)
        if not models:
            raise ValueError(f"Unsupported language code: {language_code}")
        
        model_name = models["large"] if use_big_model else models["small"]
        self.nlp = spacy.load(model_name)
        self.language_code = language_code
        print(f"Pipeline components: {self.nlp.pipe_names}")
        
        # Initialize both matchers
        self.dep_matcher = DependencyMatcher(self.nlp.vocab)
        self.phrase_matcher = PhraseMatcher(self.nlp.vocab, attr="LEMMA")
        
        # Load multiword terms
        print(f"Loading multiword terms from {terms_file}...")
        self.terms_file = terms_file
        self.multiword_terms = self._load_terms(terms_file)
        print(f"Loaded {len(self.multiword_terms)} multiword terms")
        
        # Pre-compute lemmatized form mappings and create patterns
        print("Creating patterns and lemma mappings...")
        self.lemma_to_terms = {}  # Maps tuple of lemmas to list of original terms
        self._create_patterns_and_mappings()
        print(f"Created patterns for {len(self.dep_matcher)} dependency patterns and phrase patterns")
    
    def _load_terms(self, terms_file: str) -> List[str]:
        """Load multiword terms from file"""
        with open(terms_file, 'r', encoding='utf-8') as f:
            return [line.strip() for line in f if line.strip()]
    
    def _parse_terms_cached(self):
        """Parse all terms, with a DocBin cache next to the terms file: parsing ~200k
        terms through a transformer model on CPU takes hours, and the parses depend
        only on (model, terms list) — so cache them keyed by both."""
        import hashlib
        import json
        from spacy.tokens import DocBin

        meta = self.nlp.meta
        key = hashlib.sha256(
            (meta["lang"] + meta["name"] + meta["version"] + "\x00"
             + "\x00".join(self.multiword_terms)).encode()
        ).hexdigest()
        cache_base = self.terms_file + ".parsed"
        meta_path = cache_base + ".json"
        bin_path = cache_base + ".docbin"
        try:
            if json.load(open(meta_path))["key"] == key:
                print(f"Loading cached term parses from {bin_path}...")
                doc_bin = DocBin().from_disk(bin_path)
                return list(doc_bin.get_docs(self.nlp.vocab))
        except (FileNotFoundError, json.JSONDecodeError, KeyError):
            pass

        docs = []
        batch_size = 1000
        for i in tqdm(range(0, len(self.multiword_terms), batch_size), desc="Parsing terms"):
            docs.extend(self.nlp.pipe(self.multiword_terms[i:i+batch_size], batch_size=100))
        doc_bin = DocBin(store_user_data=False, docs=docs)
        doc_bin.to_disk(bin_path)
        json.dump({"key": key}, open(meta_path, "w"))
        print(f"Cached {len(docs)} term parses to {bin_path}")
        return docs

    def _create_patterns_and_mappings(self):
        """Create both dependency and phrase patterns for each multiword term, and lemma mappings"""
        phrase_patterns = []
        all_docs = self._parse_terms_cached()

        if True:  # keep original loop indentation
            for term, doc in zip(self.multiword_terms, all_docs):
                # Create lemma mapping
                lemma_tuple = tuple(token.lemma_ for token in doc)
                if lemma_tuple not in self.lemma_to_terms:
                    self.lemma_to_terms[lemma_tuple] = []
                self.lemma_to_terms[lemma_tuple].append(term)
                
                # Add to PhraseMatcher for high confidence sequential matching
                phrase_patterns.append(doc)
                
                # Skip single-token terms for DependencyMatcher
                if len(doc) == 1:
                    # For single tokens, we'll need a different approach
                    # For now, let's create a simple pattern
                    pattern = [{"RIGHT_ID": "single", "RIGHT_ATTRS": {"LOWER": doc[0].text.lower()}}]
                    self.dep_matcher.add(term, [pattern])
                    continue
                
                patterns = []
                pat = self._create_dependency_pattern_for_doc(doc)
                if pat:
                    patterns.append(pat)
                
                if patterns:
                    self.dep_matcher.add(term, patterns)
        
        # Add all phrase patterns at once for efficiency
        self.phrase_matcher.add("MULTIWORD_TERMS", phrase_patterns)

    def _create_dependency_pattern_for_doc(self, doc) -> Optional[List[Dict]]:

        # Language-specific pattern handling
        if self.language_code == "fra":
            # -------- French: "ne X" two-word negations ----------
            NEG_PARTS = {
                ("pas",), ("plus",), ("que",), ("jamais",), ("guère",), ("point",),
                ("rien",), ("personne",), ("aucun",), ("nulle", "part")
            }
            if (
                doc[0].lemma_ == "ne"
                and tuple([part.lemma_ for part in doc[1:]]) in NEG_PARTS
            ):
                second = doc[1].lemma_
                pat = [
                    {"RIGHT_ID": "ne", "RIGHT_ATTRS": {"LEMMA": "ne"}},
                    {
                        "LEFT_ID": "ne",
                        "REL_OP": ".*",              # ne … anything … second
                        "RIGHT_ID": "neg2",
                        "RIGHT_ATTRS": {"LEMMA": second},
                    },
                ]
                return pat
        elif self.language_code == "spa":
            # Spanish-specific patterns can be added here if needed
            # For now, Spanish doesn't have the same split negation pattern as French
            pass
        elif self.language_code == "deu":
            # German-specific patterns can be added here if needed
            # German has separable verbs and other compound structures that might need special handling
            # For now, we'll use the default pattern matching
            pass

        # -------- early exits & c'est branch ----------
        num_roots = sum(1 for tok in doc if tok.dep_ == "ROOT")
        if num_roots > 1:
            return None

        if (len(doc) == 3 and doc[1].orth_ == "-") or (
            len(doc) == 2 and doc[1].orth_.startswith("-")
        ):
            return None

        if self.language_code == "fra" and str(doc) == "c'est":
            return [
                {"RIGHT_ID": "ce", "RIGHT_ATTRS": {"LEMMA": "ce"}},
                {
                    "LEFT_ID": "ce",
                    "REL_OP": "$++",
                    "RIGHT_ID": "etre",
                    "RIGHT_ATTRS": {"LEMMA": "être", "DEP": "cop"},
                },
            ]

        root = doc[:].sent.root
        pat = self._create_pattern_for_token(root, keep=("LEMMA",))
        return pat


    
    def _create_pattern_for_token(self, root,
                                keep=("DEP", "POS", "LEMMA")) -> Optional[list[dict]]:
        """
        Build a DependencyMatcher pattern that matches the clause headed by `root`.
        """

        # ------------------------------------------------------------------
        # 1) Promote *up* to a content word if the current token is weak, OR
        #    demote *down* if it's a weak artificial root produced by parsing a
        #    two-word fragment like "être à".
        # ------------------------------------------------------------------
        weak_dep = {"cop", "aux", "case", "mark", "det", "expl"}
        weak_pos = {"ADP", "DET", "PART", "PRON"}

        # upward promotion
        while (root.dep_ in weak_dep or root.pos_ in weak_pos) and root.head != root:
            root = root.head

        # downward promotion if we’re still on a weak root of a fragment
        if root.pos_ in weak_pos and all(c.dep_ in weak_dep for c in root.children):
            # pick the first non-weak child as anchor (usually the verb or noun)
            for c in root.children:
                if c.pos_ not in weak_pos:
                    root = c
                    break

        # ------------------------------------------------------------------
        # 2) Build the pattern
        # ------------------------------------------------------------------
        pattern: List[Dict] = []
        id_map: Dict[int, str] = {}

        def make_id(tok):          # stable but short IDs
            return f"t{tok.i}"

        # anchor node
        id_map[root.i] = make_id(root)
        pattern.append({
            "RIGHT_ID": id_map[root.i],
            "RIGHT_ATTRS": {k: getattr(root, k.lower() + "_")
                            for k in keep if k != "DEP"}
        })

        # ------------------------------------------------------------------
        # 3) BFS down the clause, always including cop/aux/case children even
        #    when they fall outside root.subtree (they often do).
        # ------------------------------------------------------------------
        always_take = {"cop", "aux", "case"}
        queue = [root]

        while queue:
            parent = queue.pop(0)
            for child in parent.children:

                if child.dep_ not in always_take and child not in root.subtree:
                    continue

                id_map[child.i] = make_id(child)

                # choose operator: '>' for direct child, '>>' for deeper
                rel_op = ">" if child.head is parent else ">>"

                attrs = {k: getattr(child, k.lower() + "_")
                        for k in keep if k != "DEP"}
                if "DEP" in keep:
                    attrs["DEP"] = child.dep_

                pattern.append({
                    "LEFT_ID": id_map[parent.i],
                    "REL_OP": rel_op,
                    "RIGHT_ID": id_map[child.i],
                    "RIGHT_ATTRS": attrs
                })
                queue.append(child)

        return pattern

    
    def find_multiword_terms_batch(self, sentences: List[str]) -> List[Tuple[any, DetectedTerms]]:
        """
        Find multiword terms in a batch of sentences using both matchers.
        Returns list of results with high and low confidence matches.
        """
        # Process sentences in batch
        docs = list(self.nlp.pipe(sentences, batch_size=100))
        
        results = []
        for doc in docs:
            #print("doc:", doc, "full doc:", [{"lemma": token.lemma_, "text": token.text, "dep": token.dep_, "head": token.head.i} for token in doc])
            
            # High confidence: PhraseMatcher (sequential lemma matching)
            phrase_matches = self.phrase_matcher(doc)
            high_confidence_terms = set()
            
            for match_id, start, end in phrase_matches:
                # Get the matched span
                span = doc[start:end]
                
                # Create lemma tuple for the span
                span_lemma_tuple = tuple(token.lemma_ for token in span)
                
                # Look up original terms using pre-computed mapping
                if span_lemma_tuple in self.lemma_to_terms:
                    # Get character positions
                    start_char = span[0].idx
                    end_char = span[-1].idx + len(span[-1].text)
                    
                    # Add all matching original terms (handles multiple terms with same lemmatized form)
                    for original_term in self.lemma_to_terms[span_lemma_tuple]:
                        high_confidence_terms.add(Term(original_term, start_char, end_char))
            
            # Low confidence: DependencyMatcher
            dep_matches = self.dep_matcher(doc)
            low_confidence_terms = []
            
            for match_id, token_ids in dep_matches:
                # Get the matched term name
                term = self.nlp.vocab.strings[match_id]
                
                # Get the span of tokens
                tokens = sorted(token_ids)  # Ensure tokens are in order
                start_token = doc[tokens[0]]
                end_token = doc[tokens[-1]]
                
                # Get character positions
                start_char = start_token.idx
                end_char = end_token.idx + len(end_token.text)
                
                term_obj = Term(term, start_char, end_char)
                
                # Only add to low confidence if not already in high confidence
                if term_obj not in high_confidence_terms:
                    low_confidence_terms.append(term_obj)
            
            results.append((doc, DetectedTerms(list(high_confidence_terms), low_confidence_terms)))
        
        return results
    
    def find_multiword_terms(self, sentence: str) -> Tuple[any, DetectedTerms]:
        """
        Find multiword terms in a single sentence using both matchers.
        Returns DetectedTerms with high and low confidence matches.
        """
        # Use the batch method with a single sentence
        return self.find_multiword_terms_batch([sentence])[0]
    
    def debug_parse(self, text: str):
        """Debug helper to show how spaCy parses text"""
        doc = self.nlp(text)
        print(f"\nParse of '{text}':")
        for token in doc:
            print(f"  {token.i}: '{token.text}' "
                  f"(lemma: '{token.lemma_}', pos: {token.pos_}, "
                  f"dep: {token.dep_}, head: {token.head.i})")
        return doc

def process_sentences(sentences_file: str, terms_file: str, output_file: str, language_code: str):
    """Process sentences from JSONL file and add multiword terms"""
    print(f"\nInitializing multiword term detector for language: {language_code}...")
    detector = MultiwordTermDetector(terms_file, language_code)
    
    # Count total lines first for progress bar
    print("\nCounting sentences...")
    with open(sentences_file, 'r', encoding='utf-8') as f:
        total_lines = sum(1 for line in f if line.strip())
    print(f"Found {total_lines} sentences to process")
    
    print("\nProcessing sentences...")
    batch_size = 1000  # Process 1000 entries at a time
    
    with open(sentences_file, 'r', encoding='utf-8') as infile, \
         open(output_file, 'w', encoding='utf-8') as outfile:
        
        # Process in batches
        batch_sentences = []
        
        for line in tqdm(infile, total=total_lines, desc="Processing", unit="sentences"):
            if not line.strip():
                continue
            
            sentence = json.loads(line)
            batch_sentences.append(sentence)
            
            # Process batch when it's full
            if len(batch_sentences) >= batch_size:
                # Find multiword terms for all sentences in batch
                all_terms = detector.find_multiword_terms_batch(batch_sentences)
                
                # Process each sentence and its results
                for sentence, (doc, detected_terms) in zip(batch_sentences, all_terms):
                    # Store term names by confidence level
                    high_confidence_names = list(set([term.term for term in detected_terms.high_confidence]))
                    low_confidence_names = list(set([term.term for term in detected_terms.low_confidence]))
                    
                    # Create a dict with the sentence and its multiword terms
                    sentence_with_terms = {
                        "sentence": sentence,
                        "multiword_terms": {
                            "high_confidence": high_confidence_names,
                            "low_confidence": low_confidence_names
                        },
                        "doc": [{"text": token.text, 
                                "whitespace": token.whitespace_,
                                "lemma": token.lemma_, 
                                "pos": token.pos_,
                                "morph": token.morph.to_dict(),
                                "dep": token.dep_} for token in doc],
                        "entities": [(ent.text, ent.label_) for ent in doc.ents],
                    }

                    # Write the enhanced data
                    outfile.write(json.dumps(sentence_with_terms, ensure_ascii=False) + '\n')
                
                # Clear batch
                batch_sentences = []
        
        # Process remaining sentences
        if batch_sentences:
            # Find multiword terms for all sentences in batch
            all_terms = detector.find_multiword_terms_batch(batch_sentences)
            
            # Process each sentence and its results
            for sentence, (doc, detected_terms) in zip(batch_sentences, all_terms):
                # Store term names by confidence level
                high_confidence_names = list(set([term.term for term in detected_terms.high_confidence]))
                low_confidence_names = list(set([term.term for term in detected_terms.low_confidence]))
                
                # Create a dict with the sentence and its multiword terms
                sentence_with_terms = {
                    "sentence": sentence,
                    "multiword_terms": {
                        "high_confidence": high_confidence_names,
                        "low_confidence": low_confidence_names
                    },
                    "doc": [{"text": token.text, 
                            "whitespace": token.whitespace_,
                            "lemma": token.lemma_, 
                            "pos": token.pos_,
                            "morph": token.morph.to_dict(),
                            "dep": token.dep_,
                            "head": token.head.i} for token in doc],
                    "entities": [(ent.text, ent.label_) for ent in doc.ents],
                }

                # Write the enhanced data
                outfile.write(json.dumps(sentence_with_terms, ensure_ascii=False) + '\n')
    
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
