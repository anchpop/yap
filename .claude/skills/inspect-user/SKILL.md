---
name: inspect-user
description: Inspect a user's deck state by fetching their events from Supabase and replaying them. Run with an email address (e.g. /inspect-user user@example.com). Use when the user wants to see a specific user's card state, goal progress, or next-cards-to-add.
---

# Inspect User Deck State

Fetch a user's events from Supabase, replay them to reconstruct their deck, and print a report showing their current state, goal progress, and what the next-cards-to-add function would recommend.

## How to run

```bash
INSPECT_EMAIL=<email> cargo test -p yap-frontend-rs inspect_user_deck -- --nocapture
```

Requires `SUPABASE_SERVICE_ROLE_KEY` env var (should already be in `.env`).

The language pair is auto-detected from the user's deck_selection events.

## What it reports

- Deck selection: detected language pair, onboarding selections
- Total cards, reviews, XP, placement test status, leeches, start time, current goal
- Goal comprehension percentage
- Smart add card options: how many cards to add, projected comprehension after adding, and a preview of the next cards

## If results look wrong

- Check the deck_selection output to verify the language was detected correctly
- 0 cards despite many reviews may mean the language pair doesn't match
- The test prints stream/device counts which can help diagnose missing data
