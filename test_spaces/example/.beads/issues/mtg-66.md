---
title: Begin or progress work on simple interactive TUI
status: closed
priority: 0
issue_type: task
labels:
  - human
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-26T21:06:34Z"
closed_at: "2025-10-25T21:25:09Z"
---

# Description

The basic `mtg tui --p1=tui` will be very similar to the Java version. Instead
of making a random choice for every `Enter choice (0-N)` prompt, we will present
the choice to the user and wait for input on stdin.

Later we will add a fancy TUI with the ratatui library, so we should keep an eye
on how to make our Controller interface generic. It will present the battlefield
and the choices in a different way.

## Status: COMPLETED

Implemented InteractiveController in src/game/interactive_controller.rs.
Supports all 8 core decision methods with simple stdin/stdout interface.
CLI now accepts --p1 interactive and --p2 interactive flags.
All tests passing.
