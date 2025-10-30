---
title: 'Critical: MCP integration bugs - dependencies not exposed correctly'
status: open
priority: 0
issue_type: bug
assignee: claude
created_at: 2025-10-30T14:30:56.705927961+00:00
updated_at: 2025-10-30T14:30:56.705927961+00:00
---

# Description

During comprehensive MCP testing, discovered critical bugs that break agent workflows.

## Bug #1: Dependencies/Dependents Schema Mismatch (CRITICAL)

**Symptom**: MCP server show/list operations return empty dependencies and dependents arrays even when issues have dependencies.

**Root Cause**: Schema mismatch between minibeads Issue struct and MCP server expectations.
- minibeads outputs: `depends_on: HashMap<String, DependencyType>`
- MCP server expects: `dependencies: []` and `dependents: []`

**Evidence**:
```bash
## CLI JSON output (correct):
bd show mcp-8 --json
{"depends_on": {"mcp-7": "parent-child"}, ...}

## MCP show output (wrong):
mcp show mcp-8
{"dependencies": [], "dependents": [], ...}
```

**Impact**: AI agents cannot see dependencies, breaking workflow planning.

**Reproducer**:
```bash
cd /workspace/scratch/mcp_test
## Create issues with dependencies
bd create "Task A" -p 1
bd create "Task B" -p 1
bd dep add b-2 b-1
## Via MCP: show b-2 - dependencies will be empty
```

## Bug #2: Acceptance Criteria with Markdown List Causes Parse Error

**Symptom**: Creating issue via MCP with acceptance criteria starting with `- ` fails with clap parse error.

**Error Message**:
```
error: unexpected argument '- ' found
  tip: to pass '- ' as a value, use '-- - '
```

**Reproducer**:
```python
mcp.create(
  title="Test",
  acceptance="- Criteria 1\n- Criteria 2"
)
## Fails with parse error
```

**Workaround**: Use non-dash format like "Criteria 1, Criteria 2"

**Root Cause**: MCP server passes acceptance text as CLI argument, clap interprets leading dash as flag.

## Bug #3: Nonexistent Dependencies Accepted Without Validation

**Symptom**: Can create issues depending on nonexistent issues. They appear in blocked list.

**Evidence**:
```bash
bd create "Task" --deps mcp-999  # mcp-999 doesn't exist
bd blocked  # Shows task blocked by mcp-999
```

**Impact**: Agents can create invalid dependency graphs. Might be intentional for forward references?

**Decision Needed**: Should we validate dependency targets exist, or allow forward references?

## Testing Matrix

All MCP operations tested:

| Operation | Status | Notes |
|-----------|--------|-------|
| set_context | ✅ Works | - |
| where_am_i | ✅ Works | - |
| create | ⚠️ Partial | Bug #2 with acceptance criteria |
| list | ⚠️ Partial | Bug #1 - empty dependencies |
| show | ⚠️ Partial | Bug #1 - empty dependencies |
| update | ✅ Works | - |
| close | ✅ Works | - |
| reopen | ✅ Works | Multiple IDs supported |
| dep | ⚠️ Partial | Bug #3 - no validation |
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

## Recommended Fixes

1. **Bug #1 (Priority 0)**: Add transformation layer or modify Issue struct to include dependencies/dependents arrays for JSON output
2. **Bug #2 (Priority 1)**: Escape or quote CLI arguments passed by MCP server, or use stdin for long text
3. **Bug #3 (Priority 2)**: Add optional --validate-deps flag, default to warn on nonexistent deps
