---
title: Stop-and-go game snapshots for interactive exploration
status: closed
priority: 3
issue_type: feature
created_at: "2025-10-27T00:03:50Z"
updated_at: "2025-10-27T05:58:46-04:00"
---

# Description

Implement replay-based game snapshots using undo log

**New Approach** (replay-based, Option 3):
Save GameState only at turn boundaries + log of intra-turn choices.

**Design**:
1. Snapshots save: GameState (at turn start) + list of choices made during turn
2. On stop: Rewind undo log to turn start, collecting choice history in reverse
3. On resume: Replay choices with buffered logging until stop point
4. Exercises both serialization AND undo/redo infrastructure

## Status: COMPLETED (2025-10-27)

All functionality implemented across multiple commits:

✅ **Save-side infrastructure (commits up to 6b78a4de):**
1. Add ChoicePoint to undo log - Already existed in GameAction enum
2. Track turn boundaries in undo log - Already tracked via ChangeTurn
3. Implement rewind-to-turn-start with choice extraction - Done in src/undo.rs
4. Add buffered logging mode to GameLogger - Done in src/game/logger.rs
5. Update snapshot format - Done in src/game/snapshot.rs
6. CLI integration (--stop-every, --snapshot-output flags) - Done in src/main.rs
7. Game loop integration to trigger rewind on choice limit - Done in src/game/game_loop.rs
8. DRY refactoring of game loop - Extracted helper methods

✅ **Replay-side infrastructure (commits 51c9d7bd through 7823caf9):**
1. Fix library reshuffle bug when resuming from snapshot (51c9d7bd)
2. Design and implement ReplayController architecture (53043787)
   - Created src/game/replay_controller.rs
   - ReplayChoice enum for all controller decision types
   - Wrapper pattern that replays then delegates to base controller
3. Extend GameAction::ChoicePoint to store actual choice data (53043787)
   - Added optional ReplayChoice field to ChoicePoint
4. Populate choice data at all log_choice_point call sites (24628ada)
   - Captured choices for spells, targets, mana, attackers, blockers, discard
5. Integrate ReplayController with snapshot loading (7823caf9)
   - Extract replay choices from loaded snapshots
   - Wrap controllers with ReplayController when resuming

**Benefits achieved:**
- Clean save points at turn boundaries
- Deterministic replay of intra-turn state
- Tests undo/redo infrastructure thoroughly
- Controllers can be any type (Zero, Random, Heuristic, Interactive)

**Note:** Buffered logging suppression during replay was deemed optional and not implemented. 
Seeing replay output can be useful for debugging.

# Notes

## Progress Update (2025-10-27, commit 6b78a4de)

Completed all save-side infrastructure for replay-based snapshots:

✅ **Completed:**
1. Add ChoicePoint to undo log - Already existed in GameAction enum
2. Track turn boundaries in undo log - Already tracked via ChangeTurn
3. Implement rewind-to-turn-start with choice extraction - Done in src/undo.rs
4. Add buffered logging mode to GameLogger - Done in src/game/logger.rs
5. Update snapshot format - Done in src/game/snapshot.rs (GameSnapshot struct with save/load methods)
6. CLI integration (--stop-every, --snapshot-output flags) - Done in src/main.rs
7. Game loop integration to trigger rewind on choice limit - Done in src/game/game_loop.rs
8. Test with royal_assassin decks - Verified working
9. DRY refactoring of game loop - Extracted helper methods to eliminate duplication

📋 **Remaining:**
- Implement replay logic for intra-turn choices (--start-from flag)
  * Load GameSnapshot from file
  * Enable buffered logging on GameLogger
  * Replay choices from snapshot
  * Flush buffer at stop point
- End-to-end integration testing

**Next steps:** Implement the replay/resume functionality to complete the stop-and-go snapshot feature.
