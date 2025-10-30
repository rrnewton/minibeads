---
title: 'TODO: rename test_decks "decks"'
status: closed
priority: 2
issue_type: task
labels:
  - human
created_at: "2025-10-27T06:51:48-07:00"
updated_at: "2025-10-28T00:00:00-07:00"
closed_at: "2025-10-28T00:00:00-07:00"
---

# Description

Let's use a shorter name. Just be sure to search for all references across the repository and replace them all, making sure that validation passes.

# Resolution

Completed: Renamed `test_decks/` to `decks/` and updated all references across the repository:
- src/main.rs
- tests/determinism_e2e.rs
- tests/tui_e2e.rs
- tests/undo_e2e.rs
- tests/heuristic_grizzly_bears_attack_e2e.sh
- tests/heuristic_royal_assassin_e2e.sh
- tests/interactive_tui_e2e.sh
- tests/snapshot_stress_test.py
- benches/game_benchmark.rs
- docs/DCK_FORMAT.md
- decks/README.md
- decks/old_school/README.md
- scripts/backfill_history.sh

Validation passed with all 365 tests passing.
