---
title: Implement targeting for activated abilities
status: closed
priority: 2
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-26T19:09:13Z"
---

# Description

Royal Assassin test reveals that activated abilities don't support targeting yet.

Current status:
- ✅ Priority during combat works (MTG Rules 508.4)
- ✅ Royal Assassin's ability is loaded from card database
- ✅ get_activatable_abilities() correctly finds the ability
- ✅ HeuristicController.choose_best_spell() now considers activated abilities
- ❌ Activated abilities use placeholder targets instead of asking controller to choose

What needs to be implemented:
1. In game_loop.rs ActivateAbility handler (line ~1425):
   - Get valid targets for the activated ability
   - Ask controller to choose targets (similar to spell targeting)
   - Replace placeholder targets in effects with chosen targets
   
2. Add get_valid_targets_for_ability() method to GameState
   - Similar to get_valid_targets_for_spell()
   - For Royal Assassin: filter for tapped creatures
   
3. Test with Royal Assassin scenario:
   - Royal Assassin should target Grizzly Bears (tapped attacker)
   - Grizzly Bears should be destroyed
   - Test should verify Bears in graveyard

Test file: tests/puzzle_e2e.rs::test_royal_assassin_with_log_capture
