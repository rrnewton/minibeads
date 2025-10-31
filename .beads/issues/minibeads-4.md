---
title: Performance optimization tracking
status: open
priority: 1
issue_type: epic
created_at: 2025-10-30T13:22:12.980878549+00:00
updated_at: 2025-10-31T03:08:20.499789657+00:00
---

# Description

Track performance optimization work following PROJECT_VISION conventions.

## Principles (from CLAUDE.md)
- Prefer strong types over primitives
- Avoid clone: use references and manage lifetimes
- Avoid collect: use iterators
- Zero-copy where possible
- Safe Rust only (no unsafe without very good reason)

## Completed Optimizations

### 1. Eliminated double-collect in get_blocking_dependencies (2025-10-31)
**Problem**: get_blocking_dependencies() was returning Vec<&String>, which required:
1. First collect() to create the Vec
2. Second collect() when converting &String to String in callers

**Solution**: Changed to return impl Iterator, allowing single-pass operations:
- get_blocking_dependencies() now returns iterator (zero allocations)
- Added has_blocking_dependencies() for O(1) existence checks
- Callers use .cloned().collect() for single-pass Vec creation

**Impact**:
- Reduced allocations: 2 collections → 1 collection
- Better performance in bd stats, bd blocked, bd ready commands
- Zero-cost checks with has_blocking_dependencies()

## Areas to Review
- [ ] Storage layer: further reduce allocations in list_issues
- [ ] Format layer: optimize markdown parsing
- [ ] Lock implementation: review efficiency
- [ ] Type system: ensure zero-cost abstractions

## Related Issues
TBD as granular tasks are filed
