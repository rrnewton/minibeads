---
title: Implement Comments data model in markdown format
status: open
priority: 2
issue_type: feature
depends_on:
  minibeads-10: related
created_at: 2025-10-30T14:00:08.401740698+00:00
updated_at: 2025-10-30T14:00:08.776545666+00:00
---

# Description

Add support for comments on issues, stored directly in the markdown file.

## Background
The original beads implementation has a comments system. We need to port this to our markdown-based storage format.

## Proposed Design

### Data Model
Each comment should have:
- **comment_id**: Unique ID (timestamp-based or UUID)
- **author**: Who wrote the comment
- **created_at**: ISO 8601 timestamp
- **body**: The comment text (supports markdown)

### Markdown Format
Add a `# Comments` section at the end of each issue markdown file:

```markdown
## Comments

- **2025-10-30T14:00:00Z** (@alice):
  This looks like it might be related to the authentication refactor in PR #42.

- **2025-10-30T14:15:00Z** (@bob):
  Good catch! I'll add a dependency on minibeads-12.
```

### CLI Commands
- `bd comment add <issue-id> <text>` - Add a comment
- `bd comment list <issue-id>` - List comments for an issue
- `bd comment delete <issue-id> <comment-id>` - Delete a comment (marks as deleted, keeps for audit)
- `bd show <issue-id>` - Include comments in output

### Storage Implementation
- Parse comments from markdown in `markdown_to_issue()`
- Serialize comments in `issue_to_markdown()`
- Add `comments: Vec<Comment>` to Issue struct
- Maintain chronological order

### MCP Integration
Ensure MCP tools can read/write comments through the existing format.

## Acceptance Criteria
- [ ] Comment struct added to types.rs
- [ ] Parsing and serialization in format.rs
- [ ] CLI commands implemented
- [ ] Comments shown in `bd show` output
- [ ] Tests for comment operations
- [ ] MCP compatibility verified
