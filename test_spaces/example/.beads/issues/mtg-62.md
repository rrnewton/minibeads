---
title: Overall MTG Forge Rust development tracking
status: closed
priority: 0
issue_type: epic
labels:
  - tracking
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-24T20:45:32-04:00"
---

# Description

This is the main tracking issue for MTG Forge Rust development.

**Major tracking issues (priority 1):**
- mtg-2: Optimization and performance tracking
- mtg-3: MTG feature completeness (keywords, abilities, effects)
- mtg-4: Gameplay features (TUI, human play, controls)
- mtg-5: Cross-cutting codebase issues (APIs, testing, architecture)

**Current status as of commit#179(b6bee0df):**
- Tests: 192 passing (169 lib + 10 card_loading + 4 determinism + 7 tui + 2 undo)
- Examples: 13/13 passing
- Performance (fresh mode): ~6,214 games/sec, ~7.96M actions/sec
- Performance (snapshot mode): ~7,726 games/sec, ~9.90M actions/sec
- Cards: 31k+ supported from cardsfolder

**Conventions:**
- Tracking issues (priority 1) reference granular issues
- Granular issues have priority 3-4 unless critical bugs (priority 2)
- Human-created issues have priority 0
- Reference issues in code: // TODO(mtg-N): description
- Transient info includes timestamp: commit#N(hash)

Checked up-to-date as of 2025-10-24.
