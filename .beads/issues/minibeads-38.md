---
title: 'Git merge driver: three-way merge of issue and comment files'
status: in_progress
priority: 3
issue_type: feature
created_at: 2026-10-03T06:41:11.280680550+00:00
updated_at: 2026-10-03T06:41:42.476148877+00:00
---

# Description

Owner goal (2026-10-02): before moving hermit's syscall-tracking issues into version control as mb files, mb needs good 3-way merge conflict resolution including comments, for git-level merges as well as GitHub sync (PR #26 covered only GitHub title/body/state).

Delivered in 0.29.0 (branch mb-3way/finish, supersedes https://github.com/rrnewton/minibeads/pull/26):
- src/issue_merge.rs: whole-issue 3-way merge (every scalar field, label set, per-target dependencies, timestamp rules, all four prose sections via prose_merge) and an append-only, ID-keyed comment-set merge (never dropped, never duplicated; one-sided deletions kept with a NOTICE).
- src/diff3.rs: line diff3 with fixed ours/base/theirs labels; conflicts are rendered from three views so hunks cover only the conflicting field/paragraph/comment.
- src/merge_driver.rs: mb merge-driver run|install|show (git %O %A %B %L %P protocol, git config + .gitattributes).
- Fixed nondeterministic depends_on ordering (HashMap -> BTreeMap) and a prose_merge duplication bug across paragraphs; mb refuses to read/write files containing unresolved conflict hunks.
- Tests: explicit cases + 2500-case seeded properties (commutativity, identity, re-merge idempotence, machine-resolvable hunks, no lost/duplicated comments) + tests/merge_driver.sh with real git merges; mutation-checked (8 deliberate breakages all caught).
Design: ai_docs/merge-driver-design.md.
