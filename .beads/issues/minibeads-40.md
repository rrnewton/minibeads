---
title: Same comment under two IDs after merging a GitHub export with a GitHub import
status: open
priority: 4
issue_type: bug
created_at: 2026-10-03T06:41:27.767236850+00:00
updated_at: 2026-10-03T06:41:27.767236850+00:00
---

# Description

Follow-up from minibeads-38. If branch A exports a local comment (id <issue>-c<hash>) to GitHub while branch B imports that GitHub comment (id gh-<remote id>), mb merge-driver's ID-keyed append-only comment merge keeps both: same text, two IDs. Only github-sync-state.json pairs the two IDs, and the merge driver cannot see it. A later mb github sync could detect the pair (remote id recorded for the local comment equals the gh-<id> source_id) and drop the imported duplicate. Needs a regression test with the mocked gh harness.
