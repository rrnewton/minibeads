---
title: CardDatabase.get_card() should return references instead of cloning
status: closed
priority: 3
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T22:26:30Z"
---

# Description

Currently clones CardDefinition on every access (database_async.rs:52).
Heaptrack shows this as top allocation site.

Requires adding lifetime parameters to return &CardDefinition.
Would eliminate ~50% of Card struct clones.

## Resolution (2025-10-26)

Eliminated CardDefinition cloning by using Arc<CardDefinition> instead of direct storage.

**Implementation:**
- Changed `HashMap<String, CardDefinition>` to `HashMap<String, Arc<CardDefinition>>`
- Updated `get_card()` to return `Arc<CardDefinition>` instead of cloning
- Updated `eager_load()` to wrap loaded cards in Arc
- All call sites work seamlessly with Arc (Rust's smart pointer ergonomics)

**Why Arc instead of lifetimes:**
- CardDatabase uses `Arc<RwLock<HashMap<...>>>` for async access
- Cannot return references from async functions that hold RwLock guards
- Arc provides O(1) clone (atomic ref count increment) vs O(n) deep clone
- No API changes required at call sites - Arc derefs transparently

**Performance Impact:**
- Fresh mode: ~0.5% allocation reduction (219,818 → 218,806 bytes/game)
- Snapshot mode: Stable allocation at 119,364 bytes/game
- Eliminates deep cloning of CardDefinition structures
- Each "clone" now just increments reference count (cheap atomic operation)

**Impact Analysis:**
The modest benchmark impact is expected because CardDefinitions are primarily loaded once at startup and then used to instantiate Cards. The optimization is most valuable for:
- Repeated card lookups during deck loading
- Future features that query card database frequently
- Code clarity - Arc makes ownership semantics explicit
