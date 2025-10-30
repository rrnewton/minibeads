---
title: 'TODO: minor: if one deck is passed to `mtg tui` use that for both players'
status: closed
priority: 2
issue_type: task
labels:
  - human
created_at: "2025-10-27T06:51:48-07:00"
updated_at: "2025-10-28T18:00:00Z"
---

# Description

This is just a convenience feature for me using the `mtg tui` on the command line.

## Implementation (2025-10-28)

✅ Completed - commit 12ae142

Changes:
- Made PLAYER2_DECK optional in TUI command arguments
- If omitted, PLAYER1_DECK is used for both players
- Updated help text to clarify usage
- Added informative message when using same deck

Usage:
```bash
# Single deck for both players
mtg tui decks/simple_bolt.dck --p1=random --p2=random

# Still works with two decks
mtg tui decks/simple_bolt.dck decks/vigilance_deck.dck
```
