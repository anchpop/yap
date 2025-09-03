# Simulation

This document describes the simulation used when pre-caching audio for upcoming challenges.

The simulation runs for `n` days. Each day it:

1. Reviews all due cards, answering each challenge perfectly.
2. Calls a provided callback for every challenge shown. The callback can inspect the challenge to collect audio requests.
3. Adds 10 new cards to the deck.
4. Advances the internal clock by one day.

This process allows the system to predict which audio files will be needed in the near future so they can be cached ahead of time.
