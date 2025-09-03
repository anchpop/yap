#!/usr/bin/env python3
"""Test script for multiword term detection using DependencyMatcher"""

from main import MultiwordTermDetector
import json
import tempfile
import os


def test_detection():
    # Create a temporary file with test multiword terms
    test_terms = [
        "être à",
        "être à l'ouest",
        "être à cran",
        "avoir l'air",
        "faire attention",
        "elles-mêmes",
        "c'est ça",
        "qu'est-ce que",
        "qu'est-ce qui",
    ]
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
        for term in test_terms:
            f.write(term + '\n')
        terms_file = f.name
    
    try:
        # Initialize detector
        print("Initializing detector with test terms...")
        detector = MultiwordTermDetector(terms_file)
        
        # Test sentences - including some that should NOT match
        test_cases = [
            ("Désolé, je suis complètement à l'ouest ce matin.", ["être à l'ouest"]),
            ("Il est vraiment à cran aujourd'hui.", ["être à cran"]),
            ("Elle a l'air fatiguée.", ["avoir l'air"]),
            ("Tu dois faire très attention à ça.", ["faire attention"]),
            ("Elles-mêmes ont décidé de partir.", ["elles-mêmes"]),
            ("C'est ça que je voulais.", ["c'est ça"]),
            # Test contractions with être à
            ("C'est pas à moi.", ["être à"]),  # Should match "être à" (with negation)
            ("C'est à vous de décider.", ["être à"]),  # Should match "être à"
            ("N'est-ce pas à lui?", ["être à"]),  # Should match "être à"
            ("Ce n'est pas à toi.", ["être à"]),  # Should match "être à" (with negation)
            # These should NOT match any terms
            ("Qui vous a fait ça?", []),  # Should not match "elles-mêmes" or "c'est ça"
            ("Il est tout à vous.", []),  # Should not match anything
        ]
        
        print("\nTesting multiword term detection:\n")
        
        all_passed = True
        for sentence, expected_terms in test_cases:
            print(f"Sentence: {sentence}")
            
            # Optionally show parse for debugging
            # detector.debug_parse(sentence)
            
            found = detector.find_multiword_terms(sentence)
            found_terms = [term for term, _, _ in found]
            
            if set(found_terms) == set(expected_terms):
                print(f"✓ Found terms: {found_terms}")
            else:
                print(f"✗ Found terms: {found_terms}, expected: {expected_terms}")
                all_passed = False
                
                # Show parse for failed cases
                if found_terms != expected_terms:
                    print("  Debug parse:")
                    doc = detector.nlp(sentence)
                    for token in doc:
                        print(f"    {token.i}: '{token.text}' (lemma: '{token.lemma_}', "
                              f"dep: {token.dep_}, head: {token.head.i})")
            print()
        
        if all_passed:
            print("All tests passed!")
        else:
            print("Some tests failed!")
        
        # Test the new data structure
        print("\n=== Testing new data structure ===")
        
        # Create a test JSONL file
        test_data = {
            "french": ["Qu'est-ce qu'il se passe?", "Qu'est-ce qui se passe encore?"],
            "english": "What's going on?"
        }
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            json.dump(test_data, f)
            test_jsonl = f.name
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            output_jsonl = f.name
        
        # Process the test file
        from main import process_sentences
        process_sentences(test_jsonl, terms_file, output_jsonl)
        
        # Read and verify the output
        with open(output_jsonl, 'r') as f:
            result = json.loads(f.read())
            
        print("\nOriginal data:")
        print(json.dumps(test_data, indent=2, ensure_ascii=False))
        print("\nProcessed data:")
        print(json.dumps(result, indent=2, ensure_ascii=False))
        
        # Clean up test files
        os.unlink(test_jsonl)
        os.unlink(output_jsonl)
    
    finally:
        # Clean up
        os.unlink(terms_file)


if __name__ == "__main__":
    test_detection()
