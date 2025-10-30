---
title: 'minibeads: Overall project tracking'
status: open
priority: 0
issue_type: epic
assignee: claude
created_at: 2025-10-30T13:22:12.679166235+00:00
updated_at: 2025-10-30T13:22:12.679166235+00:00
---

# Description

Main tracking issue for minibeads - a minimal, filesystem-based issue tracker in Rust.

## Status
- ✅ Core implementation complete (storage, types, CLI, locking)
- ✅ Basic MCP integration working
- 🔲 Documentation and testing improvements needed
- 🔲 Performance optimization pass

## Active Tracking Issues
- minibeads-2: Documentation and examples
- minibeads-3: Testing and validation
- minibeads-4: Performance optimization

## Project Conventions
- Use `bd` CLI or MCP tools to manage issues
- Keep all content in description field (not notes)
- Reference issues in code TODOs: `// TODO(minibeads-N): description`
- Track transient info with timestamps: YYYY-MM-DD_#DEPTH(hash)
