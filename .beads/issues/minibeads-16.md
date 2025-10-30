---
title: Implement bidirectional sync (bd sync)
status: open
priority: 3
issue_type: feature
created_at: 2025-10-30T20:38:48.178328137+00:00
updated_at: 2025-10-30T20:38:48.178328137+00:00
---

# Description

Implement bidirectional sync as specified in minibeads-12.

## Requirements

### bd sync command
- Implement explicit sync command that users can invoke manually
- Should sync issues between markdown storage and issues.jsonl
- Bidirectional: markdown → jsonl and jsonl → markdown
- Conflict resolution strategy needed

### Auto-sync configuration
- Add mb-auto-sync config option in config.yaml
- When mb-auto-sync=true, automatically sync after write operations
- When mb-auto-sync=false, only sync when bd sync is explicitly called
- Default should be false (explicit sync only)

### Implementation considerations
- Need to handle conflicts when both markdown and jsonl have been modified
- Timestamps can help determine which version is newer
- Consider using file modification times as well
- Should detect and handle concurrent modifications gracefully

### Testing
- Test explicit bd sync command
- Test auto-sync with config option enabled
- Test conflict detection and resolution
- Test with concurrent modifications
