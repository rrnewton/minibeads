---
title: Arena allocation for per-turn temporaries
status: open
priority: 4
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
---

# Description

Use arena allocators (bumpalo or typed-arena) for per-turn allocations.
Benefits: faster allocation (pointer increment), bulk deallocation, better cache locality.
