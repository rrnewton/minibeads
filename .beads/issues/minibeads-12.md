---
title: Implement bd sync for bidirectional jsonl/markdown sync
status: open
priority: 3
issue_type: feature
depends_on:
  minibeads-11: blocks
created_at: 2025-10-30T14:11:16.056738532+00:00
updated_at: 2025-10-30T14:11:26.346835477+00:00
---

# Description

Intelligent bidirectional synchronization between issues.jsonl and markdown storage formats.

## Background
Once we have `bd export` working, we need a sync mechanism to work with upstream bd commits that modify issues.jsonl. This enables collaboration where some developers use upstream bd (jsonl) and others use minibeads (markdown).

## Proposed Design

### Sync Strategy
The sync command should:
1. Detect which format has newer changes (compare timestamps)
2. Prefer newer changes automatically where possible
3. Detect conflicts (both modified since last sync)
4. Error on conflicts with clear resolution options

### CLI Command
```bash
bd sync                         # Sync jsonl <-> markdown
bd sync --dry-run              # Preview changes
bd sync --prefer-jsonl         # Resolve conflicts by preferring jsonl
bd sync --prefer-markdown      # Resolve conflicts by preferring markdown
bd sync --interactive          # Prompt for conflict resolution
```

### Sync Metadata
Store sync state in `.beads/sync_state.yaml`:
```yaml
last_sync: 2025-10-30T14:00:00Z
jsonl_hash: abc123def456
markdown_hashes:
  minibeads-1: def789abc012
  minibeads-2: 012abc345def
```

### Conflict Detection
For each issue:
- If only jsonl changed: update markdown
- If only markdown changed: update jsonl
- If both changed: detect conflict
  - Compare timestamps (updated_at field)
  - Compare content hashes
  - Require user resolution

### Implementation Steps
1. Load sync state from previous sync
2. Read both jsonl and markdown
3. Compare timestamps and hashes
4. Build change list
5. Apply changes with validation
6. Update sync state
7. Report summary

### Use Cases
- Pull changes from upstream bd users: `bd sync`
- Push minibeads changes to issues.jsonl: `bd sync`
- Preview what would change: `bd sync --dry-run`
- Resolve conflicts: `bd sync --interactive`

## Dependencies
This feature depends on having `bd export` working first to handle the jsonl format.
