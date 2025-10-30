---
title: Heuristic AI completeness tracking
status: open
priority: 1
issue_type: epic
labels:
  - tracking
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:51Z"
---

# Description

Track completion of heuristic AI port from Java Forge to Rust.

## Current Status

**What's Implemented in HeuristicController:**
- ✅ Creature evaluation (comprehensive, faithful port)
- ✅ Attack decisions with aggression levels (basic - needs improvement)
- ✅ Block decisions with value trading
- ✅ Basic targeting (best creature)
- ✅ Basic spell selection (creatures first)
- ✅ GameStateEvaluator (basic holistic board evaluation)
- ✅ Opponent life access (bd-4 completed)

**What's Missing:**

### High Priority (Core AI Strength):

1. **Attack logic improvements (mtg-85)**
   - Current: Only evaluates attacker stats in isolation
   - Missing: Board state evaluation, combat math, blockability checks
   - Reference: Java's SpellAbilityFactors class in AiAttackController.java:1350-1562
   - Impact: 2/2 vanilla creatures never attack even with no blockers
   - Impact: Shivan Dragon (5/5 flyer) doesn't attack Grizzly Bears (2/2 ground)

2. **GameStateEvaluator improvements:**
   - mtg-78: Port evalManaBase() - mana base quality scoring
   - mtg-79: Track summon sickness properly (COMPLETED 2025-10-26)
   - mtg-81: Complete land evaluation (detailed heuristics)

3. **Combat outcome prediction**
   - Simulate combat before making decisions
   - Critical for knowing if attacks will be lethal
   - Reference: GameStateEvaluator.java:40-67, 91-100

4. **Activated ability evaluation and timing**
   - Current: Activates any available ability without evaluation
   - Needed: Value assessment, timing optimization, mana efficiency
   - Example: Prodigal Sorcerer should ping opponents when valuable
   - Example: Pump abilities (Shivan Dragon) before combat damage

5. **Mana ability recognition from creatures**
   - Need to recognize Llanowar Elves and similar mana dorks
   - Should use them to cast bigger threats earlier
   - Mana engine needs to see creature mana abilities

### Medium Priority:

6. **Spell evaluation** - Beyond creatures
   - Removal spell targeting (ComputerUtilCard)
   - Card draw value assessment
   - Pump spells, combat tricks

7. **Mana tapping order** - ComputerUtilMana
   - Leave up correct colors for instant responses
   - Optimize painland/fetchland usage

### Lower Priority:

8. **Damage assignment order** - Kill blockers efficiently
9. **Bluffing/deception** - Hold information when advantageous
10. mtg-80: Improve enchantment evaluation
11. **Static abilities** - "Must attack", "Can't be blocked by walls", etc.

## Completed Work

- ✅ Basic GameStateEvaluator with hand, life, and battlefield evaluation
- ✅ Creature evaluation (faithful port from Java)
- ✅ Basic land evaluation
- ✅ Score type with summon sickness tracking (mtg-79 completed 2025-10-26)
- ✅ Opponent life access (bd-4) - GameStateView now provides player_life(), opponents(), and opponent_life() methods
- ✅ Activated ability targeting (mtg-70) - Royal Assassin can now target and destroy tapped creatures
- ✅ **Comprehensive test coverage with 4ED cards (2025-10-26) - 312 tests passing**

## Test Coverage Expansion (2025-10-26)

Added 4 new e2e tests exercising different AI scenarios:
- Prodigal Sorcerer activated ability usage (pinging)
- Llanowar Elves mana dork recognition and ramping
- Shivan Dragon pump ability and flying attacks
- Juggernaut "must attack" static ability

These tests reveal areas for improvement:
- Activated ability timing and evaluation needs work
- Mana ability recognition from creatures needs implementation
- Pump ability evaluation and usage needs improvement
- Static abilities like "must attack" not yet implemented

