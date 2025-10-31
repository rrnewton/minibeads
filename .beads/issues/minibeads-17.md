---
title: Implement bd rename operation from upstream
status: closed
priority: 3
issue_type: feature
created_at: 2025-10-30T20:39:02.772892838+00:00
updated_at: 2025-10-31T02:59:34.645939466+00:00
closed_at: 2025-10-31T02:59:34.645939145+00:00
---

# Description

Implement the bd rename operation matching upstream bd functionality.

## Requirements

### Basic rename functionality
- Rename issues by changing their ID
- Example: bd rename minibeads-10 minibeads-100
- Update all references in:
  - Issue markdown files (dependencies, dependents)
  - Other issues that reference the renamed issue
  - Command history logs

### --dry-run flag
- Show what would be renamed without actually doing it
- Display all files and references that would be updated
- Useful for previewing the impact of a rename operation

### --repair flag  
- Fix broken references after a rename
- Scan all issues and update stale references
- Useful if references got out of sync

### Implementation details
- Rename the markdown file: old-id.md → new-id.md
- Update the id field in the frontmatter
- Find and update all references in other issues
- Update depends_on and dependents fields
- Preserve all other metadata (timestamps, status, etc.)
- Atomic operation - either all updates succeed or none

### Testing
- Test basic rename operation
- Test --dry-run shows correct preview
- Test --repair fixes broken references
- Test renaming with dependencies/dependents
- Test error handling for conflicts
