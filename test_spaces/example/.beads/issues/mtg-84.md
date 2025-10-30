---
title: HeuristicController attack logic missing board state evaluation
status: closed
priority: 2
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:51Z"
closed_at: "2025-10-26T01:11:38Z"
---

# Description

## Problem

The HeuristicController's `should_attack()` method only evaluates the attacker's stats in isolation, ignoring the opponent's board state entirely. This causes creatures to be too conservative - e.g., 2/2 vanilla creatures never attack even when the opponent has no blockers.

## Root Cause

Our Rust implementation is not faithful to Java Forge's attack logic.

**Current Rust logic** (src/game/heuristic_controller.rs:442-454):
```rust
// Aggression level 3 (balanced)
has_evasion || (power >= 2 && keywords) || power >= 3
```

**Java Forge logic** (AiAttackController.java:1535-1543):
```java
case 3: // expecting to at least kill a creature of equal value or not be blocked
    if ((saf.canKillAll && saf.isWorthLessThanAllKillers)
            || (((saf.dangerousBlockersPresent && saf.canKillAllDangerous)
                || saf.hasAttackEffect || saf.hasCombatEffect) && !saf.canBeKilledByOne)
            || !saf.canBeBlocked()) {
        return true;
    }
```

## What's Missing

The Java AI evaluates:
1. **Board state** - Available blockers and their stats
2. **Combat math** - Can kill all blockers? Will survive?
3. **Value comparison** - Creature evaluation scores
4. **Blockability** - Can opponent block this creature at all?

We're missing ~90% of Java's attack decision logic.

## Evidence

Test game log (AI_VALIDATION_LOG.md):
- Grizzly Bears (2/2, no keywords) never attacked
- Opponent had NO blockers for many turns
- Serra Angels attacked correctly (flying = evasion)

## Fix Requirements

Implement equivalent of Java's `SpellAbilityFactors` class:
1. Evaluate available blockers from GameStateView
2. Calculate combat math (can_kill_all, can_be_killed, etc.)
3. Compare creature values (attacker vs blockers)
4. Check blockability (CombatUtil.canBeBlocked equivalent)
5. Use these factors in aggression-based attack decisions

## References

- Java: `forge-java/forge-ai/src/main/java/forge/ai/AiAttackController.java:1350-1562`
- Rust: `src/game/heuristic_controller.rs:388-468`
- Test log: `AI_VALIDATION_LOG.md`

