---
title: Implement bd export to issues.jsonl format
status: closed
priority: 3
issue_type: feature
created_at: 2025-10-30T14:11:15.924424355+00:00
updated_at: 2025-10-30T19:15:47.086461274+00:00
closed_at: 2025-10-30T19:15:47.086461013+00:00
---

# Description

Add support for exporting minibeads data to the upstream bd issues.jsonl format for interoperability.

## Background
Upstream bd uses a JSONL (JSON Lines) format for storing issues in a single `issues.jsonl` file. To enable interoperability between minibeads and bd, we need to support exporting our markdown-based storage to this format.

## Proposed Design

### JSONL Format
Each line in issues.jsonl is a complete JSON object representing one issue. Example:
```json
{"id":"bd-1","title":"Fix auth bug","status":"open","priority":1,"issue_type":"bug","created_at":"2025-10-30T10:00:00Z","updated_at":"2025-10-30T10:00:00Z","depends_on":{"bd-2":"blocks"}}
```

### CLI Command
```bash
bd export -o issues.jsonl       # Export all issues
bd export --status open         # Export with filters
bd export --format jsonl        # Explicit format (for future CSV/etc)
```

### Implementation
- Read all issues from markdown storage
- Serialize to JSONL format matching upstream bd schema
- Write to output file (default: issues.jsonl)
- Support filtering by status, priority, type, assignee

### Field Mapping
Map our markdown Issue struct to upstream bd JSON schema:
- All core fields (id, title, status, priority, etc.) map directly
- depends_on HashMap serializes to JSON object
- Timestamps use ISO 8601 format
- Handle optional fields (external_ref, assignee, etc.)

### Testing
- Test roundtrip with upstream bd
- Verify JSON schema compatibility
- Test with various issue types and states
