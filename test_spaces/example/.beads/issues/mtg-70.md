---
title: Implement targeting for activated abilities
status: closed
priority: 2
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T10:02:39Z"
---

# Description

Implement targeting for activated abilities

## Summary
Implemented full targeting support for activated abilities like Royal Assassin's '{T}: Destroy target tapped creature' ability.

## Implementation
1. **Added get_valid_targets_for_ability() in src/game/actions.rs (lines 434-574)**
   - Parses ability description to detect targeting restrictions ('tapped', 'untapped', 'creature')
   - Filters battlefield cards based on restrictions
   - Checks shroud/hexproof protection
   - Returns SmallVec of valid target CardIds

2. **Updated ActivateAbility handler in src/game/game_loop.rs (lines 1425-1559)**
   - Calls get_valid_targets_for_ability() before paying costs
   - Asks controller to choose targets if valid targets exist
   - Replaces placeholder targets (CardId(0)) with chosen targets
   - Handles DestroyPermanent, TapPermanent, UntapPermanent, PumpCreature effects

3. **Added target validation to get_activatable_abilities() (lines 1902-1927)**
   - Checks if ability requires targets (description contains 'target')
   - Only returns ability if valid targets are available
   - Prevents activating targeting abilities with no legal targets

4. **Fixed attacker tapping in declare_attackers_step() (line 815)**
   - Changed from Combat::declare_attacker() to GameState::declare_attacker()
   - Ensures attacking creatures are properly tapped (MTG Rules 508.1f)

## Test Results
Royal Assassin test (test_royal_assassin_with_log_capture) now passes:
- ✅ Royal Assassin activates when Grizzly Bears attacks and becomes tapped
- ✅ Royal Assassin targets and destroys Grizzly Bears
- ✅ Grizzly Bears ends up in graveyard
- ✅ No damage dealt (creature destroyed before combat damage)

Result: P2 creatures before: 1, after: 0, Grizzly Bears in graveyard: 1

## MTG Rules Implemented
- MTG Rules 508.1f: Attacking creatures become tapped
- MTG Rules 508.4: Priority after attackers declared
- Target restriction parsing for activated abilities
- Shroud/Hexproof targeting protection
