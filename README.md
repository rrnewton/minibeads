# minibeads

A minimal, markdown-based drop-in replacement for [steveyegge/beads](https://github.com/steveyegge/beads) written in Rust.

## Overview

minibeads (`mb`) is a dependency-aware issue tracker designed for AI agent workflows. Issues are stored as markdown files with YAML frontmatter, making them both human-readable and git-friendly. The tool emphasizes simplicity, with no database required—just markdown files in `.minibeads/issues/`.

### Key Features

- **Markdown-only storage**: No SQLite, no JSONL—just `.md` files
- **Dependency tracking**: Issues can block each other, with automatic detection of ready work
- **AI-friendly**: Full MCP (Model Context Protocol) integration for AI agents
- **Fast**: Rust implementation with coarse-grained file locking
- **Drop-in replacement**: Compatible with upstream beads MCP server (https://github.com/steveyegge/beads)

## Installation

### From crates.io

```bash
cargo install minibeads
```

This installs the `mb` binary (short for "minibeads"). To use minibeads as a
drop-in replacement for upstream [beads](https://github.com/steveyegge/beads)
(e.g., with the beads MCP server), alias or symlink it to `bd`:

```bash
# Symlink approach
ln -s $(which mb) ~/.local/bin/bd

# Or shell alias
alias bd=mb
```

### Build from source

```bash
# Clone the repository
git clone https://github.com/rrnewton/minibeads.git
cd minibeads

# Build in debug mode (recommended for development)
make build

# Or build release version
make release

# Install to ~/.local/bin
make install
```

The binary will be named `mb` (short for "minibeads").

## Quick Start

```bash
# Initialize a beads database in your project
mb init

# Create your first issue
mb create "Fix login bug" -p 1 -t bug

# List all issues
mb list

# Show issue details
mb show mb-1

# Update issue status
mb update mb-1 --status in_progress

# Add dependencies (mb-2 blocks mb-1)
mb dep add mb-1 mb-2

# Find ready work (no blockers)
mb ready

# Get statistics
mb stats
```

Run `mb quickstart` for a comprehensive guide.

## Storage Format

minibeads stores all data in `.minibeads/issues/` as markdown files. Existing
projects with `.beads/` continue to work as a legacy fallback until you move the
directory.

```
.minibeads/
├── config.yaml           # Contains issue-prefix
├── .gitignore           # Auto-managed (minibeads.lock, command_history.log)
├── issues/
│   ├── myproject-1.md   # Issue files with YAML frontmatter
│   └── myproject-2.md
├── comments/            # Optional per-issue comment JSON files
└── github-sync-state.json # Last-synced GitHub ancestry state, when used
```

### Issue Format

Each issue is a markdown file with YAML frontmatter:

```markdown
---
title: Fix authentication bug
status: in_progress
priority: 1
issue_type: bug
assignee: alice
depends_on:
  myproject-5: blocks
created_at: 2025-10-30T10:00:00Z
updated_at: 2025-10-30T11:00:00Z
---

# Description

User sessions expire too quickly. Need to extend timeout to 24 hours.

# Design

Update session middleware to use configurable timeout from environment variable.

# Acceptance Criteria

- [ ] Session timeout configurable via SESS_TIMEOUT env var
- [ ] Default remains 1 hour if not set
- [ ] Tests pass for various timeout values
```

## MCP Integration

minibeads works seamlessly with AI agents via the beads MCP server. Agents can:

- Create, update, and close issues
- Query dependencies and find ready work
- Track progress across sessions using the markdown history

Set `BEADS_DB` or `MB_BEADS_DIR` environment variables, or let the MCP server auto-discover `.minibeads/` in your project.

## Development

### Running Tests

```bash
# Run all tests (unit + e2e)
make test

# Run full validation (Rust tests + e2e + fmt + clippy)
make validate

# Run longer randomized/stress suites
make stress-test

# Format code
make fmt
```

### Project Structure

```
src/
├── main.rs      # CLI entry point and command handlers
├── storage.rs   # File-based storage operations
├── format.rs    # Markdown serialization/deserialization
├── types.rs     # Core data structures (Issue, Status, etc.)
└── lock.rs      # Coarse-grained file locking

tests/
├── e2e_tests.rs           # Test harness
└── basic_operations.sh    # Shell-based e2e tests
```

## Design Philosophy

### Why Markdown?

- **Human-readable**: Issues are plain text files you can read, edit, and grep
- **Git-friendly**: Diffs, merges, and history work naturally
- **Simple**: No schema migrations, no database corruption, no SQL
- **Portable**: Copy `.minibeads/` anywhere, it just works

### Why Rust?

- **Performance**: Faster than Go for file operations
- **Safety**: No null pointers, no data races
- **Zero-copy**: Minimize allocations (see CLAUDE.md for patterns)
- **Small binary**: Single ~23MB binary with no runtime dependencies

### Locking Strategy

minibeads uses coarse-grained locking with `.minibeads/minibeads.lock` containing the process PID. Operations use exponential backoff (up to 5 seconds) when lock is held. This is simpler than upstream's per-issue locking and sufficient for AI agent workflows.

## Command Reference

### Core Commands

- `mb init [--prefix PREFIX]` - Initialize beads database
- `mb create TITLE [OPTIONS]` - Create new issue
- `mb list [FILTERS]` - List issues with optional filters
- `mb show ISSUE_ID` - Show detailed issue information
- `mb update ISSUE_ID [OPTIONS]` - Update issue fields
  - `--search TEXT --replace TEXT [--field FIELD] [--replace-all]` - targeted,
    aider-style edit of a text field (default `description`) instead of
    overwriting it wholesale. By default the search text must match exactly once;
    a missing or ambiguous match is an error and the issue is left untouched.
    This is the recommended way for agents to revise a long description — far
    safer than rewriting the whole field. (minibeads-specific)
  - `--append TEXT [--field FIELD]` - append `TEXT` to the end of a text field
    (default `description`), inserting a blank line before it when the field is
    non-empty so it becomes its own paragraph. Simpler than a search/replace when
    you only want to add to the end. (minibeads-specific)
- `mb close ISSUE_ID [--reason REASON]` - Close (complete) an issue
- `mb reopen ISSUE_ID...` - Reopen closed issues
- `mb comments add ISSUE_ID --body TEXT` - Add a local issue comment
- `mb comments list ISSUE_ID` - List local issue comments
- `mb comments delete ISSUE_ID COMMENT_ID...` - Delete local issue comment(s) by ID (minibeads-specific)

### Dependencies

- `mb dep add FROM TO [--type TYPE]` - Add dependency
  - Types: `blocks` (default), `related`, `parent-child`, `discovered-from`

### Queries

- `mb ready [--assignee USER] [--priority N]` - Find ready work (no blockers)
- `mb blocked` - Show blocked issues and what blocks them
- `mb stats` - Show statistics (total, open, blocked, average lead time)
- `mb list --github` - Show only issues linked to GitHub Issues

### GitHub Issues Sync

minibeads can sync a subset of issues with GitHub Issues using the authenticated
`gh` CLI. Linked issues store the GitHub issue URL in `external_ref`; unlinked
issues are ignored.

- `mb github link ISSUE_ID GITHUB_ISSUE [-R owner/repo]` - Link to an existing GitHub issue
- `mb github list` - Show current minibeads-to-GitHub issue links
- `mb github import [-R owner/repo] [--state open|closed|all] [--label LABEL] [--assignee USER] [--author USER] [--mention USER] [--milestone M] [--app APP] [--search QUERY] [--limit N] [--dry-run] [--quiet|--verbose]` - Import matching GitHub issues that are not already linked to minibeads issues
- `mb github publish ISSUE_ID [-R owner/repo]` - Create a GitHub issue and link it
- `mb github sync [ISSUE_ID...] [-R owner/repo] [--dry-run] [--quiet|--verbose]` - Bidirectionally sync linked issues
- `mb github stress-test -R owner/repo [-n N] [--steps N] [--seed N] [--adversarial] [--verbose]` - Create real temporary GitHub issues in a disposable repo and run seeded randomized sync stress tests

Synced fields are title, description/body, open/closed state, and comments.
minibeads keeps `.minibeads/github-sync-state.json` as the last-synced ancestry
record so it can distinguish local-only changes, GitHub-only changes, and
both-sides conflicts. Labels, priority, assignee, dependencies, and other
minibeads-specific metadata remain local for now.

Comment sync propagates deletions in both directions. The sync state pairs each
synced local comment with its GitHub comment id, so deleting a comment on one
side (for example with `mb comments delete`) deletes its counterpart on the
other side on the next sync, rather than re-importing it. Pull-only sync
(`--pull-only`) applies GitHub-side deletions locally but never deletes on
GitHub.

Linked GitHub issues get a marker comment containing `MB_DO_NOT_SYNC` so people
viewing the GitHub issue can see which local minibeads issue owns the sync. That
marker comment is ignored by comment sync and is not imported into minibeads.

`mb github import` only creates local issues for GitHub issues whose URL is not
already present in any local issue's `external_ref`; already linked issues remain
the responsibility of `mb github sync`.

By default, `mb github sync` prints one informative line per linked issue plus a
summary. Use `--quiet` for only the summary line, or `--verbose` to include
field/comment details under each issue and print each underlying `gh` CLI call
with elapsed time to stderr.

Design note: upstream Beads has an `external_ref` field and import/collision
logic around it, but does not provide this exact GitHub sync workflow in the
vendored version. minibeads uses the same `external_ref` idea for the URL and
keeps the sync ancestry outside the issue markdown to avoid churning normal
issue fields.

### Options

- `--db PATH` - Path to .minibeads directory
- `--json` - Output JSON format
- `--mb-validation MODE` - Validation mode: silent, warn, error (default) [minibeads-specific]
- `--mb-no-cmd-logging` - Disable command history logging [minibeads-specific]

## Differences from Upstream Beads

### What's the Same

- All MCP operations work identically
- Command-line interface is compatible
- Issue semantics (status, priority, dependencies)

### What's Different

- **No SQLite database**: Markdown is the only storage
- **No issues.jsonl**: Markdown files are the source of truth
- **Simpler locking**: Coarse-grained lock instead of per-issue locks
- **File-backed comments**: Comments are stored separately from issue markdown
- **Better markdown sanitization**: Auto-escapes section headers in user content

### Migration Path

To export for upstream beads compatibility, use `mb export` (planned in minibeads-11) to generate `issues.jsonl`. Bidirectional sync (minibeads-12) will enable hybrid workflows.

## Roadmap

See `.minibeads/issues/` for tracking:

- **minibeads-9**: Comments data model
- **minibeads-10**: Events/audit trail
- **minibeads-11**: Export to issues.jsonl format
- **minibeads-12**: Bidirectional jsonl/markdown sync
- **minibeads-7**: Colorful CLI output

## Environment Variables

- `MB_BEADS_DIR` - Path to .minibeads directory [minibeads-specific]
- `BEADS_DB` - Path to .beads database (supports `.db` extension for compatibility)
- `BEADS_WORKING_DIR` - Working directory for MCP operations

## Contributing

1. Read `CLAUDE.md` for coding conventions (strong types, zero-copy patterns)
2. Read `OPTIMIZATION.md` for performance guidelines
3. Use `make validate` before committing
4. Follow commit message format (see recent commits)

## License

MIT License. See [LICENSE](LICENSE) for details.

## Links

- Upstream beads: https://github.com/steveyegge/beads
- MCP specification: https://modelcontextprotocol.io
