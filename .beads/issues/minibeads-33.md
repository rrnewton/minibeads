---
title: Add adversarial multi-issue GitHub sync stress tests
status: open
priority: 2
issue_type: feature
created_at: 2026-06-27T18:03:40.219979034+00:00
updated_at: 2026-06-27T18:03:40.219979034+00:00
---

# Description

Current `mb github stress-test` is isolated per issue: it mutates one linked issue through a sequence of steps, syncs and verifies after each step, then moves to the next issue. That is useful but not adversarial enough.

Add a more aggressive live-GitHub stress mode against a disposable repo that:
- creates a batch of linked temporary GitHub issues
- applies many random local and GitHub mutations across the whole batch before syncing
- deliberately creates both-side changes on the same issue to exercise conflict detection
- syncs the batch as a unit, then verifies one-sided changes converge while both-sided field edits remain conflicts
- verifies comments still append-sync exactly once and marker comments are ignored
- verifies a second no-op sync does not silently resolve or corrupt conflicts
- optionally injects remote changes between fetch and mutation to model stale-write races

Also decide and test the conflict-resolution workflow. Current conflict handling should not record a divergent issue as clean ancestry. Likely follow-up commands: `mb github resolve ISSUE --take local` and `--take remote`; manual merge can be local edit followed by `--take local`.

Do not implement concurrent sync as part of this issue unless separately approved by a human.
