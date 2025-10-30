---
title: Eliminate GameStateView clones
status: closed
priority: 3
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T22:18:42Z"
---

# Description

Created on every controller decision.
Consider borrowing instead of cloning where possible.

## Resolution (2025-10-26)

This issue was already resolved in earlier development. Investigation shows:

1. `GameStateView` does NOT derive Clone
2. `GameStateView` is a lightweight struct with just two fields:
   - `game: &'a GameState` (a reference, not owned)
   - `player_id: PlayerId` (Copy type)
3. Creating a new GameStateView is O(1) and doesn't clone the game state
4. All controller interfaces already take `&GameStateView` (reference)
5. No cloning of GameStateView found in codebase

The architecture already uses borrowing correctly. GameStateView is created as needed but never cloned, achieving the zero-copy design goal.
