---
title: Implement bidirectional sync (bd sync)
status: open
priority: 3
issue_type: feature
depends_on:
  minibeads-19: related
created_at: 2025-10-30T20:38:48.178328137+00:00
updated_at: 2025-10-31T11:45:17.795154605+00:00
---

# Description

## Status Update (2025-10-31)

**ARCHITECTURE DECISION CHANGED** - Duplicate of minibeads-12. Now implementing via minibeads-19.

## New Approach (Dual Source of Truth)

Minibeads will support **two independent sources of truth**:
- **Markdown files** (.beads/issues/*.md) - human-friendly, git-mergeable
- **JSONL file** (issues.jsonl) - machine-friendly, upstream bd compatible

Either format can be modified independently, and `bd sync` merges changes bidirectionally.

## Previous Status

This issue was originally marked as duplicate of minibeads-12, which itself was marked "won't implement" due to architectural mismatch with the markdown-only design.

The original architecture had:
- ❌ Markdown as the ONLY source of truth
- ❌ JSONL as export-only format
- ❌ No bidirectional sync needed
- ❌ No mb-auto-sync config option needed

## New Architecture Decision

After user request, the architecture has been changed to support:
- ✅ Markdown AND JSONL as dual sources of truth
- ✅ Bidirectional synchronization via `bd sync`
- ✅ Timestamp-based conflict detection
- ✅ Non-conflicting updates handled automatically

## Implementation

**Primary tracking issue:** minibeads-12
**Implementation tracking issue:** minibeads-19

See minibeads-19 for detailed implementation plan.

## Recommendation

This issue should be closed as duplicate once minibeads-19 is implemented, since all functionality will be covered by the `bd sync` command.
