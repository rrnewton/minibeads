---
title: 'Bidirectional upstream sync stress test fails: issue count mismatch (expected 45, got 30)'
status: open
priority: 2
issue_type: bug
created_at: 2026-07-07T01:37:30.231222746+00:00
updated_at: 2026-07-07T01:37:30.231222746+00:00
---

# Description

`make stress-test` (cargo test --test random_minibeads) fails deterministically in `test_sync_stress` with seed 12345, 10 cycles, 15 actions/phase:

```
❌ Issue count mismatch!
Error: Issue count mismatch: expected 45, got 30
```

## Scope / confirmation
- PRE-EXISTING: reproduced identically at commit af00710 (Release 0.21.6), before the comment-deletion-propagation work and before `mb update --append`. Same expected 45 / got 30. So it is NOT a regression from those changes.
- NOT covered by `make validate` (which runs `cargo test --lib --bins --test e2e_tests` and passes clean). Only `make stress-test` exercises random_minibeads, so this rots undetected.

## Next steps
- Determine whether the bug is in the bidirectional jsonl/markdown sync path (test_minibeads sync-test harness) or in the test's expected-count bookkeeping.
- The harness invokes `test_minibeads sync-test --seed 12345 --cycles 10 --actions-per-phase 15`; deterministic seed makes it reproducible for bisection.
- Consider wiring a fast, small-seed variant into make validate/CI once fixed, so sync regressions are caught.

Tracked under testing/validation (minibeads-3).
