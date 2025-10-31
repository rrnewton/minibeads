---
title: Complete remaining DRY refactoring in test_minibeads.rs
status: closed
priority: 3
issue_type: task
created_at: 2025-10-31T19:25:46.021172621+00:00
updated_at: 2025-10-31T19:28:47.239208845+00:00
closed_at: 2025-10-31T19:28:47.239208565+00:00
---

# Description

Complete the remaining DRY (Don't Repeat Yourself) refactoring opportunities in test_minibeads.rs identified during the enum type cleanup work.

## Completed in Previous Commit

✅ Eliminated duplicate enum types (Status, IssueType, DependencyType)
✅ Removed converter functions by using FromStr trait with .parse()
✅ Net 70 lines removed from codebase

## Remaining Work

### 1. verify_config() Function (Lines 817-847)

**Current Issue**: verify_config() manually parses config.yaml to extract the prefix

**Problem**: Duplicates logic that exists in Storage::get_prefix()

**Solution**: Use Storage::get_prefix() or shared config parsing code instead of manual YAML parsing

**Complexity**: Medium - requires understanding how Storage handles config loading

### 2. parse_minibeads_issue() Function (Lines 926-995)

**Current Issue**: Manually parses markdown files with custom frontmatter parsing

**Problem**: Duplicates logic from format::markdown_to_issue()

**Solution**: 
- Use format::markdown_to_issue() to parse markdown files
- Convert the resulting Issue to ReferenceIssue as needed
- May require adding a From<Issue> impl for ReferenceIssue

**Complexity**: Medium-High - involves type conversions between full Issue and simplified ReferenceIssue

## Analysis Notes

The converter functions we initially thought were duplicates turned out to be legitimate - they converted strings to test-specific enum types in beads_generator. However, those test types were themselves duplicates, which we've now eliminated.

The verify_config and parse_minibeads_issue functions are genuine DRY violations that should be addressed in a future commit.

## Priority

P3 - This is cleanup work that improves code maintainability but doesn't affect functionality. Can be done when convenient.
