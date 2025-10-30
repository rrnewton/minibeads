---
title: Improve test coverage for edge cases
status: open
priority: 4
issue_type: chore
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-27T19:30:00Z"
---

# Description

Expand unit tests to cover edge cases:
- Interaction between multiple keywords
- Complex combat scenarios
- Stack interaction corner cases
- Zone transfer edge cases
Target >80% code coverage.

## Recent Work (2025-10-27, commit 2cc1f4ad)

Added keyword interaction tests in `tests/keyword_interactions_test.rs`:
- First strike + trample interaction (MTG Rules 510.1c, 702.19b)
- Double strike + trample interaction (MTG Rules 702.4b, 702.19c)
- Deathtouch + trample interaction (MTG Rules 702.2c, 702.19c)
- Lifelink + trample interaction (MTG Rules 702.15b)
- Flying + reach interaction (MTG Rules 702.9c, 702.17b)

These are documentation tests that specify expected behavior with comprehensive rules citations. They serve as specifications for future implementation of full combat keyword interactions.

Test count increased from 360 to 365 tests (all passing).
