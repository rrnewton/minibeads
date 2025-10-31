---
title: Implement bidirectional sync (bd sync)
status: open
priority: 3
issue_type: feature
created_at: 2025-10-30T20:38:48.178328137+00:00
updated_at: 2025-10-31T04:19:58.476725277+00:00
---

# Description

NOTE: This issue is a duplicate of minibeads-12 and conflicts with the minibeads architecture.

## Architectural Clarification (2025-10-31)

Per PROJECT_VISION.md, the **markdown files in `.beads/issues/` are the single source of truth**. The `issues.jsonl` file is NOT part of the core storage architecture - it's only created when explicitly exporting with `bd export`.

This means:
- ❌ **No bidirectional sync needed** - there's no dual representation to sync
- ❌ Jsonl does NOT store issue state that needs to be synced back
- ❌ No mb-auto-sync config option needed
- ✅ Markdown is the only source of truth
- ✅ Export to jsonl is one-way only (for interop/backup)

## What This Issue Originally Described

Implement bidirectional sync as specified in minibeads-12.

Requirements included:
- bd sync command for markdown ↔ jsonl sync
- Auto-sync configuration option
- Conflict resolution

## Current Status

**This issue should be closed as duplicate of minibeads-12, which itself conflicts with the architecture.**

See minibeads-12 for full discussion. If import functionality is needed, create a new issue for one-way import (jsonl → markdown) instead.

---

**Checked up-to-date as of 2025-10-31_#70(fe445a9)**

Marked as duplicate. Architectural mismatch identified. Markdown is the single source of truth per PROJECT_VISION.md.
