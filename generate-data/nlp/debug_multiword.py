#!/usr/bin/env python3
"""Debug script to understand multiword term detection issues"""

import json
import spacy
from main import MultiwordTermDetector
import tempfile


def debug_term_detection():
    # Create test files with various multiword terms
    test_terms = ["c'est ça", "elles-mêmes", "être à l'ouest", "avoir l'air"]
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
        for term in test_terms:
            f.write(term + '\n')
        terms_file = f.name
    
    # Initialize spaCy
    nlp = spacy.load("fr_core_news_sm")
    
    # Show tokenization of test terms
    print("=== Tokenization of test terms ===")
    for term in test_terms:
        doc = nlp(term)
        print(f"\n'{term}':")
        for token in doc:
            print(f"  Token: '{token.text}', Lemma: '{token.lemma_}', POS: '{token.pos_}'")
    
    # Initialize detector
    print("\n=== Initializing detector ===")
    detector = MultiwordTermDetector(terms_file)
    
    # Test sentences
    test_sentences = [
        "Il est tout à vous.",
        "Qui vous a fait ça?",
        "C'est ça que je voulais dire.",
        "Elles-mêmes ont décidé.",
        "Je suis complètement à l'ouest.",
        "Elle a l'air fatiguée."
    ]
    
    print("\n=== Testing sentences ===")
    for sentence in test_sentences:
        print(f"\nSentence: '{sentence}'")
        
        # Tokenize the sentence
        doc = nlp(sentence)
        print("Tokens:")
        for i, token in enumerate(doc):
            print(f"  {i}: '{token.text}', Lemma: '{token.lemma_}', POS: '{token.pos_}'")
        
        # Find matches
        terms = detector.find_multiword_terms(sentence)
        if terms:
            print(f"Found terms: {terms}")
        else:
            print("No terms found")
    
    # Clean up
    import os
    os.unlink(terms_file)


if __name__ == "__main__":
    debug_term_detection()
