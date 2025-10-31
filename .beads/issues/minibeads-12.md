---
title: Implement bd sync for bidirectional jsonl/markdown sync
status: open
priority: 3
issue_type: feature
depends_on:
  minibeads-11: blocks
created_at: 2025-10-30T14:11:16.056738532+00:00
updated_at: 2025-10-31T04:19:46.729880907+00:00
---

# Description

NOTE: This issue describes a feature that conflicts with the minibeads architecture.

## Architectural Clarification (2025-10-31)

Per PROJECT_VISION.md, the **markdown files in `.beads/issues/` are the single source of truth**. The `issues.jsonl` file is NOT part of the core storage architecture - it's only created when explicitly exporting with `bd export`.

This means:
- ❌ **No bidirectional sync needed** - there's no dual representation to sync
- ❌ Jsonl does NOT store issue state that needs to be synced back
- ✅ Markdown is the only source of truth
- ✅ Export to jsonl is one-way only (for interop/backup)

## What This Issue Originally Described

Intelligent bidirectional synchronization between issues.jsonl and markdown storage formats.

### Background
Once we have `bd export` working, we need a sync mechanism to work with upstream bd commits that modify issues.jsonl. This enables collaboration where some developers use upstream bd (jsonl) and others use minibeads (markdown).

## Current Status

**This feature is not aligned with minibeads architecture.**

If collaboration with upstream bd users is needed, the correct approach would be:
1. Upstream bd users export to jsonl
2. Convert jsonl to markdown (import)
3. Work in markdown (minibeads)
4. Export back to jsonl if needed

This would be a **one-way import** feature (jsonl → markdown), not bidirectional sync.

## Recommendation

- Close this issue as "won't implement" due to architectural mismatch
- OR repurpose as "bd import" feature (one-way: jsonl → markdown)
- See minibeads-16 (duplicate issue)

Dependencies:
  minibeads-11 (blocks)

---

**Checked up-to-date as of 2025-10-31_#70(fe445a9)**

Architectural mismatch identified. Markdown is the single source of truth per PROJECT_VISION.md.
