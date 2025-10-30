---
title: 'Logging allocations are #1 hotspot (70%+ of allocations)'
status: closed
priority: 3
issue_type: chore
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-23T09:52:38Z"
---

# Description

String formatting in logging dominates allocations. 
- 77,378 calls in Combat.clear() (src/game/game_loop.rs:819)
- 45,274 calls in draw card logging (src/game/game_loop.rs:517) 
- 43,437 calls in discard logging (src/game/game_loop.rs:863)

Solutions to consider:
1. Use tracing crate with zero-cost disabled spans
2. Implement string interning for repeated messages
3. Add compile-time feature flag to disable logging
4. Use Cow<'static, str> for common log messages
5. Pre-allocate string buffers and reuse them

Discovered from heap profiling at commit#162(387498cecf).
