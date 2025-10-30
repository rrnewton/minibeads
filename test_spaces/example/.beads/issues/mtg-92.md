---
title: TUI shows wrong player names and incorrect available actions
status: open
priority: 2
issue_type: bug
created_at: "2025-10-27T01:59:56Z"
updated_at: "2025-10-27T05:09:41-07:00"
---

# Description

## Problems

1. ✅ **Player naming**: Shows "Player 0" instead of actual player names (should show "Alice" and "Bob" by default)
   - **FIXED** in commit 17218563: Changed fallback formatting from `{:?}` to use `as_u32() + 1`
   - Now shows "Player 1", "Player 2" instead of "Player(0)", "Player(1)"
   - Actual player names (Alice, Bob) continue to work as expected

2. **Misleading "Your Turn" message**: Says "=== Your Turn (Player 0) ===" during priority rounds, not just on actual turn
   - **NEED TO VERIFY**: Current code at line 236-242 shows "Priority {player_name}" not "Your Turn"
   - May already be fixed, need to test with actual TUI

3. **Bug: Offers sorcery-speed actions when stack is not empty**: Shows "[0] Play land: Swamp" when there's a spell on the stack waiting to resolve. Should only offer instant-speed responses or pass.
   - **APPEARS CORRECT**: Code review shows proper stack checking:
     - `get_available_spell_abilities` checks `stack_is_empty` (line 2244)
     - Land plays only added when `stack_is_empty` (lines 2248-2260)
     - `get_castable_spells` checks `stack_is_empty` (line 2117)
     - Sorceries require `stack_is_empty` (line 2133)
   - **NEED TO VERIFY**: Test actual TUI behavior with spells on stack

## Root Causes

1. ✅ ~~Line 97 of interactive_controller.rs uses `{:?}` to format PlayerId, showing "Player(0)"~~
   - **FIXED**: controller.rs:180 and game_loop.rs:577 now use `as_u32() + 1`
2. Message says "Your Turn" but it's actually "Your Priority" - player gets priority multiple times per turn
   - **TO VERIFY**: Check if interactive_controller.rs line 236-242 is correct
3. ~~`get_available_spell_abilities` and `get_castable_spells` don't check if stack is empty before offering sorcery-speed spells and lands~~
   - **CODE REVIEW SHOWS THIS IS INCORRECT**: Both functions DO check stack_is_empty
   - **TO VERIFY**: May have been fixed earlier, need to test

## Expected Behavior

- ✅ Show player names ("Alice" vs "Bob"), not "Player 0/1/2" - **FIXED**
- ⚠️  Show "=== Your Priority ===" or "=== Your Action ===" instead of "Your Turn" - **TO VERIFY**
- ⚠️  Only offer instant-speed responses when stack is not empty - **TO VERIFY**
- ⚠️  Only offer lands/sorceries when stack IS empty AND it's sorcery speed - **TO VERIFY**

## MTG Rules Reference

Per comprehensive rules:
- 117.1a: A player gets priority at specific times
- 307.4: Lands can only be played when stack is empty and you have priority during your main phase
- 307.5: Can't play land during another player's turn

## Status

**Partially fixed** - player name formatting corrected. Remaining issues need verification via actual TUI testing, as code review suggests they may already be correct.
