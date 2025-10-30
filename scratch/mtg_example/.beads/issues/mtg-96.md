---
title: 'TODO: search for cardsfolder'
status: open
priority: 2
issue_type: task
labels:
  - human
created_at: "2025-10-27T06:51:48-07:00"
updated_at: "2025-10-27T06:51:48-07:00"
---

# Description

Right now, we expect `./cardsfolder` to exist. Later we'll have a proper installer,
but for now let's make our search process this:
 - `./cardsfolder` if it exists, if not
 - go to the directory containing the `mtg` binary, look for `cardsfolder` there
 - if not found, go up to the parent directory, repeating the search for `./cardsfolder`.
 - if we reach the root `/` and don't find it, then error.
