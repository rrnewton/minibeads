---
title: Optimization and performance tracking
status: closed
priority: 1
issue_type: epic
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-24T20:46:04-04:00"
---

# Description

Track performance optimization work for MTG Forge Rust.

**Current performance as of commit#179(b6bee0df):**

*Fresh Mode (seed 42):*
- Games/sec: ~6,214
- Actions/sec: ~7,960,611
- Turns/sec: ~546,864
- Actions/turn: 14.56
- Avg bytes/game: 267,082
- Bytes/turn: 3,035
- Avg duration/game: 160.92µs

*Snapshot Mode (seed 42):*
- Games/sec: ~7,726
- Actions/sec: ~9,896,970
- Turns/sec: ~679,885
- Actions/turn: 14.56
- Avg bytes/game: 164,610
- Bytes/turn: 1,870
- Avg duration/game: 129.43µs

**Completed optimizations:**
- ✓ mtg-6: Logging allocations (conditional compilation)
- ✓ mtg-10: Vec reallocations (fixed arrays + SmallVec)

**High priority open issues:**
- mtg-7: CardDatabase.get_card() should return references
- mtg-8: Eliminate GameStateView clones
- mtg-9: String allocation optimization

**Medium priority:**
- mtg-11: Zone transfer operations optimization
- mtg-12: Mana pool calculation optimization

**Future considerations:**
- mtg-13: Arena allocation for per-turn temporaries
- mtg-14: Object pools for reusable objects
- mtg-15: Compile-time feature flags for profiling modes

See OPTIMIZATION.md for detailed analysis and profiling methodology.

Checked up-to-date as of 2025-10-24.
