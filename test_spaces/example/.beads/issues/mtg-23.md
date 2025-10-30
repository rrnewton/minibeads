---
title: Target selection by controllers
status: closed
priority: 3
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-25T00:40:39Z"
---

# Description

Allow controllers to select targets for spells and abilities.
- Add choose_targets() method to PlayerController trait
- Handle "any target", creature-only, player-only target modes
- Support multiple targets (e.g., Forked Lightning)
- Handle optional vs required targets
