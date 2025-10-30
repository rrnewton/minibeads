---
title: Fix player name consistency throughout TUI
status: closed
priority: 2
issue_type: bug
created_at: "2025-10-26T23:49:49Z"
updated_at: "2025-10-27T10:42:00Z"
---

# Description

Player names are inconsistent during gameplay. Seeing:
- "Turn 22 - Player 2's turn"  
- "Player 1 discards Royal Assassin"
- "=== Your Turn (Player 0) ==="

All in the same game. This is confusing.

**Requirements:**
1. Use one consistent name per player throughout entire game
2. Add --p1-name and --p2-name CLI options  
3. If names are "Alice" and "Bob", never show any other names
4. Fix all display code to use player.name field consistently
5. Search for "Player 0", "Player 1", "Player {:?}" patterns

**Files likely involved:**
- src/main.rs (CLI options, initialization)
- src/game/game_loop.rs (turn announcements, logging)
- src/game/interactive_controller.rs (TUI display)
- src/game/logger.rs (game event logging)

# Resolution

**FIXED** - All requirements implemented:

1. ✅ **CLI options added** - main.rs lines 92-98:
   - `--p1-name` (default: "Alice")
   - `--p2-name` (default: "Bob")

2. ✅ **Names passed to game initialization** - main.rs lines 365, 367:
   - TUI uses actual player names from CLI args

3. ✅ **Fallback formatting fixed** - commit 17218563:
   - controller.rs:180 - Changed from `Player {:?}` to `Player {N}` (1-based)
   - game_loop.rs:577 - Changed from `Player {:?}` to `Player {N}` (1-based)

4. ✅ **Verified no remaining issues**:
   - No `Player {:?}` patterns found in src/
   - Hardcoded "Player 1"/"Player 2" only in:
     - Profiling mode (acceptable - not interactive)
     - Replay controller (acceptable - testing utility)
     - Puzzle loader (acceptable - testing utility)
     - Test assertions (acceptable)

The original issue about "=== Your Turn (Player 0) ===" would have been caused
by the Debug formatting (`{:?}`) which has been fixed. All interactive gameplay
now uses consistent player names (Alice/Bob by default, customizable via CLI).
