---
title: Testing and validation tracking
status: in_progress
priority: 1
issue_type: epic
created_at: 2025-10-30T13:22:12.880731360+00:00
updated_at: 2025-10-30T13:56:02.945405078+00:00
---

# Description

Track testing improvements for minibeads.

## Current Status
- ✅ 3 unit tests passing (format, lock)
- ✅ Makefile with validate target created
- ✅ Clippy checks passing
- ✅ E2E test infrastructure complete with auto-discovery
- ✅ Shell-based e2e tests: 64 total assertions
  - basic_operations.sh: 36 assertions (core commands)
  - help_version.sh: 28 assertions (help/version/quickstart)
- ✅ Rust test harness for automatic shell test discovery
- ✅ GitHub Actions CI/CD pipeline configured
- ✅ Phase 1 test porting complete (see minibeads-14)
- 🔲 Need Phase 2 unit tests for edge cases
- 🔲 Need Phase 4 MCP integration tests

## TODO
- [ ] Add more unit tests for storage operations
- [ ] Add more e2e test scenarios (concurrent access, error handling, edge cases)
- [ ] Add property-based tests for markdown format
- [ ] Add code coverage reporting
- [ ] Implement test porting plan from upstream beads (see minibeads-14)

## Completed
- [x] Created tests/basic_operations.sh with 36 test assertions
- [x] Created tests/help_version.sh with 28 test assertions
- [x] Created tests/e2e_tests.rs Rust harness with auto-discovery
- [x] Integrated with cargo test
- [x] All tests passing in make validate
- [x] Phase 1 test porting complete (all implemented commands covered)
- [x] Added tests for numeric shorthand in bd show
- [x] Added tests for multi-issue bd show
- [x] GitHub Actions CI with multiple jobs:
  - Test suite (make validate)
  - Code coverage (tarpaulin)
  - Linting (fmt + clippy)
  - Security audit (cargo-audit)
  - Cross-platform testing (Ubuntu, macOS, Windows)

## Related Issues
- minibeads-5: Fixed serialization bug and validation (CLOSED)
- minibeads-14: Test porting plan - adapting upstream beads tests for minibeads
