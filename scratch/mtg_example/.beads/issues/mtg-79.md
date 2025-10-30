---
title: Track summon sickness in GameStateEvaluator
status: closed
priority: 3
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T19:11:27Z"
---

# Description

Properly track summon sickness when evaluating game states.

Reference: GameStateEvaluator.java:146-173

The evaluator should maintain two scores:
1. Full score: includes all creatures
2. Summon sick score: excludes value from summon sick creatures

This encourages the AI to hold creatures until Main Phase 2 if they provide no immediate value (can't attack yet).

Implementation:
- Check if creature is sick (entered battlefield this turn)
- Check if game phase is before MAIN2
- If both true, don't add creature value to summon_sick_score
- Use turn_entered_battlefield field on Card
