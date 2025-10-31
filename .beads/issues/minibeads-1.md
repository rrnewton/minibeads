---
title: 'minibeads: Overall project tracking'
status: open
priority: 0
issue_type: epic
assignee: claude
created_at: 2025-10-30T13:22:12.679166235+00:00
updated_at: 2025-10-31T04:26:26.036565348+00:00
---

# Description

Main tracking issue for minibeads - a minimal, filesystem-based issue tracker in Rust.

## Status (as of 2025-10-31_#71(0692919))
- ✅ Core implementation complete (storage, types, CLI, locking)
- ✅ MCP integration working (dependencies/dependents fixed in minibeads-13)
- ✅ Feature parity largely achieved (see minibeads-15)
  - All P0, P1, P2 features complete
  - Many P3 features complete (export, rename, rename-prefix, ready --sort)
- ✅ CI/CD pipeline operational (Linux, macOS, Windows)
- ✅ Performance optimizations completed (see minibeads-4)
  - Iterator-based blocking dependency checks
  - Zero-copy populate_dependents
- 🔲 Documentation improvements in progress (minibeads-2)
- 🔲 Additional testing coverage needed (minibeads-3, minibeads-14)

## Project Stats (2025-10-31)
- Total issues: 17 (6 closed, 9 open, 2 in progress)
- Ready work: 8 issues
- Average lead time: 3.8 hours
- All validation checks passing (unit tests, e2e tests, fmt, clippy)

## Active Tracking Issues
- minibeads-2: Documentation and examples (in_progress)
- minibeads-3: Testing and validation (in_progress)
- minibeads-4: Performance optimization (open - validated 2025-10-31)
- minibeads-15: Feature completeness vs upstream bd (open - validated 2025-10-31)

## Recent Completions
- minibeads-18: bd rename-prefix command
- minibeads-17: bd rename operation
- minibeads-13: MCP integration bug fixes
- minibeads-11: bd export command
- minibeads-8: CI pipeline setup
- minibeads-5: Status enum serialization fixes

## Architectural Notes
- minibeads-12, minibeads-16: Sync issues marked with architectural mismatch (2025-10-31)
  - Clarified that markdown is the ONLY source of truth
  - issues.jsonl is export-only, not part of core storage
  - No bidirectional sync needed

## Project Conventions
- Use `bd` CLI or MCP tools to manage issues
- Keep all content in description field (not notes)
- Reference issues in code TODOs: `// TODO(minibeads-N): description`
- Track transient info with timestamps: YYYY-MM-DD_#DEPTH(hash)
- Validate tracking issues periodically and add timestamps

---

**Checked up-to-date as of 2025-10-31_#71(0692919)**

All stats verified, architecture clarifications added for sync issues.
