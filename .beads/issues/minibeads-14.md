---
title: 'Test Porting Plan: Adapt upstream beads tests for minibeads'
status: open
priority: 2
issue_type: task
labels:
- testing
- planning
- documentation
depends_on:
  minibeads-3: parent-child
created_at: 2025-10-30T14:51:10.942162916+00:00
updated_at: 2025-10-30T16:26:20.249371754+00:00
---

# Description

## Overview

Research upstream beads testing approach and create porting plan for minibeads. The goal is to ensure minibeads maintains compatibility with upstream while leveraging our simpler architecture.

## Current Status

### Minibeads Testing (Current)
- ✅ Unit tests: 3 tests in src/ (format, lock roundtrip)
- ✅ E2E tests: 2 shell scripts (basic_operations.sh: 36 assertions, help_version.sh: 28 assertions)
- ✅ Test harness: Rust integration in tests/e2e_tests.rs with auto-discovery
- ✅ CI: GitHub Actions with full validation
- ✅ Phase 1 COMPLETE: All core commands have test coverage
- ⚠️ Coverage: Need Phase 2 unit tests for edge cases

### Upstream Beads Testing (Analyzed)
- **78 Go test files** in total
- **17 script tests** (rsc.io/script format)
- **Test categories**: Unit, integration, script-based, benchmarks

## Upstream Testing Approach

### 1. Script-Based Tests (scripttest)
**Framework**: rsc.io/script
**Location**: `cmd/bd/testdata/*.txt`
**Count**: 17 test files

**Format**:
```txt
## Comment describing test
bd init --prefix test
stdout 'initialized successfully'
exists .beads/test.db
grep '^pattern$' .beads/.gitignore
```

**Covered Commands**:
- blocked.txt
- close.txt
- create.txt
- dep_add.txt, dep_remove.txt, dep_tree.txt
- export.txt, import.txt
- help.txt
- init.txt
- list.txt, list_json.txt
- ready.txt
- show.txt
- update.txt
- version.txt

### 2. Unit Tests (Table-Driven)
**Framework**: Standard Go testing
**Pattern**: Table-driven tests with struct test cases

**Example** (markdown_test.go):
```go
tests := []struct{
    name string
    content string
    expected []*IssueTemplate
    wantErr bool
}{...}
```

**Key Test Files**:
- markdown_test.go - Markdown parsing (YAML frontmatter)
- dep_test.go - Dependency logic
- export_test.go - Export to JSONL
- import_test.go - Import from various formats
- comments_test.go - Comment operations
- events_test.go - Event/audit trail
- config_test.go - Configuration handling

### 3. Integration Tests
**Test Complex Workflows**:
- autoimport_collision_test.go - Auto-import conflict resolution
- daemon_test.go - Daemon operations
- export_import_test.go - Roundtrip testing
- import_collision_test.go - Collision handling
- renumber_test.go - ID renumbering
- worktree_test.go - Git worktree support

### 4. Feature-Specific Tests
**Advanced Features**:
- compact_test.go - Issue compaction
- duplicates_test.go - Duplicate detection
- epics_test.go - Epic/subtask relationships
- stale_test.go - Stale claim detection
- cycle detection, graph operations

### 5. Benchmarks
**Performance Testing**:
- bench_test.go - General benchmarks
- compact_bench_test.go - Compaction performance
- cycle_bench_test.go - Cycle detection perf

## Applicability to Minibeads

### ✅ Directly Applicable (High Priority)

**Script Tests** (Can port almost all):
- `init.txt` - ✅ Covered in basic_operations.sh
- `create.txt` - ✅ Covered in basic_operations.sh
- `list.txt` - ✅ Covered in basic_operations.sh
- `show.txt` - ✅ Covered in basic_operations.sh (+ numeric shorthand)
- `update.txt` - ✅ Covered in basic_operations.sh
- `close.txt` - ✅ Covered in basic_operations.sh
- `dep_add.txt` - ✅ Covered in basic_operations.sh
- `blocked.txt` - ✅ Covered in basic_operations.sh
- `ready.txt` - ✅ Covered in basic_operations.sh
- `help.txt` - ✅ Covered in help_version.sh
- `version.txt` - ✅ Covered in help_version.sh
- `list_json.txt` - ✅ Covered in basic_operations.sh
- `dep_remove.txt` - ❌ Not implemented (no dep remove command yet)
- `dep_tree.txt` - ❌ Not implemented
- `export.txt` - ❌ Not implemented (minibeads-11)
- `import.txt` - ❌ Not implemented

**Unit Tests** (Should port):
- Markdown parsing (format.rs already has some)
- Dependency logic (types.rs logic needs tests)
- Config validation (config.yaml handling)
- Lock acquisition/release (lock.rs has basic test)

### ⚠️ Partially Applicable (Adapt for Markdown)

**Tests Requiring Adaptation**:
- `export_test.go` - Adapt for markdown-only storage
- `import_test.go` - Adapt for markdown import
- `comments_test.go` - Port when minibeads-9 implemented
- `events_test.go` - Port when minibeads-10 implemented
- `markdown_test.go` - Already using YAML frontmatter, verify compatibility

### ❌ Not Applicable (SQLite/Daemon/Advanced Features)

**Skip These Tests**:
- `daemon_*.go` - No daemon in minibeads
- `beads_multidb_test.go` - No SQLite
- `autoimport_*.go` - Auto-import not planned
- `autostart_test.go` - No daemon
- `compact_*.go` - Compaction not planned for markdown
- `worktree_test.go` - Git worktree not priority
- `duplicates_test.go` - Not priority
- `import_collision_*.go` - Import not priority
- `direct_mode_test.go` - SQLite-specific

## Test Porting Plan

### Phase 1: Core Command Coverage (P0)
**Goal**: Achieve parity with upstream script tests for implemented commands

**Tasks**:
1. ✅ Port init.txt (DONE - basic_operations.sh)
2. ✅ Port create.txt (DONE - basic_operations.sh)
3. ✅ Port list.txt (DONE - basic_operations.sh)
4. ✅ Port show.txt (DONE - basic_operations.sh + numeric shorthand)
5. ✅ Port update.txt (DONE - basic_operations.sh)
6. ✅ Port close.txt (DONE - basic_operations.sh)
7. ✅ Port dep_add.txt (DONE - basic_operations.sh)
8. ✅ Port blocked.txt (DONE - basic_operations.sh)
9. ✅ Port ready.txt (DONE - basic_operations.sh)
10. ✅ Port help.txt (DONE - help_version.sh)
11. ✅ Port version.txt (DONE - help_version.sh)
12. ✅ Add reopen tests (DONE - basic_operations.sh Test 11)

**Approach**: Extend `tests/basic_operations.sh` or create parallel script tests.

### Phase 2: Unit Test Coverage (P1)
**Goal**: Test edge cases and internal logic

**Tests to Add**:
1. **format.rs unit tests**:
   - ✅ Roundtrip already tested
   - ❌ Sanitization edge cases (section headers in content)
   - ❌ Special characters in YAML frontmatter
   - ❌ Malformed markdown handling
   - ❌ Empty fields serialization

2. **types.rs unit tests**:
   - ❌ Dependency type parsing
   - ❌ Status/IssueType enum parsing
   - ❌ Blocking dependency detection
   - ❌ Invalid enum values

3. **storage.rs unit tests**:
   - ❌ Prefix inference logic
   - ❌ Config.yaml validation
   - ❌ .gitignore management
   - ❌ Nonexistent issue handling
   - ❌ Dependency graph queries

4. **lock.rs unit tests**:
   - ✅ Basic acquire/release tested
   - ❌ Contention scenarios
   - ❌ Stale lock handling
   - ❌ PID validation

**Approach**: Add Rust unit tests in each module's `#[cfg(test)]` section.

### Phase 3: Integration Tests (P2)
**Goal**: Test complex workflows and error scenarios

**Scenarios**:
1. ❌ Concurrent operations (multi-process locking)
2. ❌ Large dependency graphs
3. ❌ Circular dependency detection
4. ❌ Invalid markdown file recovery
5. ❌ Migration from empty .beads (first init)
6. ❌ Reopening with dependencies
7. ❌ Filtering combinations (status + priority + type + assignee)

**Approach**: Create additional shell scripts in `tests/` directory.

### Phase 4: MCP Integration Tests (P0 - CRITICAL)
**Goal**: Prevent regressions in MCP compatibility (see minibeads-13)

**Tests Needed**:
1. ❌ JSON schema validation (dependencies/dependents fields)
2. ❌ All MCP operations with varied inputs
3. ❌ Special characters in arguments
4. ❌ Multiline content via MCP
5. ❌ Error message format verification

**Approach**: Create Python or Rust tests that invoke bd CLI as MCP does.

### Phase 5: Future Feature Tests (P3)
**Goal**: Test coverage for planned features

**When Implemented**:
- minibeads-9 (Comments): Port comments_test.go
- minibeads-10 (Events): Port events_test.go
- minibeads-11 (Export): Port export_test.go
- minibeads-12 (Sync): Create new sync tests

## Testing Framework Decisions

### Keep Shell Scripts
**Pros**:
- Simple to write and maintain
- Easy to debug (just run bash script)
- Cross-platform (works on Linux/macOS/Windows with bash)
- Human-readable test cases
- Upstream uses similar approach (script tests)

**Cons**:
- Less structured than Rust tests
- No type safety
- Harder to test Rust internals

**Decision**: ✅ Keep shell scripts for e2e tests

### Add Rust Unit Tests
**Pros**:
- Test internal logic directly
- Type-safe test data
- Integrated with cargo test
- Fast execution

**Cons**:
- More verbose than shell
- Can't test CLI interface directly

**Decision**: ✅ Add Rust unit tests for modules

### Consider rsc.io/script Port?
**Pros**:
- Exact parity with upstream format
- Portable test definitions
- Nice assertion syntax

**Cons**:
- Additional dependency (Go or Rust port)
- Overkill for current needs
- Shell scripts work fine

**Decision**: ❌ Not needed yet, revisit if test suite grows

## Success Criteria

### Phase 1 Complete When:
- ✅ All implemented commands have script test coverage
- ✅ Tests run in CI
- ✅ Tests pass on Linux, macOS, Windows (via CI matrix)
- ✅ help and version commands tested

**Status: ✅ PHASE 1 COMPLETE** (64 test assertions across 2 shell scripts)

### Phase 2 Complete When:
- ❌ >80% unit test coverage of core modules
- ❌ All enum parsers tested
- ❌ All storage operations tested
- ❌ Edge cases from minibeads-13 have tests

### Phase 4 Complete When:
- ❌ MCP schema bugs from minibeads-13 have regression tests
- ❌ All 14 MCP operations have automated tests
- ❌ No manual MCP testing required for releases

## Open Questions

1. **Test Data Management**: Should we commit test .beads directories or generate fresh each time?
   - Current: Generate fresh (tests/basic_operations.sh creates temp dir)
   - Upstream: Mix of both
   - Recommendation: Generate fresh for now

2. **Test Fixtures**: Should we create reusable test issue sets?
   - Upstream: Uses helper functions to create common scenarios
   - Recommendation: Yes, add helpers_test.go equivalent in Rust

3. **Performance Testing**: Do we need benchmarks?
   - Current: No benchmarks
   - Upstream: Has benchmarks for compaction, cycles
   - Recommendation: Add simple benchmarks in Phase 3

4. **Compatibility Testing**: Test against upstream bd?
   - Question: Should we verify compatibility by testing both implementations?
   - Recommendation: Yes, but only for export/import once minibeads-11/12 done

## Implementation Notes

### Test Organization
```
tests/
├── e2e_tests.rs         # Rust harness with auto-discovery ✅
├── basic_operations.sh  # Core commands (36 assertions) ✅
├── help_version.sh      # Help/version tests (28 assertions) ✅
├── edge_cases.sh        # Error scenarios (TODO)
├── dependencies.sh      # Complex dep graphs (TODO)
└── mcp_integration.sh   # MCP compatibility (TODO)

src/
├── format.rs           # Add unit tests (3 existing, more needed)
├── types.rs            # Add unit tests (TODO)
├── storage.rs          # Add unit tests (TODO)
├── lock.rs             # Extend tests (1 existing)
└── main.rs             # CLI integration tests?
```

### Example Test Additions

**help_version.sh**:
```bash
#!/bin/bash
## Test help and version commands
bd --help
assert_contains "$OUTPUT" "Minibeads"

bd version
assert_contains "$OUTPUT" "0.9.0"

bd quickstart
assert_contains "$OUTPUT" "GETTING STARTED"
```

**types.rs unit tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dependency_type_parsing() {
        assert_eq!("blocks".parse::<DependencyType>().unwrap(), DependencyType::Blocks);
        assert!("invalid".parse::<DependencyType>().is_err());
    }
}
```

## Next Steps

1. Create minibeads-15: Phase 1 - Help/version test coverage
2. Create minibeads-16: Phase 2 - Unit test coverage
3. Create minibeads-17: Phase 4 - MCP integration tests
4. Update CI to run all test phases
5. Add coverage reporting to CI
