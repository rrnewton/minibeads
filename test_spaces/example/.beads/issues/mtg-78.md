---
title: Port evalManaBase() to GameStateEvaluator
status: closed
priority: 3
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T23:45:00Z"
---

# Description

Port the evalManaBase() function from Java to Rust GameStateEvaluator.

Reference: GameStateEvaluator.java:176-216

The function evaluates mana base quality by:
- Counting available mana sources by color
- Comparing against deck's color requirements (maxPips)
- Comparing total mana against deck's max CMC
- Value: +100 per color pip up to deck needs
- Value: +100 per total mana source up to max CMC
- Value: +5 per excess mana beyond deck needs

Requires:
- Access to player's lands and mana abilities
- Deck statistics (max pips by color, max CMC)
- Mana ability parsing to determine colors produced

## Resolution (2025-10-26)

Implemented a **simplified version** of evalManaBase() that doesn't require deck statistics (AiDeckStatistics).

The full Java implementation requires:
- `AiDeckStatistics` structure with `maxPips[]` and `maxCost` fields
- Deck analysis infrastructure to compute these statistics

Since we don't have this infrastructure yet, implemented `evaluate_mana_base_simplified()` that:
- Counts total mana sources from activated abilities with `is_mana_ability` flag
- Tracks which colors (WUBRG) are available
- Awards value based on heuristic brackets:
  - First 6 mana sources: +100 each (typical early/mid game)
  - Next 4 mana sources: +50 each (late game)
  - Beyond 10: +5 each (excess)
  - +50 per color available (encourages color fixing)

This provides immediate value for game state evaluation without requiring the full deck statistics infrastructure.

**Future work**: Create new issue for porting AiDeckStatistics, then revisit this for full faithful port.
