---
title: Add profile command to CLI
status: closed
priority: 3
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-24T23:20:24Z"
---

# Description

Add a `profile` subcommand that runs heaptrack profiling automatically.

Should:
- Run a configurable number of games
- Generate heaptrack output
- Automatically analyze results
- Print top allocation sites

Example: `mtg profile --games 1000 --seed 42`
