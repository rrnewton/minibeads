---
title: Add opponent life access to GameStateView
status: closed
priority: 3
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T19:09:13Z"
---

# Description

Add method to GameStateView to get opponent life totals.

Currently GameStateEvaluator uses a hardcoded placeholder (20 life) for opponent life.

Need to add:
- GameStateView::opponent_life(opponent_id: PlayerId) -> i32
- Or: GameStateView::opponents() -> Iterator<PlayerId>
- Support for multiplayer (multiple opponents)

This allows proper life total evaluation in game states.
