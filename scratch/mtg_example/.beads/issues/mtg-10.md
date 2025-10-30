---
title: Vec reallocations in game loop
status: closed
priority: 4
issue_type: chore
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-23T10:11:38Z"
---

# Description

Many small Vec allocations for temporary collections:
- game_loop.rs:418 (player_ids collection)
- combat.rs:90,95 (attackers/blockers lists)

Return iterators instead of Vec where possible.

# Notes

Completed in commit 0d6cdb50. Replaced player_ids Vec with fixed array and cards_to_untap Vec with SmallVec. Combat allocations already addressed in 60d257e1.
