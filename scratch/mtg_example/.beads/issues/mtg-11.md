---
title: Zone transfer operations optimization
status: closed
priority: 4
issue_type: chore
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-27T16:45:00Z"
closed_at: "2025-10-27T16:45:00Z"
---

# Description

Potential temporary allocations during card movement between zones (hand→battlefield→graveyard).

# Resolution

**Investigated and documented** - zone transfers are already well-optimized (2025-10-27).

## Analysis performed:
1. **Zone transfer mechanism** (`src/game/state.rs:188-235`):
   - `move_card()` performs in-place zone operations
   - No temporary allocations for card movement
   - Direct remove from source + add to destination
   - Properly logs to undo log

2. **Zone storage** (`src/zones.rs`):
   - Uses `Vec<CardId>` for card storage (CardId is Copy, so no allocations)
   - `remove()` uses Vec::remove(pos) which is O(n) but unavoidable
   - Attempted optimization to `swap_remove()` for O(1) removal **broke determinism tests**

## Why swap_remove() doesn't work:
- Even "semantically unordered" zones (Hand, Battlefield) require consistent iteration order
- Controllers iterate over cards in a fixed order for deterministic decision-making
- Changing iteration order breaks determinism tests (test_deck_determinism__white_weenie failed)
- Example: Plains cards appeared in different order after swap_remove, causing non-deterministic gameplay

## Current implementation is optimal:
- **No allocations** during zone transfers (CardId is Copy)
- **Maintains determinism** by preserving insertion order
- **Vec::remove()** is appropriate - O(n) cost is acceptable for zone sizes (typically < 20 cards)

## Added documentation:
Added comment in `zones.rs:46-49` explaining why we use `remove()` instead of `swap_remove()` for determinism.

## Performance impact:
Zone operations are not a bottleneck. Current performance (2025-10-26):
- 3,842 games/sec in fresh mode
- 16.56 actions/turn average
- Vec::remove() cost is negligible compared to other game operations

**No optimization needed** - implementation is already optimal for our determinism requirements.
