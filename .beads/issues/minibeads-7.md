---
title: Add colorful output to CLI commands
status: open
priority: 3
issue_type: feature
created_at: 2025-10-30T13:43:05.212615007+00:00
updated_at: 2025-10-30T13:43:05.212615007+00:00
---

# Description

Enhance CLI output with colors similar to the original bd implementation.

## Motivation
Currently all output is plain text. Adding colors would improve readability and user experience, especially for commands like `bd show`, `bd list`, and `bd stats`.

## Scope
- Add a color/styling library (e.g., `colored`, `owo-colors`, or `termcolor`)
- Colorize `bd show` output:
  - Issue ID and title (bold/bright)
  - Status with semantic colors (green=open, blue=in_progress, yellow=blocked, gray=closed)
  - Priority levels (red=0, yellow=1, white=2+)
  - Section headers (Description, Dependencies, etc.)
- Colorize `bd list` output
- Colorize `bd stats` output
- Add `--no-color` flag to disable colors
- Respect NO_COLOR environment variable
- Ensure JSON output is not affected by color codes

## Acceptance Criteria
- [ ] Colors work in terminals that support ANSI codes
- [ ] Plain output when piped or NO_COLOR is set
- [ ] JSON output remains clean (no color codes)
- [ ] --no-color flag works correctly
