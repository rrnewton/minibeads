---
title: Performance optimization tracking
status: open
priority: 1
issue_type: epic
created_at: 2025-10-30T13:22:12.980878549+00:00
updated_at: 2025-10-30T13:22:12.980878549+00:00
---

# Description

Track performance optimization work following PROJECT_VISION conventions.

## Principles (from CLAUDE.md)
- Prefer strong types over primitives
- Avoid clone: use references and manage lifetimes
- Avoid collect: use iterators
- Zero-copy where possible
- Safe Rust only (no unsafe without very good reason)

## Areas to Review
- [ ] Storage layer: reduce allocations in list_issues
- [ ] Format layer: optimize markdown parsing
- [ ] Lock implementation: review efficiency
- [ ] Type system: ensure zero-cost abstractions

## Related Issues
TBD as granular tasks are filed
