---
title: Implement Events/audit trail data model in markdown format
status: open
priority: 4
issue_type: feature
created_at: 2025-10-30T14:00:08.585118377+00:00
updated_at: 2025-10-30T14:00:08.585118377+00:00
---

# Description

Add audit trail tracking for all issue changes, stored in markdown format.

## Background
The original beads has a sophisticated audit trail that agents use to reconstruct complex operations spanning multiple sessions. We need to implement this in our markdown-based system.

## Proposed Design

### Data Model
Each event should capture:
- **event_id**: Unique ID (timestamp-based)
- **event_type**: created, updated, status_changed, closed, reopened, comment_added, dependency_added, etc.
- **actor**: Who performed the action (user, agent name, or "system")
- **timestamp**: ISO 8601 timestamp
- **changes**: What changed (field name, old value, new value)
- **metadata**: Optional context (commit hash, session ID, etc.)

### Markdown Format
Add an `# Events` section in the markdown file:

```markdown
## Events

- **2025-10-30T13:22:12Z** [created] by claude
  - Created issue with priority 1, type task

- **2025-10-30T13:27:05Z** [status_changed] by claude
  - Changed status from 'open' to 'in_progress'

- **2025-10-30T13:30:25Z** [updated] by alice
  - Changed priority from 1 to 0
  - Changed assignee from '' to 'alice'

- **2025-10-30T14:00:00Z** [comment_added] by alice (comment-1)
  - Added comment

- **2025-10-30T14:15:00Z** [dependency_added] by bob
  - Added dependency: blocks minibeads-12
```

### Storage Strategy Options

**Option A: Embedded in markdown** (simpler)
- Events stored in markdown # Events section
- Parsed and written with issue
- Easy to see history in file

**Option B: Separate events file** (more scalable)
- Store in `.beads/events/<issue-id>.jsonl`
- Append-only for performance
- Separate from issue content

**Recommendation**: Start with Option A for consistency with our markdown-first approach. Can migrate to Option B if performance becomes an issue.

### CLI Commands
- `bd events <issue-id>` - Show event history
- `bd events <issue-id> --type status_changed` - Filter by event type
- `bd events <issue-id> --actor alice` - Filter by actor
- Auto-log events on all update operations

### Implementation Details
- Add `events: Vec<Event>` to Issue struct
- Auto-generate events in update operations (update_issue, close_issue, etc.)
- Include actor from `--actor` CLI flag or env var
- Preserve events when parsing/serializing

## Use Cases for Agents
- Reconstruct what happened across multiple sessions
- Understand who made which changes and why
- Debug issues by reviewing change history
- Generate reports on velocity and activity

## Acceptance Criteria
- [ ] Event struct added to types.rs with all event types
- [ ] Auto-logging in all mutation operations
- [ ] Events section in markdown format
- [ ] CLI command for viewing events
- [ ] Events included in `bd show --verbose` output
- [ ] Tests for event tracking
- [ ] Documentation on event types
