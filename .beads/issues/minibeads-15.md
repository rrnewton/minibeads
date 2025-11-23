---
title: Feature completeness tracking vs upstream bd
status: open
priority: 1
issue_type: task
created_at: 2025-10-30T16:22:39.915813892+00:00
updated_at: 2025-11-23T06:03:01.588163229+00:00
---

# Description

Track all missing features, flags, and commands compared to upstream beads. This issue audits minibeads CLI completeness by comparing `bd --help` output for every command with upstream beads.

## Purpose

Ensure minibeads provides full compatibility with the beads MCP server, which relies on specific `bd` CLI commands and flags. Any gaps here could cause MCP integration failures.

## Global Flags Comparison

### ✅ Implemented (Compatible)
- `--actor` - Actor name for audit trail
- `--db` - Database path (syntactic sugar for upstream compatibility, see note below)
- `--json` - JSON output format
- `--no-auto-flush` - (ignored, for compatibility)
- `--no-auto-import` - (ignored, for compatibility)

### ❌ Missing Global Flags
- `--no-daemon` - Force direct storage mode, bypass daemon
- `--no-db` - Use no-db mode (load from JSONL, no SQLite)
- `--no-json` - Disable all JSON export/import
- `--sandbox` - Sandbox mode (equivalent to --no-daemon --no-auto-flush --no-auto-import)

### 🔧 Minibeads-Specific Flags (prefixed with --mb)
- `--mb-beads-dir` - ✅ **Preferred flag for specifying .beads directory** (added 2025-10-31_#98)
- `--mb-validation` - Validation mode (silent, warn, error)
- `--mb-no-cmd-logging` - Disable command history logging

**Note on --db flag:** As of commit #98 (2025-10-31), `--db` is treated as **syntactic sugar** for upstream bd compatibility. When `--db` points to a `.db` file, minibeads automatically uses the parent directory. The configuration priority order is:
1. `--mb-beads-dir` (minibeads-specific, preferred)
2. `--db` (upstream compatibility)
3. `MB_BEADS_DIR` environment variable
4. `BEADS_DB` environment variable (upstream compat)
5. Automatic .beads directory tree search

## Command Completeness

### ✅ Implemented Commands
- `init` - Initialize beads in current directory
- `create` - Create a new issue
- `list` - List issues
- `show` - Show issue details
- `update` - Update an issue
- `close` - Close an issue
- `reopen` - Reopen closed issues
- `rename` - Rename an issue ID
- `rename-prefix` - Rename the issue prefix for all issues
- `dep add` - Add a dependency
- `dep remove` - Remove a dependency
- `dep tree` - Show dependency tree
- `dep cycles` - Detect dependency cycles
- `export` - Export issues to JSONL format
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
- `renumber` - Renumber issues to compact ID space
- `restore` - Restore full history of compacted issue
- `stale` - Show orphaned claims and dead executors
- `sync` - Synchronize issues with git remote (see minibeads-12)

### ⚠️ Missing Commands (Priority - MCP Needed)
- `import` - Import issues from JSONL format (see minibeads-11)

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

**✅ Implemented Features (as of 2025-11-07):**
- `-p, --priority` - Priority (0-4) ✅
- `-t, --issue-type` - Issue type (bug/feature/task/epic/chore) ✅
- `-d, --description` - Description ✅
- `--design` - Design notes ✅
- `--acceptance` - Acceptance criteria ✅
- `-a, --assignee` - Assignee ✅
- `-l, --label` - Labels (multiple) ✅
- `--external-ref` - External reference ✅
- `--id` - Explicit issue ID ✅
- `--deps` - Dependencies (comma-separated) ✅
- `--title` - Alternative to positional title argument ✅ **NEW**
- `--parent` - Parent issue ID for hierarchical child ✅ **NEW**
- `--force` - Force creation even if prefix doesn't match ✅ **NEW**

**✅ Advanced --deps Syntax:**
- Simple format: `bd-1,bd-2` (defaults to 'blocks') ✅
- Advanced typed format: `blocks:bd-1,related:bd-2,discovered-from:bd-3` ✅

**❌ Missing Flags:**
- `-f, --file` - Create multiple issues from markdown file (placeholder added, not implemented)

**Status:** ✅ **Feature-complete for single issue creation!** All upstream flags implemented except bulk file creation.

---

### `bd list`

**✅ Implemented Features:**
- `-s, --status` - Filter by status ✅
- `-p, --priority` - Filter by priority (comma-separated list) ✅
- `-t, --type` - Filter by type ✅
- `--assignee` - Filter by assignee ✅
- `--limit` - Limit results (no default) ✅
- `-l, --label` - Filter by labels (must have ALL specified labels) ✅
- `--id` - Filter by specific issue IDs (comma-separated) ✅
- `--title` - Filter by title substring (case-insensitive) ✅
- `--group-priority` - Group issues by priority with headers ✅

**❌ Missing Flags:**
- `-a` short form for `--assignee`
- `--label-any` - Filter by labels (OR: must have AT LEAST ONE)
- `-n` short form for `--limit`
- `--format` - Output format (digraph, dot, Go template)

**Status:** All major filtering features implemented, missing only convenience short flags and custom output formats

---

### `bd show`

**✅ Implemented Features:**
- Shows single issue with full details ✅
- Multiple issue IDs as arguments (e.g., `bd show bd-1 bd-2 bd-3`) ✅
- `--json` output ✅

**❌ Missing Features:**
- `--all-issues` - Show all issues
- `-p, --priority` - Show issues with specified priority (can be used multiple times)

**Status:** Bulk show operations fully supported, missing only convenience filters

---

### `bd update`

**✅ Implemented Features:**
- `-s, --status` - New status ✅
- `-p, --priority` - New priority ✅
- `-a, --assignee` - New assignee ✅
- `--title` - New title ✅
- `-d, --description` - New description ✅
- `--design` - New design notes ✅
- `--acceptance` - New acceptance criteria ✅
- `--notes` - Additional notes ✅
- `--external-ref` - New external reference ✅
- Multiple issue IDs as arguments (bulk update) ✅

**Status:** ✅ Feature-complete with all short flags and bulk operations

---

### `bd close`

**✅ Implemented Features:**
- `-r, --reason` - Reason for closing ✅
- Multiple issue IDs as arguments (bulk close) ✅

**Status:** ✅ Feature-complete with short flags and bulk operations

---

### `bd reopen`

**✅ Implemented Features:**
- Supports multiple issue IDs ✅
- `-r, --reason` - Reason for reopening ✅

**Status:** ✅ Feature-complete

---

### `bd rename`

**✅ Implemented Features:**
- Rename an issue ID from old-id to new-id
- `--dry-run` - Preview changes without applying them
- `--repair` - Repair broken references (scan all issues and fix stale references)

**Status:** ✅ Feature-complete

---

### `bd rename-prefix`

**✅ Implemented Features:**
- Rename the issue prefix for all issues in the database
- `--dry-run` - Preview changes without applying them
- `--force` - Force rename even if issues would conflict
- Atomically updates all files, IDs, dependencies, and config.yaml
- Validates prefix format (alphanumeric and hyphens only)

**Status:** ✅ Feature-complete

---

### `bd export`

**✅ Implemented Features:**
- Export issues to JSONL format
- `-o, --output` - Output file path (defaults to stdout)
- `--mb-output-default` - Use default file output (.beads/issues.jsonl)
- Filter options: `--status`, `--priority`, `--type`, `--assignee`

**Status:** ✅ Feature-complete

---

### `bd dep`

**✅ Implemented Subcommands:**
- `dep add` - Add a dependency
  - `-t, --type` - Dependency type (blocks/related/parent-child/discovered-from)
- `dep remove` - Remove a dependency
- `dep tree` - Show dependency tree
  - `-d, --max-depth` - Maximum tree depth (default 50)
  - `--show-all-paths` - Show all paths to nodes
- `dep cycles` - Detect dependency cycles

**Status:** ✅ Feature-complete

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
- `-a, --assignee` - Filter by assignee
- `-p, --priority` - Filter by priority
- `-n, --limit` - Maximum issues to show (default: 10)
- `-s, --sort` - Sort policy (hybrid, priority, oldest) - default: "hybrid"

**Status:** ✅ Feature-complete

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

3. ✅ DONE - Advanced `bd list` filters:
   - ✅ `--label` (-l) - Label filtering (must have ALL specified labels)
   - ✅ `--id` - Filter by specific IDs (comma-separated)
   - ✅ `--title` - Filter by title text (case-insensitive substring)

4. ✅ DONE - **Clarify --db semantics and add --mb-beads-dir** (commit #98, 2025-10-31)
   - ✅ Add `--mb-beads-dir` as preferred minibeads-specific flag
   - ✅ Treat `--db` as syntactic sugar for upstream compatibility
   - ✅ Automatic parent directory extraction when --db points to .db file
   - ✅ Documented configuration priority order in quickstart

5. ✅ DONE - **Complete `bd create` flag parity** (2025-11-07)
   - ✅ Add `--title` flag as alternative to positional argument
   - ✅ Add `--parent` flag for hierarchical child issues
   - ✅ Add `--force` flag to allow creation with mismatched prefix
   - ✅ Add `-f, --file` flag placeholder (implementation pending)

### P2 - Medium Priority (Dependency Management)
1. ✅ DONE - Implement `bd dep remove` - Remove dependencies
2. ✅ DONE - Implement `bd dep tree` - Show dependency tree visualization
3. ✅ DONE - Implement `bd dep cycles` - Detect circular dependencies
4. ✅ DONE - Support advanced `--deps` syntax in `bd create`:
   - Format: `type:id,type:id` (e.g., `discovered-from:bd-20,blocks:bd-15`)

### P3 - Low Priority (Nice to Have)
1. `bd show --all-issues` - Show all issues
2. `bd show -p 0 -p 1` - Show by priority
3. ✅ DONE - `bd ready --sort` - Sort policy for ready work
4. ✅ DONE - `bd export` - Export issues to JSONL format
5. ✅ DONE - `bd rename` - Rename an issue ID with --dry-run and --repair
6. ✅ DONE - `bd rename-prefix` - Rename issue prefix with --dry-run and --force
7. `bd list --format` - Custom output formats (digraph, dot, templates)
8. `bd init -q` - Quiet mode
9. `bd create -f` - Bulk create from markdown file (flag added, implementation pending)

### P4 - Not Planned (Advanced Features)
- `bd import` - Import issues from JSONL format (see minibeads-11)
- `bd comments` - See minibeads-9
- `bd label` - Label management
- `bd edit` - Edit in $EDITOR
- `bd delete` - Delete issues
- `bd daemon` - Background sync daemon
- `bd compact` - Issue compaction
- `bd sync` - Git remote sync (see minibeads-12)
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

---

**Updated 2025-11-07 (commit #162+)** - `bd create` flag parity complete

All upstream `bd create` flags now implemented:
- ✅ `--title` - Alternative to positional title argument
- ✅ `--parent` - Parent issue ID for hierarchical child 
- ✅ `--force` - Force creation with mismatched prefix
- ✅ `-f, --file` - Bulk creation flag (placeholder, implementation pending)

The `bd create` command is now feature-complete for single-issue creation and fully compatible with agents using `bd create --title "..."` syntax.

---

**Previous update: 2025-10-31_#98(bea2bc8)**

All implemented commands verified against `bd --help` output and command-specific help pages. Status markers updated to reflect actual implementation state including latest --mb-beads-dir addition.

Validated CLI features at commit #98:
- ✅ Global flag improvements: --mb-beads-dir added, --db semantics clarified
- ✅ Configuration priority order: documented and tested
- ✅ Short flags: `-p, -t, -d, -a, -l` (create); `-s, -p, -l` (list); `-s, -p, -a, -d` (update); `-r` (close/reopen); `-a, -p, -n, -s` (ready); `-t` (dep add); `-d` (dep tree)
- ✅ Bulk operations: show, update, close all support multiple issue IDs
- ✅ Advanced --deps syntax: both simple (`bd-1,bd-2`) and typed (`blocks:bd-1,related:bd-2`) formats
- ✅ Advanced list filters: `--label`, `--id`, `--title` all implemented
- ✅ No default limit on `bd list` (--limit optional)
- ✅ Dependency management: dep add/remove/tree/cycles all feature-complete
- ✅ Export, rename, rename-prefix, ready --sort all feature-complete

All P0, P1, and P2 features are complete. Many P3 features also completed.

# Notes

**DIVERGENCE IDENTIFIED (2025-11-23):** minibeads currently uses hexadecimal (base16) encoding for hash IDs, while upstream bd uses base36 (0-9, a-z). This affects hash ID format and length. See minibeads-25 for implementation tracking.
