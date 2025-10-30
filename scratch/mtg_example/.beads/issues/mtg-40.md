---
title: Migrate game loop from v1 to v2 controller interface
status: closed
priority: 3
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-27T10:50:00Z"
closed_at: "2025-10-27T10:50:00Z"
---

# Description

Complete migration to v2 controller architecture:
- Game loop currently uses v1 interface
- v2 provides better zero-copy patterns (SmallVec, slices)
- v2 has specific callbacks (choose_land_to_play, choose_spell_to_cast, etc.)
- See CONTROLLER_DESIGN.md for architecture details

# Resolution

**OBSOLETE** - This issue is no longer applicable.

The controller architecture has already been unified. There is only one PlayerController trait that all controllers implement, using zero-copy patterns with SmallVec and slices.

Current implementation (verified 2025-10-27):
- Single unified PlayerController trait in src/game/controller.rs
- Method `choose_spell_ability_to_play()` handles lands, spells, and abilities
- GameStateView provides read-only access with zero-copy patterns
- 5 controller implementations: RandomController, ZeroController, HeuristicController, FixedScriptController, InteractiveController

The v1/v2 distinction no longer exists - we have one modern interface.

Marked as closed/obsolete per mtg-5 tracking issue.
