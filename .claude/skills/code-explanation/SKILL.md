---
name: code-explanation
description: Explain code in a self-contained way. Use whenever asked to explain, walk through, or document how code works.
---

# code-explanation

Explain the code in a self-contained way.

Try to avoid using terms that have been defined earlier in the conversation,
or if you do use such terms, explain their meaning in parentheses after the
first time you use it. (General rust concepts are fine, but an example of
something to avoid might be simply referencing "the invariant" or even "the
loopback invariant" — instead say "the loopback invariant (no outputs from a
later step may be used as an input to an earlier step)".)

Use code blocks to quote the code relevant to what you're talking about.

All relevant prompts should be quoted in full as they become relevant.
