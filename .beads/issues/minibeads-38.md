---
title: 'Git merge driver: three-way merge of issue and comment files'
status: in_progress
priority: 3
issue_type: feature
created_at: 2026-10-03T06:41:11.280680550+00:00
updated_at: 2026-10-03T07:22:02.439787572+00:00
---

# Description

Owner goal (2026-10-02): before moving hermit's syscall-tracking issues into version control as mb files, mb needs good 3-way merge conflict resolution including comments, for git-level merges as well as GitHub sync (PR #26 covered only GitHub title/body/state).

Delivered in 0.29.0 (branch mb-3way/finish, supersedes https://github.com/rrnewton/minibeads/pull/26):
- src/issue_merge.rs: whole-issue 3-way merge (every scalar field, label set, per-target dependencies, timestamp rules with closed_at derived from status, all four prose sections via prose_merge) and a three-way, ID-keyed comment-set merge (additions kept once; a deletion propagates unless the other side edited the comment, then the edit is kept with a NOTICE; deleted-on-both disappears).
- src/diff3.rs: line diff3 with fixed ours/base/theirs labels plus mark_view_differences; conflicts are rendered from three views so hunks cover only the conflicting field/paragraph/comment and taking side S of every hunk reproduces side S exactly (status hunks carry closed_at). Markers grow past marker-like content lines.
- src/merge_driver.rs: mb merge-driver run|install|show (git %O %A %B %L %P protocol, git config + **/-anchored .gitattributes for nested databases); non-UTF-8 input becomes one byte hunk.
- src/format.rs: quoted conflict-opening lines are escaped as \<<<<<<< on write and unescaped on read, so creation and GitHub import never fail; genuine unresolved hunks are refused on read with the file, line and escape named.
- src/prose_merge.rs: word-level merges must keep every word's count between the two sides' counts (fixes duplicated insertions and double deletions); final-paragraph separator padding.
- Fixed nondeterministic depends_on ordering (HashMap -> BTreeMap).
- Tests: explicit cases + 2500-case seeded properties (commutativity, identity, re-merge idempotence against a paragraph-diff3 oracle, machine-resolvable hunks incl. closed_at/created_at, exact comment sets under mixed resolutions) + a 5000-seed prose subsumption property + tests/merge_driver.sh with real git merges (comment deletion vs add/untouched/edit, nested routing). Mutation-checked 2026-10-03_#232(5742073e8e): 16 deliberate breakages all caught.
- Adversarial review (2026-10-03) requested changes; all HIGH/MEDIUM/LOW findings addressed in the follow-up commit.
Design: ai_docs/merge-driver-design.md.
