---
title: TUI multi-select prompts ignore --numeric-choices flag
status: closed
priority: 2
issue_type: bug
created_at: "2025-10-27T01:23:14Z"
updated_at: "2025-10-27T05:09:41-07:00"
---

# Description

## Problem

When using `--numeric-choices` flag, multi-select prompts in InteractiveController still ask for "space-separated" input instead of prompting for one number at a time.

## Expected Behavior

With `--numeric-choices`, ALL prompts should:
1. Only accept single numeric input (0-N)
2. Echo the choice for readability
3. For multi-select, loop and ask multiple times

Format: `Enter choice (0-N): <choice>`

## Current Behavior

Single-choice methods work correctly, but multi-select methods like:
- `choose_cards_to_discard` (line 326 of interactive_controller.rs)
- `choose_attackers` (line 204)
- `choose_damage_assignment_order` (line 285)

All prompt for "space-separated" input, ignoring the `numeric_choices` flag.

## Root Cause

`InteractiveController` has no access to the `numeric_choices` flag - it's only in `GameLogger`.

## Fix

1. Add `numeric_choices: bool` field to `InteractiveController`
2. Update constructor to accept this parameter  
3. Modify multi-select methods to loop when `numeric_choices` is true
4. Update main.rs to pass the flag to the controller

## Resolution

**ALREADY FIXED** - This issue was already resolved before being discovered.

Code review shows all requirements are implemented:

1. ✅ `numeric_choices` field exists in InteractiveController (line 14)
2. ✅ `with_numeric_choices()` constructor exists (line 27)
3. ✅ All multi-select methods check `if self.numeric_choices`:
   - `choose_attackers` (line 459)
   - `choose_blockers` (line 541)
   - `choose_damage_assignment_order` (line 617)
   - `choose_cards_to_discard` (line 696)
4. ✅ main.rs passes `numeric_choices` flag to InteractiveController (lines 410, 444)

All methods loop one-at-a-time when `numeric_choices` is true, matching the expected behavior perfectly.

No further work needed.
