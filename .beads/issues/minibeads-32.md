---
title: Plan optional concurrent GitHub sync
status: open
priority: 3
issue_type: feature
created_at: 2026-06-27T14:06:32.864566631+00:00
updated_at: 2026-06-27T14:06:32.864566631+00:00
---

# Description

GitHub sync currently runs gh CLI operations sequentially. If sync starts taking too long in real use, design and implement concurrency only after explicit human approval.

Future plan should cover:
- concurrency limits and GitHub API/CLI rate-limit behavior
- preserving deterministic conflict handling and ancestry-state writes
- avoiding concurrent writes to the same local issue or sync-state file
- keeping verbose gh timing output readable
- stress tests that compare sequential and concurrent behavior against the sync model

Do not enable concurrent GitHub sync by default or as an opportunistic refactor without human approval.
