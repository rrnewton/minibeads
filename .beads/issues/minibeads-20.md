---
title: Implement deep verification with reference interpreter for random action tests
status: open
priority: 3
issue_type: feature
created_at: 2025-10-31T13:33:00.367658716+00:00
updated_at: 2025-10-31T13:33:00.367658716+00:00
---

# Description

Currently the random action test only verifies:
- Commands run without error
- config.yaml exists
- Can list issues at the end

We need much deeper verification to ensure correctness.

## Implementation Plan

### 1. Reference Interpreter
Build an in-memory reference interpreter that:
- Maintains a HashMap of Issue structs
- Executes each BeadsAction to update the in-memory state
- Produces a "golden" final state that implementations must match

### 2. Recursive .beads Directory Listing
In verify_consistency():
- Walk the .beads directory tree recursively
- In verbose mode, report:
  - List of all file paths
  - Size of each file
  - Cumulative total size

### 3. Minibeads State Verification
For minibeads implementation:
- Parse and validate config.yaml
- Read each markdown issue file using existing deserialization
- Compare each issue against the golden reference state
- Report any differences using colorful diff library (similar-asserts)

### 4. Upstream JSONL Verification
For upstream bd with --no-db:
- Verify SQLite DBs do NOT exist (already done)
- Read issues.jsonl using our import_from_jsonl code
- Compare full state against golden reference
- Report differences with colorful diffs

## Dependencies Added
- similar-asserts = "1.6" (for colorful diffs)

## Benefits
- Catches subtle correctness bugs that error checking misses
- Ensures both implementations produce identical results for same actions
- Makes test failures actionable with detailed diffs
- Validates our markdown format roundtrips correctly

# Design

Reference Interpreter Design:
```rust
struct ReferenceInterpreter {
    issues: HashMap<String, Issue>,
    prefix: String,
    next_id: usize,
}

impl ReferenceInterpreter {
    fn new(prefix: String) -> Self { ... }
    
    fn execute(&mut self, action: &BeadsAction) -> Result<()> {
        match action {
            Init => initialize state,
            Create => add to HashMap,
            Update => modify in HashMap,
            Close => update status,
            // etc for all actions
        }
    }
    
    fn get_final_state(&self) -> &HashMap<String, Issue> { ... }
}
```

Deep Verification Flow:
1. Execute actions against real implementation
2. Execute same actions against reference interpreter
3. Compare final states with detailed diff reporting

# Acceptance Criteria

- [ ] ReferenceInterpreter maintains accurate in-memory state
- [ ] All BeadsActions handled by interpreter
- [ ] Recursive directory listing with sizes in verbose mode
- [ ] Minibeads: markdown files parsed and compared to golden state
- [ ] Upstream: JSONL parsed and compared to golden state
- [ ] Colorful diffs show exact differences when verification fails
- [ ] Test passes for both minibeads and upstream with --verbose showing detailed info
