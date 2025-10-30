---
title: 'Cross-cutting codebase issues: APIs, testing, architecture'
status: open
priority: 1
issue_type: epic
labels:
  - tracking
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
---

# Description

Track architectural improvements, API design, and testing infrastructure.

**Controller architecture:**
- Current: Unified PlayerController trait (documented in ai_docs/CONTROLLER_DESIGN.md)
  - Single `choose_spell_ability_to_play()` method for lands, spells, and abilities
  - GameStateView provides read-only access with zero-copy patterns
  - Callback-based casting with proper mana timing (step 6 of 8)
- Implementations:
  - RandomController: Random decisions with seeded RNG
  - ZeroController: Always chooses first option (deterministic)
  - HeuristicController: Evaluation-based AI (faithful Java port)
  - FixedScriptController: Script-based decisions for testing
  - InteractiveController: Human player via stdin/stdout
- mtg-40: Migrate game loop from v1 to v2 controller interface (OBSOLETE - already unified)
- mtg-41: Controller API consistency and documentation

**Testing infrastructure:**
- Current: 360 passing tests (nextest, all categories)
- mtg-42: Improve test coverage for edge cases
- mtg-43: Integration test suite expansion
- mtg-44: Determinism testing for more complex scenarios
- mtg-45: Property-based testing with proptest

**Performance & Tree Search (Phase 4):**
- mtg-46: Undo/redo performance testing
- mtg-47: Board state evaluation function
- mtg-48: Tree search using undo log
- mtg-49: MCTS or minimax search implementation
- mtg-50: Measure boardstates-per-second

**Serialization:**
- mtg-51: Fast binary game snapshots (rkyv)
- mtg-52: Parallel game search capabilities
- mtg-53: SIMD optimizations where applicable

---
**Checked up-to-date as of 2025-10-27_#381(9fea5cda)**
- Verified controller architecture (5 implementations, unified interface)
- Updated test count: 312 → 360 tests
- Marked mtg-40 as obsolete (v1/v2 already unified)
- Verified file locations (ai_docs/CONTROLLER_DESIGN.md exists)
- All controller implementations working correctly
