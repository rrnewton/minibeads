---
title: Merge driver for tracked .beads/github-sync-state.json
status: open
priority: 4
issue_type: task
created_at: 2026-10-03T06:41:27.761480786+00:00
updated_at: 2026-10-03T06:41:27.761480786+00:00
---

# Description

Follow-up from minibeads-38. github-sync-state.json (comment ancestry for GitHub sync) is tracked by git but has no merge driver, so two branches that each ran mb github sync conflict textually on it. Options: route it to mb merge-driver with a keyed per-issue/per-comment-pair union (pairs are append-only like comments; conflicting pairings for one local id need a typed conflict), or move it out of version control next to sync_ancestors/ (per clone). Decide which; the ancestor archive is already per-clone and gitignored.
