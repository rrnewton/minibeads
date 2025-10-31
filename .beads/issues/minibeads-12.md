---
title: Implement bd sync for bidirectional jsonl/markdown sync
status: open
priority: 3
issue_type: feature
depends_on:
  minibeads-11: blocks
  minibeads-19: related
created_at: 2025-10-30T14:11:16.056738532+00:00
updated_at: 2025-10-31T11:45:17.791631043+00:00
---

# Description

## Status Update (2025-10-31)

**ARCHITECTURE DECISION CHANGED** - Now implementing bidirectional sync via minibeads-19.

## New Approach (Dual Source of Truth)

Minibeads will support **two independent sources of truth**:
- **Markdown files** (.beads/issues/*.md) - human-friendly, git-mergeable
- **JSONL file** (issues.jsonl) - machine-friendly, upstream bd compatible

Either format can be modified independently, and `bd sync` merges changes bidirectionally.

## Previous Status

This issue was originally marked as "won't implement" due to architectural mismatch with the markdown-only design documented in PROJECT_VISION.md (as of 2025-10-31_#70).

The original architecture had:
- ❌ Markdown as the ONLY source of truth
- ❌ JSONL as export-only format
- ❌ No bidirectional sync needed

## New Architecture Decision

After user request, the architecture has been changed to support:
- ✅ Markdown AND JSONL as dual sources of truth
- ✅ Bidirectional synchronization via `bd sync`
- ✅ Timestamp-based conflict detection
- ✅ Non-conflicting updates handled automatically

## Implementation

**Tracking issue:** minibeads-19 (Implement bidirectional sync)

**Algorithm:**
1. Parse all markdown issues into memory
2. Parse all JSONL issues into memory
3. Compare using updated_at timestamps
4. Apply non-conflicting changes (newer wins)
5. Flag conflicts for manual resolution (Phase 2)

**Phase 1 scope:**
- Handle new issues in either format
- Handle updates where one format is newer
- Detect and log conflicts (skip for now)
- Defer interactive conflict resolution

## Related Issues

- minibeads-16: Duplicate of this issue
- minibeads-19: Implementation tracking issue

## Next Steps

See minibeads-19 for detailed implementation plan.
