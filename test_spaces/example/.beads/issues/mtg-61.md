---
title: Document the mana engine API
status: closed
priority: 4
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T22:21:08Z"
---

# Description

The ManaEngine is complex and needs better documentation.

Add:
- Module-level docs explaining the architecture
- Examples of common use cases
- Diagram showing interaction with GameState
- Performance characteristics and optimization notes

Target audience: contributors implementing new card effects.

## Resolution (2025-10-26)

Enhanced the mana_engine.rs module documentation with comprehensive coverage:

**Added:**
- **Architecture section**: Two-phase operation (Update/Query), mana source classification
- **Performance characteristics**: Big-O complexity analysis for update and query operations
- **Integration with GameState**: Explained read-only cache layer design
- **Usage examples**: Three complete examples:
  1. Basic spell castability checking
  2. AI controller integration pattern
  3. Engine maintenance across game actions
- **Future enhancements**: Documented planned improvements (complex sources, creature mana, etc.)

**Documentation Coverage:**
- Module-level docs: ~100 lines of detailed explanation
- Performance notes: O(1) query, O(n) update complexity documented
- Code examples: 3 practical usage patterns with inline comments
- Design rationale: Separation of concerns between cache and game state

The documentation now provides clear guidance for contributors implementing card effects that need to check mana availability.
