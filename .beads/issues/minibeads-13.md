---
title: 'Critical: MCP integration bugs - dependencies not exposed correctly'
status: closed
priority: 0
issue_type: bug
assignee: claude
created_at: 2025-10-30T14:30:56.705927961+00:00
updated_at: 2025-10-31T03:16:31.852955717+00:00
closed_at: 2025-10-31T03:16:31.852955447+00:00
---

# Description

During comprehensive MCP testing, discovered critical bugs that break agent workflows.

## Bug #1: Dependencies/Dependents Schema Mismatch (CRITICAL) - ✅ FIXED

**Status**: ✅ RESOLVED in commit 5247166

**Symptom**: MCP server show/list operations return empty dependencies and dependents arrays even when issues have dependencies.

**Root Cause**: Schema mismatch between minibeads Issue struct and MCP server expectations.
- minibeads outputs: `depends_on: HashMap<String, DependencyType>`
- MCP server expects: `dependencies: []` and `dependents: []`

**Fix Applied**:
1. Added custom serde serialization for `depends_on` HashMap
   - Serializes as "dependencies" array with {id, type} objects
   - Deserializes both old HashMap and new array formats (backward compat)
2. Added computed "dependents" field to Issue struct
   - Populated by reverse dependency lookup in storage layer
3. Storage layer changes to compute and populate dependents

**Impact**: ✅ AI agents can now see dependencies correctly, unblocking workflow planning.

## Bug #2: Acceptance Criteria with Markdown List Causes Parse Error - ✅ FIXED

**Status**: ✅ RESOLVED

**Symptom**: Creating issue via MCP with acceptance criteria starting with `- ` fails with clap parse error.

**Fix Applied**:
Added `allow_hyphen_values = true` to all text field arguments that might contain markdown lists or leading hyphens.

**Impact**: ✅ AI agents can now use markdown lists in all text fields without workarounds.

## Bug #3: Nonexistent Dependencies Accepted Without Validation - ✅ FIXED

**Status**: ✅ RESOLVED

**Symptom**: Can create issues depending on nonexistent issues without any warning.

**Evidence**:
```bash
bd create "Task" --deps mcp-999  # mcp-999 doesn't exist, no warning
bd blocked  # Shows task blocked by mcp-999
```

**Root Cause**: No validation in create_issue() or add_dependency() methods.

**Fix Applied** (src/storage.rs:499-512):
Added validate_dependency_exists() helper that emits warnings when dependencies don't exist:
- Called in create_issue() when processing --deps
- Called in add_dependency() when adding dependencies
- Warns user but allows forward references (for planned work)

**Impact**: ✅ Users now get clear warnings about nonexistent dependencies while still supporting forward references.

**Verification**:
```bash
bd create "Task" --deps mcp-999 2>&1
## Warning: Dependency target does not exist: mcp-999
##   This issue will be blocked until mcp-999 is created.
## Created issue: mcp-1
```

## Testing Matrix

All MCP operations tested:

| Operation | Status | Notes |
|-----------|--------|-------|
| set_context | ✅ Works | - |
| where_am_i | ✅ Works | - |
| create | ✅ FIXED | Bug #2 resolved |
| list | ✅ FIXED | Bug #1 resolved |
| show | ✅ FIXED | Bug #1 resolved |
| update | ✅ FIXED | Bug #2 resolved |
| close | ✅ FIXED | Bug #2 resolved |
| reopen | ✅ FIXED | Bug #2 resolved |
| dep | ✅ FIXED | Bug #3 resolved |
| stats | ✅ Works | - |
| blocked | ✅ Works | Shows all blockers including nonexistent |
| ready | ✅ Works | Correctly excludes hard blockers only |
| init | ❌ N/A | Can't test in existing dir |
| debug_env | ✅ Works | - |

## Edge Cases Tested

✅ Empty descriptions
✅ Special chars in titles/descriptions (<>[]{}|&$`~!@#%^*())
✅ Quotes and apostrophes in text
✅ Multiline content with markdown
✅ Very long titles (120+ chars)
✅ All issue types (bug, feature, task, epic, chore)
✅ All dependency types (blocks, related, parent-child, discovered-from)
✅ Multiple labels
✅ External references
✅ Reopening multiple issues at once
✅ Filtering by status, priority, type, assignee
✅ Nonexistent dependencies (now warns)

## Resolution

All three critical bugs have been resolved. MCP integration is now fully functional with proper validation and user warnings.
