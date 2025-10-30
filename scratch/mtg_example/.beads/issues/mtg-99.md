---
title: 'TODO: overhaul snapshot serialization'
status: open
priority: 2
issue_type: task
labels:
  - human
created_at: "2025-10-27T06:51:48-07:00"
updated_at: "2025-10-27T06:51:48-07:00"
---

# Description

First, produce a criterion benchmark that times the saving of snapshot to disk. You probably want to play midway into a game to get a good representative snapshot to benchmark.

Second, stop pretty-printing the json snapshots. We don't need that and the user can always use `jq`.

Third, introduce a flag to control json/binary serialization, and make it binary by default. You should be able to use the same `Serialize` instance but with a different backend. You can use the `bincode` serde backend because we don't need them to be versioned, self-describing, or shared with non-Rust languages.
