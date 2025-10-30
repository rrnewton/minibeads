---
title: Feature completeness tracking vs upstream bd
status: open
priority: 1
issue_type: task
created_at: 2025-10-30T16:22:39.915813892+00:00
updated_at: 2025-10-30T16:22:39.915813892+00:00
---

# Description

Track all missing features, flags, and commands compared to upstream beads. This issue audits minibeads CLI completeness by comparing `bd --help` output for every command with upstream beads.

## Purpose

Ensure minibeads provides full compatibility with the beads MCP server, which relies on specific `bd` CLI commands and flags. Any gaps here could cause MCP integration failures.

## Global Flags Comparison

### ✅ Implemented (Compatible)
- `--actor` - Actor name for audit trail
- `--db` - Database path
- `--json` - JSON output format
- `--no-auto-flush` - (ignored, for compatibility)
- `--no-auto-import` - (ignored, for compatibility)

### ❌ Missing Global Flags
- `--no-daemon` - Force direct storage mode, bypass daemon
- `--no-db` - Use no-db mode (load from JSONL, no SQLite)
- `--no-json` - Disable all JSON export/import
- `--sandbox` - Sandbox mode (equivalent to --no-daemon --no-auto-flush --no-auto-import)

### 🔧 Minibeads-Specific Flags (prefixed with --mb)
- `--mb-validation` - Validation mode (silent, warn, error)
- `--mb-no-cmd-logging` - Disable command history logging

## Command Completeness

### ✅ Implemented Commands
- `init` - Initialize beads in current directory
- `create` - Create a new issue
- `list` - List issues
- `show` - Show issue details
- `update` - Update an issue
- `close` - Close an issue
- `reopen` - Reopen closed issues
- `dep add` - Add a dependency
- `stats` - Get statistics
- `blocked` - Get blocked issues
- `ready` - Find ready work
- `quickstart` - Show quickstart guide
- `version` - Show version information

### ❌ Missing Commands (Not Planned - Advanced Features)
- `comments` - View/manage comments (see minibeads-9)
- `compact` - Compact old closed issues (not needed for markdown)
- `completion` - Generate shell completion scripts
- `config` - Manage configuration settings
- `daemon` - Run background sync daemon (not planned)
- `delete` - Delete issues and clean up references
- `duplicates` - Find/merge duplicate issues
- `edit` - Edit issue field in $EDITOR
- `epic` - Epic management commands
- `info` - Show database and daemon information
- `label` - Manage issue labels
- `merge` - Merge duplicate issues
- `onboard` - Display AGENTS.md configuration instructions
- `rename-prefix` - Rename issue prefix
- `renumber` - Renumber issues to compact ID space
- `restore` - Restore full history of compacted issue
- `stale` - Show orphaned claims and dead executors
- `sync` - Synchronize issues with git remote (see minibeads-12)

### ⚠️ Missing Commands (Priority - MCP Needed)
- `export` - Export issues to JSONL format (see minibeads-11)
- `import` - Import issues from JSONL format (see minibeads-11)
- `dep remove` - Remove a dependency
- `dep tree` - Show dependency tree
- `dep cycles` - Detect dependency cycles

---

## Command-by-Command Analysis

### `bd init`

**✅ Implemented Features:**
- `--prefix` - Custom issue prefix
- Auto-prefix detection from directory name
- Creates .beads/config.yaml and .beads/issues/

**❌ Missing Flags:**
- `-p` short form for `--prefix`
- `-q, --quiet` - Suppress output
- `--backend` - Storage backend selection (not needed, we're markdown-only)

**Status:** Mostly complete, missing convenience flags

---

### `bd create`

**✅ Implemented Features:**
- `-p, --priority` - Priority (0-4)
- `-t, --issue-type` - Issue type (bug/feature/task/epic/chore)
- `-d, --description` - Description
- `--design` - Design notes
- `--acceptance` - Acceptance criteria
- `--assignee` - Assignee
- `-l, --label` - Labels (multiple)
- `--external-ref` - External reference
- `--id` - Explicit issue ID
- `--deps` - Dependencies (comma-separated)

**❌ Missing Flags:**
- `-a` short form for `--assignee`
- `-f, --file` - Create multiple issues from markdown file
- `--force` - Force creation even if prefix doesn't match
- `--title` - Alternative to positional title argument

**⚠️ Feature Gaps:**
- `--deps` format: Upstream supports `type:id` format (e.g., `discovered-from:bd-20,blocks:bd-15`), we only support comma-separated IDs
- `-l, --labels` vs our `-l, --label` - Upstream uses plural, we use singular (but both work)

**Status:** Core features complete, missing bulk creation and advanced dep syntax

---

### `bd list`

**✅ Implemented Features:**
- `-s, --status` - Filter by status ✅ (just added)
- `-p, --priority` - Filter by priority ✅ (just added)
- `-t, --type` - Filter by type
- `--assignee` - Filter by assignee
- `--limit` - Limit results (no default) ✅ (just fixed)

**❌ Missing Flags:**
- `-a` short form for `--assignee`
- `-l, --label` - Filter by labels (AND: must have ALL)
- `--label-any` - Filter by labels (OR: must have AT LEAST ONE)
- `-n` short form for `--limit`
- `--id` - Filter by specific issue IDs (comma-separated)
- `--title` - Filter by title text (case-insensitive substring)
- `--format` - Output format (digraph, dot, Go template)

**Status:** Basic filtering works, missing advanced filters (labels, title, id, custom formats)

---

### `bd show`

**✅ Implemented Features:**
- Shows single issue with full details
- `--json` output

**❌ Missing Features:**
- Multiple issue IDs as arguments (e.g., `bd show bd-1 bd-2 bd-3`)
- `--all-issues` - Show all issues
- `-p, --priority` - Show issues with specified priority (can be used multiple times)

**Status:** Basic single-issue show works, missing bulk show features

---

### `bd update`

**✅ Implemented Features:**
- `--status` - New status
- `--priority` - New priority
- `--assignee` - New assignee
- `--title` - New title
- `--description` - New description
- `--design` - New design notes
- `--acceptance` - New acceptance criteria
- `--notes` - Additional notes
- `--external-ref` - New external reference

**❌ Missing Flags:**
- `-s` short form for `--status`
- `-p` short form for `--priority`
- `-a` short form for `--assignee`
- `-d` short form for `--description`

**⚠️ Feature Gaps:**
- Upstream supports multiple issue IDs as arguments (bulk update)
- We only support single issue update

**Status:** Feature-complete for single issues, missing bulk operations and short flags

---

### `bd close`

**✅ Implemented Features:**
- Closes single issue
- `--reason` - Reason for closing

**❌ Missing Flags:**
- `-r` short form for `--reason`

**⚠️ Feature Gaps:**
- Upstream supports multiple issue IDs as arguments (bulk close)
- We only support single issue close

**Status:** Works for single issues, missing bulk close and short flag

---

### `bd reopen`

**✅ Implemented Features:**
- Supports multiple issue IDs
- `--reason` - Reason for reopening

**❌ Missing Flags:**
- `-r` short form for `--reason`

**Status:** Feature-complete, missing only short flag

---

### `bd dep`

**✅ Implemented Subcommands:**
- `dep add` - Add a dependency
  - `--type` - Dependency type (blocks/related/parent-child/discovered-from)

**❌ Missing Subcommands:**
- `dep remove` - Remove a dependency
- `dep tree` - Show dependency tree
  - `-d, --max-depth` - Maximum tree depth (default 50)
  - `--show-all-paths` - Show all paths to nodes
- `dep cycles` - Detect dependency cycles

**Missing Flags in `dep add`:**
- `-t` short form for `--type`

**Status:** Only basic dep add implemented, missing tree visualization and cycle detection

---

### `bd stats`

**✅ Implemented Features:**
- Shows total, open, in_progress, blocked, closed, ready issues
- Shows average lead time in hours

**Status:** ✅ Feature-complete

---

### `bd blocked`

**✅ Implemented Features:**
- Shows blocked issues and what blocks them

**Status:** ✅ Feature-complete

---

### `bd ready`

**✅ Implemented Features:**
- `--assignee` - Filter by assignee
- `--priority` - Filter by priority
- `--limit` - Maximum issues to show (default: 10)

**❌ Missing Flags:**
- `-a` short form for `--assignee`
- `-p` short form for `--priority`
- `-n` short form for `--limit`
- `-s, --sort` - Sort policy (hybrid, priority, oldest) - upstream default: "hybrid"

**Status:** Core features work, missing sort policy and short flags

---

## Priority Action Items

### P0 - Critical (MCP Integration)
1. ✅ Fix `bd list` default limit (DONE - commit 85f7d77)
2. ✅ Add `-s` and `-p` short flags to `bd list` (DONE - commit 85f7d77)
3. Verify MCP server compatibility with current command set
4. Fix minibeads-13 (dependencies/dependents schema bug)

### P1 - High Priority (Common Usage)
1. ✅ DONE - Add short flags throughout:
   - ✅ `bd init -p` (for --prefix)
   - ✅ `bd create -a` (for --assignee)
   - ✅ `bd update -s, -p, -a, -d` (for status, priority, assignee, description)
   - ✅ `bd close -r` (for --reason)
   - ✅ `bd reopen -r` (for --reason)
   - ✅ `bd ready -a, -p, -n` (for assignee, priority, limit)
   - ✅ `bd dep add -t` (for --type)

2. ✅ DONE - Add bulk operations:
   - ✅ `bd show bd-1 bd-2 bd-3` - Show multiple issues
   - ✅ `bd update bd-1 bd-2 --status in_progress` - Update multiple issues
   - ✅ `bd close bd-1 bd-2` - Close multiple issues

3. Advanced `bd list` filters:
   - `--label` and `--label-any` - Label filtering
   - `--id` - Filter by specific IDs
   - `--title` - Filter by title text

### P2 - Medium Priority (Dependency Management)
1. Implement `bd dep remove` - Remove dependencies
2. Implement `bd dep tree` - Show dependency tree visualization
3. Implement `bd dep cycles` - Detect circular dependencies
4. Support advanced `--deps` syntax in `bd create`:
   - Format: `type:id,type:id` (e.g., `discovered-from:bd-20,blocks:bd-15`)

### P3 - Low Priority (Nice to Have)
1. `bd show --all-issues` - Show all issues
2. `bd show -p 0 -p 1` - Show by priority
3. `bd ready --sort` - Sort policy for ready work
4. `bd list --format` - Custom output formats (digraph, dot, templates)
5. `bd init -q` - Quiet mode
6. `bd create -f` - Bulk create from markdown file

### P4 - Not Planned (Advanced Features)
- `bd export` / `bd import` - See minibeads-11
- `bd comments` - See minibeads-9
- `bd label` - Label management
- `bd edit` - Edit in $EDITOR
- `bd delete` - Delete issues
- `bd daemon` - Background sync daemon
- `bd compact` - Issue compaction
- `bd sync` - Git remote sync (see minibeads-12)
- `bd rename-prefix` - Rename prefix
- `bd renumber` - Renumber issues
- `bd completion` - Shell completion
- `bd config` - Config management
- `bd duplicates` / `bd merge` - Duplicate handling
- `bd epic` - Epic management
- `bd info` - Database info
- `bd onboard` - AGENTS.md configuration
- `bd restore` - Restore compacted issues
- `bd stale` - Stale claim detection

---

## Testing Plan

After implementing priority items:

1. **Unit tests** for each new flag/feature
2. **E2E tests** in `tests/basic_operations.sh` for bulk operations
3. **MCP integration tests** to verify beads-mcp compatibility (minibeads-14 Phase 4)
4. **Regression tests** for edge cases discovered in minibeads-13

---

## Related Issues

- minibeads-1: Overall project tracking
- minibeads-9: Comments data model (future)
- minibeads-10: Events/audit trail (future)
- minibeads-11: Export/import to JSONL (future)
- minibeads-12: Bidirectional sync (future)
- minibeads-13: Critical MCP integration bugs (URGENT)
- minibeads-14: Test porting plan

---

## Notes

This issue serves as the **central tracking issue for feature parity** with upstream beads. Individual features may be split into separate issues as work progresses.

**Key principle:** We prioritize MCP compatibility first, then common CLI usage patterns, then advanced features.
