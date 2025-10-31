---
title: Implement bidirectional sync (bd sync command)
status: open
priority: 1
issue_type: feature
created_at: 2025-10-31T11:44:22.314553235+00:00
updated_at: 2025-10-31T11:52:44.090757091+00:00
---

# Description

Implement bidirectional synchronization between markdown and JSONL formats.

## Architecture Change

This feature changes minibeads to support **dual sources of truth**:
- Markdown files (.beads/issues/*.md) - human-friendly, git-mergeable
- JSONL file (issues.jsonl) - machine-friendly, upstream bd compatible

Either format can be modified independently, and 'bd sync' merges changes bidirectionally.

## Implementation Approach

### Phase 1: Non-Conflicting Sync (Initial Implementation)

**Algorithm:**
1. Parse all markdown issues into memory (HashMap<IssueId, (Issue, SystemTime)>)
   - Use filesystem mtime (last modified time) as the authoritative timestamp
   - Store both the parsed Issue and its file's mtime
2. Parse all JSONL issues into memory (HashMap<IssueId, (Issue, DateTime<Utc>)>)
   - Use the updated_at field from JSONL as the timestamp
3. Compare issue-by-issue using these timestamps
4. Classify each issue:
   - markdown_only: Create in JSONL
   - jsonl_only: Create in markdown
   - markdown_newer: Update JSONL from markdown (file mtime > JSONL updated_at)
   - jsonl_newer: Update markdown from JSONL (JSONL updated_at > file mtime)
   - no_change: Skip (timestamps match within tolerance)
   - conflict: Skip with warning (defer to Phase 2)

**Timestamp Strategy:**
- **Markdown issues**: Use filesystem mtime (last modified time) from stat()
  - Rationale: Manual edits to .md files won't update YAML frontmatter timestamps
  - Filesystem mtime is authoritative for detecting external changes
- **JSONL issues**: Use updated_at field from JSON
  - Rationale: JSONL is typically machine-generated, updated_at is reliable
- **Comparison**: Convert both to comparable format (SystemTime or DateTime)
- **Tolerance**: Allow small differences (e.g., 1 second) to handle filesystem precision

**Conflict handling (Phase 1):**
- If timestamps differ beyond tolerance: newer wins (no conflict)
- If timestamps equal but content differs: flag as conflict, skip, log warning
- Defer interactive conflict resolution to Phase 2

### Components to Implement

1. **JSONL Import** (src/storage.rs)
   - import_from_jsonl() function
   - Parse JSONL into Issue structs
   - Write to markdown files
   - Set file mtime to match JSONL updated_at (preserve timestamps)
   - Handle overwrite flag

2. **Sync Engine** (new file: src/sync.rs)
   - SyncEngine struct
   - SyncPlan struct (holds categorized issues)
   - SyncReport struct (results summary)
   - TimestampedIssue enum (Markdown(Issue, SystemTime) | Jsonl(Issue, DateTime))
   - analyze() - compare both formats using filesystem mtime vs JSONL updated_at
   - apply() - execute plan atomically, preserve timestamps when writing

3. **CLI Command** (src/main.rs)
   - Add Sync subcommand
   - Flags: --dry-run, --jsonl <path>, --direction
   - Optional: --tolerance-ms for timestamp comparison tolerance

4. **Architecture Docs** (PROJECT_VISION.md)
   - Document dual storage model
   - Explain sync strategy with filesystem mtime approach
   - Update from "markdown-only" to "dual format"
   - Already updated (2025-10-31)

5. **Issue Updates**
   - Update minibeads-12 (originally marked "won't implement") - DONE
   - Update minibeads-16 (duplicate issue) - DONE
   - Mark architectural decision change - DONE

### Testing

**Unit tests (src/sync.rs):**
- test_sync_markdown_newer (using mocked filesystem mtime)
- test_sync_jsonl_newer
- test_sync_new_in_markdown
- test_sync_new_in_jsonl
- test_sync_no_changes (within tolerance)
- test_timestamp_comparison (mtime vs DateTime conversion)

**E2E tests (tests/sync.sh):**
- Create issues in markdown
- Export to JSONL
- Manually edit markdown file (touch to update mtime), sync, verify JSONL updated
- Modify JSONL (updated_at field), sync, verify markdown updated
- Test dry-run mode
- Test conflict detection (same timestamp, different content)
- Test timestamp preservation when syncing

### Performance Considerations

Following OPTIMIZATION.md zero-copy principles:
- Use HashMap for O(1) lookups (avoid repeated iterations)
- Use references during comparison (avoid clone)
- Use iterators for filtering (no intermediate allocations)
- Batch writes to minimize I/O
- Single stat() call per markdown file (cache mtime)

### Edge Cases

- Missing JSONL file: Initialize from markdown (one-way export)
- Empty markdown: Initialize from JSONL (one-way import)
- Dependency validation: Ensure referenced issues exist
- Timestamp precision: Handle different filesystem precision (ext4=ns, FAT32=2s)
- Timestamp preservation: Set file mtime when writing markdown from JSONL
- Git operations: mtime changes after checkout/pull (acceptable, triggers re-sync)

### Implementation Details

**Filesystem mtime handling:**
```rust
// Get markdown file mtime
let metadata = fs::metadata(&markdown_path)?;
let mtime: SystemTime = metadata.modified()?;

// Compare with JSONL updated_at
let jsonl_time: SystemTime = jsonl_issue.updated_at.into();
if mtime > jsonl_time { /* markdown newer */ }
```

**Timestamp preservation when writing:**
```rust
// After writing markdown from JSONL, set file mtime
use filetime::{set_file_mtime, FileTime};
let mtime = FileTime::from_system_time(jsonl_issue.updated_at.into());
set_file_mtime(&markdown_path, mtime)?;
```

**Dependency:** Will need `filetime` crate for mtime manipulation.

## Success Criteria

Phase 1 complete when:
- bd sync command works for non-conflicting changes
- Filesystem mtime (markdown) vs updated_at (JSONL) comparison works correctly
- New issues in either format are created in the other
- Timestamps are preserved when syncing (file mtime set appropriately)
- Conflicts are detected and logged (not resolved)
- Unit and E2E tests pass
- Documentation updated
- Zero-copy patterns followed

## Future Work (Phase 2+)

- Phase 2: Interactive conflict resolution
- Phase 3: Auto-sync with watch mode (inotify/FSEvents)
- Phase 4: Git hooks integration
- Phase 5: Three-way merge with git history as common ancestor

## Dependencies

Depends on:
- minibeads-11 (bd export) - COMPLETED
- Architectural decision to support dual formats - APPROVED

Related issues:
- minibeads-12 (original sync issue, marked "won't implement", now approved)
- minibeads-16 (duplicate sync issue)

## Notes

Using filesystem mtime instead of YAML frontmatter updated_at ensures that:
1. Manual edits to markdown files are detected
2. Git operations (checkout, merge) that change files are detected
3. External tools modifying markdown are detected
4. Sync remains robust even if updated_at in frontmatter is stale

# Notes

## Implementation Progress

### Completed (2025-10-31):
1. ✅ Added filetime dependency to Cargo.toml
2. ✅ Implemented import_from_jsonl() in storage.rs (lines 1008-1093)
   - Parses JSONL line-by-line
   - Converts to markdown and writes files
   - Preserves timestamps using set_file_mtime_from_issue()
   - Returns (imported_count, skipped_count, errors)
   - Handles overwrite flag
3. ✅ Implemented mtime helper functions:
   - set_file_mtime_from_issue() - sets file mtime from issue.updated_at
   - get_file_mtime() - reads filesystem mtime for comparison
4. ✅ All code compiles and passes validation (fmt, clippy, tests)

### In Progress:
- Sync engine (src/sync.rs) - not yet started
- CLI command wiring - not yet started

### Remaining Work:
- Create src/sync.rs with SyncEngine, SyncPlan, comparison logic
- Implement sync apply logic with atomic updates
- Wire up bd sync CLI command
- Write unit tests for sync functionality
- Write E2E test script (tests/sync.sh)
- Update documentation

**Next commit will include:** Sync engine core implementation
