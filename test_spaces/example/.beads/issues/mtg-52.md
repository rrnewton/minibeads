---
title: Parallel game search capabilities
status: open
priority: 4
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
---

# Description

Multi-threaded tree search:
- Parallel MCTS (root or leaf parallelization)
- Thread-safe game state cloning
- Work-stealing scheduler
- Utilize all CPU cores
