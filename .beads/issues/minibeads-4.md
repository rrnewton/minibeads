---
title: Performance optimization tracking
status: open
priority: 1
issue_type: epic
created_at: 2025-10-30T13:22:12.980878549+00:00
updated_at: 2025-10-31T03:23:02.728009815+00:00
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

### 2. Eliminated unnecessary clone in populate_dependents (2025-10-31)
**Problem**: populate_dependents() was cloning Vec<Dependency> for each issue:
- Built HashMap<String, Vec<Dependency>> of reverse dependencies
- Used .get() and .clone() to copy vectors into each issue
- Resulted in unnecessary allocations for every issue in list operations

**Solution**: Take ownership from HashMap instead of cloning (src/storage.rs:782-784):
```rust
// Before: issue.dependents = dependents.clone();
// After:  issue.dependents = reverse_deps.remove(&issue.id).unwrap_or_default();
```

**Impact**:
- Eliminated 1 Vec clone per issue in all list operations
- Affects: bd list, bd stats, bd blocked, bd ready
- Zero-copy transfer of ownership from HashMap to Issue

## Areas to Review
- [ ] Storage layer: further reduce allocations in list_issues
- [ ] Format layer: optimize markdown parsing
- [ ] Lock implementation: review efficiency
- [ ] Type system: ensure zero-cost abstractions

## Related Issues
TBD as granular tasks are filed

---

**Checked up-to-date as of 2025-10-31_#66(29b9753)**

All code references validated:
- src/types.rs:235 - get_blocking_dependencies() returns impl Iterator ✓
- src/types.rs:243 - has_blocking_dependencies() for O(1) checks ✓
- src/storage.rs:782-784 - populate_dependents() uses remove() instead of clone() ✓
