---
title: 'Manual QA: remote pull 2026-06-27T021927Z'
status: closed
priority: 2
issue_type: task
external_ref: https://github.com/rrnewton/minibeads/issues/9
created_at: 2026-06-27T02:16:07.448902032+00:00
updated_at: 2026-06-27T02:19:34.025303549+00:00
closed_at: 2026-06-27T02:19:34.025303074+00:00
---

# Description

Remote body pulled from GitHub during QA 2026-06-27T021927Z

# Notes

Manual QA completed at 2026-06-27T021927Z against https://github.com/rrnewton/minibeads/issues/9.

Checks passed:
- Created local minibeads issue minibeads-31.
- Created real GitHub issue with gh: https://github.com/rrnewton/minibeads/issues/9.
- Linked with mb github link and verified mb github list shows the linkage.
- Verified mb show displays External ref.
- Updated title/body locally and synced to GitHub with mb github sync --verbose.
- Added local mb comment and verified it appeared once on GitHub.
- Fixed real QA failures discovered during testing: missing GitHub comment updatedAt handling and retry duplicate prevention.
- Edited GitHub title/body and added a GitHub comment with gh.
- Synced back with mb github sync --verbose.
- Verified minibeads title/body and comments reflected GitHub changes.
- Closed local issue and synced closure to GitHub.
