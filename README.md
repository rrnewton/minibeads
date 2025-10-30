# minibeads

A minimal, markdown-based drop-in replacement for [steveyegge/beads](https://github.com/steveyegge/beads) written in Rust.

## Overview

minibeads (`bd`) is a dependency-aware issue tracker designed for AI agent workflows. Issues are stored as markdown files with YAML frontmatter, making them both human-readable and git-friendly. The tool emphasizes simplicity, with no database required—just markdown files in `.beads/issues/`.

### Key Features

- **Markdown-only storage**: No SQLite, no JSONL—just `.md` files
- **Dependency tracking**: Issues can block each other, with automatic detection of ready work
- **AI-friendly**: Full MCP (Model Context Protocol) integration for AI agents
- **Fast**: Rust implementation with coarse-grained file locking
- **Drop-in replacement**: Compatible with upstream beads MCP server (https://github.com/steveyegge/beads)

## Installation

### Build from source

```bash
# Clone the repository
git clone https://github.com/yourusername/minibeads.git
cd minibeads

# Build in debug mode (recommended for development)
make build

# Or build release version
make release

# Install to ~/.local/bin
make install
```

The binary will be named `bd` (short for "beads").

## Quick Start

```bash
# Initialize a beads database in your project
bd init

# Create your first issue
bd create "Fix login bug" -p 1 -t bug

# List all issues
bd list

# Show issue details
bd show bd-1

# Update issue status
bd update bd-1 --status in_progress

# Add dependencies (bd-2 blocks bd-1)
bd dep add bd-1 bd-2

# Find ready work (no blockers)
bd ready

# Get statistics
bd stats
```

Run `bd quickstart` for a comprehensive guide.

## Storage Format

minibeads stores all data in `.beads/issues/` as markdown files:

```
.beads/
├── config.yaml           # Contains issue-prefix
├── .gitignore           # Auto-managed (minibeads.lock, command_history.log)
└── issues/
    ├── myproject-1.md   # Issue files with YAML frontmatter
    └── myproject-2.md
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

Set `BEADS_DB` or `MB_BEADS_DIR` environment variables, or let the MCP server auto-discover `.beads/` in your project.

## Development

### Running Tests

```bash
# Run all tests (unit + e2e)
make test

# Run full validation (test + fmt + clippy)
make validate

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
- **Portable**: Copy `.beads/` anywhere, it just works

### Why Rust?

- **Performance**: Faster than Go for file operations
- **Safety**: No null pointers, no data races
- **Zero-copy**: Minimize allocations (see CLAUDE.md for patterns)
- **Small binary**: Single ~23MB binary with no runtime dependencies

### Locking Strategy

minibeads uses coarse-grained locking with `.beads/minibeads.lock` containing the process PID. Operations use exponential backoff (up to 5 seconds) when lock is held. This is simpler than upstream's per-issue locking and sufficient for AI agent workflows.

## Command Reference

### Core Commands

- `bd init [--prefix PREFIX]` - Initialize beads database
- `bd create TITLE [OPTIONS]` - Create new issue
- `bd list [FILTERS]` - List issues with optional filters
- `bd show ISSUE_ID` - Show detailed issue information
- `bd update ISSUE_ID [OPTIONS]` - Update issue fields
- `bd close ISSUE_ID [--reason REASON]` - Close (complete) an issue
- `bd reopen ISSUE_ID...` - Reopen closed issues

### Dependencies

- `bd dep add FROM TO [--type TYPE]` - Add dependency
  - Types: `blocks` (default), `related`, `parent-child`, `discovered-from`

### Queries

- `bd ready [--assignee USER] [--priority N]` - Find ready work (no blockers)
- `bd blocked` - Show blocked issues and what blocks them
- `bd stats` - Show statistics (total, open, blocked, average lead time)

### Options

- `--db PATH` - Path to .beads directory
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
- **No comments/events yet**: Planned for future releases (see minibeads-9, minibeads-10)
- **Better markdown sanitization**: Auto-escapes section headers in user content

### Migration Path

To export for upstream beads compatibility, use `bd export` (planned in minibeads-11) to generate `issues.jsonl`. Bidirectional sync (minibeads-12) will enable hybrid workflows.

## Roadmap

See `.beads/issues/` for tracking:

- **minibeads-9**: Comments data model
- **minibeads-10**: Events/audit trail
- **minibeads-11**: Export to issues.jsonl format
- **minibeads-12**: Bidirectional jsonl/markdown sync
- **minibeads-7**: Colorful CLI output

## Environment Variables

- `MB_BEADS_DIR` - Path to .beads directory [minibeads-specific]
- `BEADS_DB` - Path to .beads database (supports `.db` extension for compatibility)
- `BEADS_WORKING_DIR` - Working directory for MCP operations

## Contributing

1. Read `CLAUDE.md` for coding conventions (strong types, zero-copy patterns)
2. Read `OPTIMIZATION.md` for performance guidelines
3. Use `make validate` before committing
4. Follow commit message format (see recent commits)

## License

[License TBD - check upstream beads for guidance]

## Links

- Upstream beads: https://github.com/steveyegge/beads
- MCP specification: https://modelcontextprotocol.io
