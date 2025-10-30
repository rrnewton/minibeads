---
title: 'TUI: Fix missing choices and remove DEBUG messages'
status: closed
priority: 2
issue_type: task
created_at: "2025-10-26T23:01:11Z"
updated_at: "2025-10-27T05:58:46-04:00"
---

# Description

User reports TUI is lagging behind functionality:

**Issues:**
1. Only asks to play land, never to play creature/attack/use abilities
2. DEBUG messages from Royal Assassin testing still present (game_loop.rs:1896-1920, 1925-1950)

**Requirements:**
1. Remove all DEBUG eprintln! statements and debug_found_royal_assassin variable from game_loop.rs
2. Add --p1=fixed option with --fixed-inputs="1 1 2" argument to TUI
3. Test TUI with royal_assassin.dck and verify all choices appear
4. Fix get_available_spell_abilities() or related methods if needed

**User comment:**
"I think it's important that you directly use the TUI more to see how it is doing"
