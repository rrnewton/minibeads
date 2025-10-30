---
title: Complete land evaluation in GameStateEvaluator
status: closed
priority: 3
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T23:55:00Z"
---

# Description

Implement detailed land evaluation heuristics.

Reference: GameStateEvaluator.java:240-285

Currently uses a simple heuristic. Should implement:
- +100 per mana the land can produce
- +3 per color of mana produced
- +25 for manlands (activated abilities without tap cost)
- +10 for sac abilities (not repeatable)
- +50 for repeatable utility abilities (tap cost)
- +6 per static ability

Requires parsing mana abilities to determine:
- How much mana produced
- What colors produced
- Cost to activate

## Resolution (2025-10-26)

Implemented detailed land evaluation that closely matches Java's logic:

**Mana production evaluation:**
- Calculates net mana produced (mana generated - mana cost to activate)
- Tracks all colors produced (WUBRG + colorless)
- Awards +100 per mana produced (net)
- Awards +3 per color available

**Non-mana utility abilities:**
- Manlands (no tap cost): +25 value
- Sacrifice abilities (not repeatable): +10 value
- Repeatable utility (tap cost): +50 value

**Not implemented yet:**
- Static ability evaluation (+6 per static ability)
- Requires adding a `static_abilities` field to Card struct
- Can be added in future work

**Testing:**
Added comprehensive unit test (`test_land_evaluation`) covering:
- Basic lands (Forest: 3 + 100 + 3 = 106)
- Dual lands (Command Tower: 3 + 100 + 6 = 109)
- Utility lands (3 + 100 + 3 + 50 = 156)

All 314 tests passing.
