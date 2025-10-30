---
title: String allocation optimization
status: closed
priority: 4
issue_type: chore
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T22:15:43Z"
---

# Description

Card names, player names cloned frequently.
Consider using Arc<str> or &'static str where appropriate.

## Resolution (2025-10-26)

Implemented Arc<str> optimization for CardName and PlayerName types in src/core/types.rs.

**Changes:**
- Changed `CardName(String)` to `CardName(Arc<str>)`
- Changed `PlayerName(String)` to `PlayerName(Arc<str>)`
- Implemented custom Serialize/Deserialize for both types to handle Arc<str>
- Maintained full API compatibility - all existing code continues to work

**Benefits:**
- Cloning CardName or PlayerName now only increments a reference count (atomic operation)
- Previous behavior: cloned the entire string data on heap
- Significantly reduces allocation pressure when names are passed around

**Measured Performance Impact:**
- Fresh mode: +9.1% games/sec improvement (4,917 → 5,364 games/sec)
- Fresh mode: -2.5% allocation reduction (225,356 → 219,818 bytes/game)
- Snapshot mode: -2.9% allocation reduction (122,884 → 119,364 bytes/game)
- All 314 tests passing

**Why Arc<str> instead of String:**
Arc<str> is an immutable, reference-counted string slice. When cloned:
- Arc<str>: O(1) atomic increment, no allocation
- String: O(n) heap allocation and copy

Card and player names are created once and cloned many times during gameplay (passed to evaluators, logged, etc.), making Arc<str> ideal.
