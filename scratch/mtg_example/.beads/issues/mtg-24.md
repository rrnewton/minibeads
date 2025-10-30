---
title: '"Any target" vs creature-only vs player-only targeting'
status: open
priority: 3
issue_type: feature
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
---

# Description

Properly distinguish between target types:
- ValidTgts$Creature,Player (any target - can hit creatures or players)
- ValidTgts$Creature (creature-only)
- ValidTgts$Player (player-only)
- ValidTgts$Permanent (any permanent)

Update spell effects to respect these distinctions.
