---
title: Add 'mb comments delete' to remove comments by ID
status: closed
priority: 3
issue_type: feature
created_at: 2026-07-02T22:57:19.080606267+00:00
updated_at: 2026-07-02T22:58:18.457857074+00:00
closed_at: 2026-07-02T22:58:18.457856940+00:00
---

# Description

Comments supported add/list but not delete, so a mistaken or obsolete comment could not be removed via the CLI (agents would be tempted to hand-edit .minibeads/comments/*.json).

Added `mb comments delete <ISSUE_ID> <COMMENT_ID>...`:
- Deletes one or more comments by the ID shown in `mb comments list`.
- Unknown/typo comment ID is an error and the store is left untouched.
- Completes comment CRUD (add/list/delete). minibeads-specific; upstream bd has no comment deletion.

Implemented: Storage::delete_comment (storage.rs), CommentCommands::Delete + handler (main.rs). Tests: 2 unit tests (delete_comment_removes_only_the_targeted_comment, delete_missing_comment_errors_and_leaves_others) + basic_operations.sh Test 5b. Docs: README + CHANGELOG (Unreleased).
