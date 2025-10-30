---
title: TUI shows wrong player names and incorrect available actions
status: open
priority: 2
issue_type: bug
created_at: "2025-10-27T01:59:56Z"
updated_at: "2025-10-27T09:24:06-04:00"
---

# Description

## Problems

1. **Player naming**: Shows "Player 0" instead of actual player names (should show "Alice" and "Bob" by default)
2. **Misleading "Your Turn" message**: Says "=== Your Turn (Player 0) ===" during priority rounds, not just on actual turn
3. **Bug: Offers sorcery-speed actions when stack is not empty**: Shows "[0] Play land: Swamp" when there's a spell on the stack waiting to resolve. Should only offer instant-speed responses or pass.

## Root Causes

1. Line 97 of interactive_controller.rs uses `{:?}` to format PlayerId, showing "Player(0)" 
2. Message says "Your Turn" but it's actually "Your Priority" - player gets priority multiple times per turn
3. `get_available_spell_abilities` and `get_castable_spells` don't check if stack is empty before offering sorcery-speed spells and lands

## Expected Behavior

- Show player names ("Alice" vs "Bob"), not "Player 0/1/2"
- Show "=== Your Priority ===" or "=== Your Action ===" instead of "Your Turn"
- Only offer instant-speed responses when stack is not empty
- Only offer lands/sorceries when stack IS empty AND it's sorcery speed

## MTG Rules Reference

Per comprehensive rules:
- 117.1a: A player gets priority at specific times
- 307.4: Lands can only be played when stack is empty and you have priority during your main phase
- 307.5: Can't play land during another player's turn
