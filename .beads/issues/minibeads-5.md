---
title: Fix Status enum serialization and improve error messages
status: closed
priority: 3
issue_type: bug
assignee: claude
created_at: 2025-10-30T13:30:25.247720444+00:00
updated_at: 2025-10-30T13:30:25.366797217+00:00
closed_at: 2025-10-30T13:30:25.366796656+00:00
---

# Description

Fixed MCP integration bug where Status enum was serializing incorrectly.

## Changes Made
- Changed Status enum from `#[serde(rename_all = "lowercase")]` to `snake_case` to match Display/FromStr format
- Improved all FromStr error messages to show invalid input and list valid values
- Added `--validation=silent|warn|error` flag (default: error) for future extensibility
- Fixed clippy warnings: needless question marks, manual strip, collapsible str::replace
- Created Makefile with validate target (test + fmt + clippy)
- Reorganized scratch directory structure

## Test Results
- All 3 unit tests passing
- MCP integration verified working with in_progress status
- make validate passes cleanly
