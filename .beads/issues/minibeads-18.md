---
title: Implement bd rename-prefix command
status: closed
priority: 3
issue_type: feature
created_at: 2025-10-31T03:38:49.859944718+00:00
updated_at: 2025-10-31T03:38:55.858011570+00:00
closed_at: 2025-10-31T03:38:55.858011289+00:00
---

# Description

Implement the bd rename-prefix command to rename the issue prefix for all issues in the database. Includes --dry-run flag for previewing changes and --force flag for handling conflicts.

# Acceptance Criteria

- Command supports renaming all issues from old-prefix to new-prefix
- --dry-run flag shows preview of changes
- --force flag allows overriding conflicts
- All dependencies are updated correctly
- config.yaml is updated with new prefix
- All tests pass
