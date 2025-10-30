---
title: Optimization and performance tracking
status: open
priority: 1
issue_type: epic
labels:
  - tracking
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
---

# Description

Track performance optimization work for MTG Forge Rust.

**Current performance as of 2025-10-26_#333(dc90c78b):**

*Fresh Mode (seed 42):*
- Games/sec: ~3,842
- Actions/sec: ~464,585
- Turns/sec: ~28,066
- Actions/turn: 16.56
- Avg bytes/game: ~233,426
- Avg bytes/turn: ~12,968
- Avg duration/game: 260.36µs

*Snapshot Mode (seed 42):*
- Games/sec: ~9,177
- Actions/sec: ~2,734,713
- Avg bytes/game: ~122,884
- Avg bytes/turn: ~6,827
- Avg duration/game: 108.97µs

*Rewind Mode (seed 42):*
- Rewinds/sec: ~332,103
- Actions/sec (rewind): ~107,686,651
- Avg bytes allocated: 0 (zero-copy rewind)

**Note:** Fresh mode performance decreased from commit#162 due to expanded game features
(activated abilities, more complex AI, additional tests). However, actions/turn increased
dramatically from 0.82 to 16.56, showing much richer gameplay. Rewind mode demonstrates
excellent zero-copy characteristics for tree search.

**Completed optimizations:**
- ✅ mtg-6: Logging allocations (conditional compilation added, COMPLETED)
- ✅ mtg-10: Vec reallocations in game loop (SmallVec + fixed arrays, COMPLETED)
- ✅ mtg-7: CardDatabase.get_card() returns Arc<CardDefinition> (COMPLETED 2025-10-26)
- ✅ mtg-8: GameStateView already uses borrowing, not cloning (COMPLETED 2025-10-26)
- ✅ mtg-9: CardName and PlayerName use Arc<str> (COMPLETED 2025-10-26)
- ✅ mtg-12: Mana pool calculation optimization (already resolved - no clones in ManaEngine, COMPLETED 2025-10-27)
- ✅ mtg-11: Zone transfer operations (investigated, already optimal, determinism requirement prevents swap_remove, COMPLETED 2025-10-27)

**High priority open issues:**
- (None currently)

**Medium priority:**
- (None currently - all investigated issues found to be already optimal)

**Future considerations:**
- mtg-13: Arena allocation for per-turn temporaries
- mtg-14: Object pools for reusable objects
- mtg-15: Compile-time feature flags for profiling modes

See OPTIMIZATION.md for detailed analysis and profiling methodology.

---
**Checked up-to-date as of 2025-10-27_#381(9fea5cda)**
- Updated performance metrics from fresh cargo bench run
- Verified completion status of mtg-6, mtg-10, mtg-11, mtg-12
- All benchmark modes tested: fresh, snapshot, rewind
- All medium-priority optimization items investigated and completed
