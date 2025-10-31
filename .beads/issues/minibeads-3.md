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

## Current Status (as of 2025-10-31_#80(570324f8fe))
- ✅ 3 unit tests passing (format, lock, issue_roundtrip)
- ✅ Makefile with validate target created
- ✅ Clippy checks passing with -D warnings
- ✅ E2E test infrastructure complete with auto-discovery
- ✅ Shell-based e2e tests: 104 total assertions across 3 test files
  - basic_operations.sh: 38 assertions (core commands: create, list, show, update, close, reopen, dep)
  - help_version.sh: 29 assertions (help, version, quickstart validation)
  - export_interop.sh: 37 assertions (export functionality, JSONL format)
- ✅ Rust test harness for automatic shell test discovery
- ✅ Random property-based test generator (src/beads_generator.rs + src/bin/test_minibeads.rs)
  - Renamed to `test_minibeads random-actions` subcommand
  - `--impl minibeads` (default) or `--impl upstream` for testing both implementations
  - Upstream testing uses `--no-db` flag automatically
  - Sequential numbering verification with expected_id checking
  - State tracking for valid action sequence generation
  - Deterministic testing with seed support for reproducibility
  - Verbose output mode with concise Display format
  - Distinguishes expected failures from critical errors
  - DRY refactoring with build_command() helper method
  - **Deep verification with reference interpreter (minibeads-20)**:
    - ReferenceInterpreter maintains in-memory HashMap as "golden state"
    - Recursive .beads directory walk with file size reporting
    - Config.yaml validation (prefix matching)
    - Full state comparison for minibeads (markdown) and upstream (JSONL)
    - Field-by-field verification: title, status, priority, issue_type, dependencies
    - Detailed error messages showing mismatches
- ✅ GitHub Actions CI/CD pipeline configured
  - Multi-platform testing (Linux, macOS, Windows)
  - All platforms passing as of 2025-10-31
- ✅ Phase 1 test porting complete (see minibeads-14)
- 🔲 Need Phase 2 unit tests for edge cases
- 🔲 Need Phase 4 MCP integration tests

## TODO
- [ ] Add more unit tests for storage operations
- [ ] Add more e2e test scenarios (concurrent access, error handling, edge cases)
- [ ] Add colorful diff reporting using similar-asserts for deep verification
- [ ] Add code coverage reporting
- [ ] Implement test porting plan from upstream beads (see minibeads-14)

## Completed
- [x] Created tests/basic_operations.sh with 38 test assertions
- [x] Created tests/help_version.sh with 29 test assertions
- [x] Created tests/export_interop.sh with 37 test assertions
- [x] Created tests/e2e_tests.rs Rust harness with auto-discovery
- [x] Integrated with cargo test
- [x] All tests passing in make validate (104 shell assertions + 3 unit tests)
- [x] Phase 1 test porting complete (all implemented commands covered)
- [x] Added tests for numeric shorthand in bd show
- [x] Added tests for multi-issue bd show
- [x] Added comprehensive export/JSONL interop tests
- [x] Random property-based test generator for beads commands (commit #76/209a9ce)
  - BeadsAction enum with all command types
  - ActionGenerator with weighted random generation
  - ActionExecutor with sequential numbering verification
  - CLI with --seed, --seed-from-entropy, --iters, --verbose flags
  - Display trait for concise action summaries
- [x] GitHub Actions CI with multiple jobs:
  - Test suite (make validate)
  - Code coverage (tarpaulin)
  - Linting (fmt + clippy)
  - Security audit (cargo-audit)
  - Cross-platform testing (Ubuntu, macOS, Windows)

## Related Issues
- minibeads-5: Fixed serialization bug and validation (CLOSED)
- minibeads-14: Test porting plan - adapting upstream beads tests for minibeads
- minibeads-20: Deep verification with reference interpreter (CLOSED)
