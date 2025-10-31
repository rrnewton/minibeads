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
updated_at: 2025-10-31T04:33:23.583472330+00:00
---

# Description

## Overview

Research upstream beads testing approach and create porting plan for minibeads. The goal is to ensure minibeads maintains compatibility with upstream while leveraging our simpler architecture.

## Current Status (as of 2025-10-31_#72(ddc372b))

### Minibeads Testing (Current)
- ✅ Unit tests: 3 tests in src/ (format, lock, issue_roundtrip)
- ✅ E2E tests: 3 shell scripts with 104 total assertions
  - basic_operations.sh: 38 assertions (core commands: create, list, show, update, close, reopen, dep)
  - help_version.sh: 29 assertions (help, version, quickstart validation)
  - export_interop.sh: 37 assertions (export functionality, JSONL format)
- ✅ Test harness: Rust integration in tests/e2e_tests.rs with auto-discovery
- ✅ CI: GitHub Actions with full validation on Linux, macOS, Windows
- ✅ Phase 1 COMPLETE: All core commands have test coverage
- ⚠️ Coverage: Need Phase 2 unit tests for edge cases

### Upstream Beads Testing (Analyzed)
- **78 Go test files** in total
- **17 script tests** (rsc.io/script format)
- **Test categories**: Unit, integration, script-based, benchmarks

## Test Porting Plan

### Phase 1: Core Command Coverage (P0) ✅ COMPLETE
**Status**: ✅ PHASE 1 COMPLETE (104 test assertions across 3 shell scripts)

All implemented commands now have script test coverage:
1. ✅ Port init.txt (DONE - basic_operations.sh)
2. ✅ Port create.txt (DONE - basic_operations.sh)
3. ✅ Port list.txt (DONE - basic_operations.sh)
4. ✅ Port show.txt (DONE - basic_operations.sh + numeric shorthand + multi-issue)
5. ✅ Port update.txt (DONE - basic_operations.sh + bulk operations)
6. ✅ Port close.txt (DONE - basic_operations.sh + bulk operations)
7. ✅ Port dep_add.txt (DONE - basic_operations.sh)
8. ✅ Port dep_remove.txt (DONE - basic_operations.sh)
9. ✅ Port dep_tree.txt (DONE - basic_operations.sh)
10. ✅ Port dep_cycles.txt (DONE - basic_operations.sh)
11. ✅ Port blocked.txt (DONE - basic_operations.sh)
12. ✅ Port ready.txt (DONE - basic_operations.sh)
13. ✅ Port help.txt (DONE - help_version.sh)
14. ✅ Port version.txt (DONE - help_version.sh)
15. ✅ Port export.txt (DONE - export_interop.sh)
16. ✅ Add reopen tests (DONE - basic_operations.sh)

### Phase 2: Unit Test Coverage (P1) - IN PROGRESS
**Goal**: Test edge cases and internal logic

**Tests to Add**:
1. **format.rs unit tests**:
   - ✅ Roundtrip already tested (issue_roundtrip)
   - ✅ Sanitization tested (sanitize_headers)
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
3. ❌ Circular dependency detection (partially covered in basic_operations.sh)
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
- minibeads-11 (Export): ✅ DONE - export_interop.sh
- minibeads-12 (Sync): Create new sync tests
- minibeads-16 (Import): Port import_test.go when implemented

## Success Criteria

### Phase 1 Complete ✅
- ✅ All implemented commands have script test coverage
- ✅ Tests run in CI
- ✅ Tests pass on Linux, macOS, Windows (via CI matrix)
- ✅ Help and version commands tested
- ✅ Export command tested

**Status: ✅ PHASE 1 COMPLETE** (104 test assertions across 3 shell scripts)

### Phase 2 Complete When:
- ❌ >80% unit test coverage of core modules
- ❌ All enum parsers tested
- ❌ All storage operations tested
- ❌ Edge cases from minibeads-13 have tests

### Phase 4 Complete When:
- ❌ MCP schema bugs from minibeads-13 have regression tests
- ❌ All MCP operations have automated tests
- ❌ No manual MCP testing required for releases

## Implementation Notes

### Test Organization (Current)
```
tests/
├── e2e_tests.rs           # Rust harness with auto-discovery ✅
├── basic_operations.sh    # Core commands (38 assertions) ✅
├── help_version.sh        # Help/version tests (29 assertions) ✅
├── export_interop.sh      # Export/JSONL tests (37 assertions) ✅
├── edge_cases.sh          # Error scenarios (TODO)
├── dependencies.sh        # Complex dep graphs (TODO)
└── mcp_integration.sh     # MCP compatibility (TODO)

src/
├── format.rs              # 3 unit tests ✅
├── types.rs               # Add unit tests (TODO)
├── storage.rs             # Add unit tests (TODO)
├── lock.rs                # 1 unit test ✅
└── main.rs                # CLI integration tests?
```

## Next Steps

1. ❌ Create unit tests for types.rs (enum parsing, dependency logic)
2. ❌ Create unit tests for storage.rs (prefix inference, config validation)
3. ❌ Create Phase 4 MCP integration tests
4. ❌ Add coverage reporting to CI
5. ✅ Update test counts in tracking issues (minibeads-3, minibeads-14)

---

**Checked up-to-date as of 2025-10-31_#72(ddc372b)**

Test counts verified against actual test files. Phase 1 completion status confirmed.
All references to architecture and system components validated against current codebase.

Dependencies:
  minibeads-3 (parent-child)
