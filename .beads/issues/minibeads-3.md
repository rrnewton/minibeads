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

## Current Status (as of 2025-10-31_#84(27525bf))
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
    - **Colorful diff output using similar-asserts**:
      - Visual diffs showing expected vs actual with - and + markers
      - Issue count mismatch shows missing vs extra issues
      - Each field mismatch displays clear headers and color-coded differences
      - Dependencies shown as sorted vectors for consistent comparison
  - **Time-based stress testing with --seconds flag**:
    - Duration-based testing that runs until time expires or first failure
    - Progress reporting with iteration count and elapsed time
    - Early exit on first failure with reproducible seed
  - **Parallel stress testing with --parallel flag**:
    - Multi-threaded execution using std::thread with configurable worker count
    - Defaults to number of system cores (64 workers on test system)
    - Thread-safe coordination using Arc<AtomicBool> and Arc<AtomicU64>
    - Separate seed space per worker (offset by worker_id * 1000000)
    - Early exit across all workers on first failure
    - Performance: **1232 iterations in 5 seconds** with 64 workers (240.8 iters/sec)
    - Speedup: **11.4x faster** than single-threaded (108 iters in 5 seconds)
- ✅ GitHub Actions CI/CD pipeline configured
  - Multi-platform testing (Linux, macOS, Windows)
  - All platforms passing as of 2025-10-31
- ✅ Phase 1 test porting complete (see minibeads-14)
- 🔲 Need Phase 2 unit tests for edge cases
- 🔲 Need Phase 4 MCP integration tests

## TODO
- [ ] Add more unit tests for storage operations
- [ ] Add more e2e test scenarios (concurrent access, error handling, edge cases)
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
- [x] Colorful diff reporting using similar-asserts for deep verification (commit #82/ccbda9f23c)
  - Integrated similar-asserts into compare_issue_states()
  - Visual diffs with - and + markers for expected vs actual
  - Enhanced error reporting for issue count mismatches
  - Field-by-field comparison with clear headers and color-coded output
- [x] Time-based and parallel stress testing (commit #84/27525bf)
  - Added --seconds flag for duration-based testing
  - Added --parallel flag with optional core count
  - Thread-safe parallel execution with atomic coordination
  - 11.4x speedup with 64 workers vs single-threaded

## Related Issues
- minibeads-5: Fixed serialization bug and validation (CLOSED)
- minibeads-14: Test porting plan - adapting upstream beads tests for minibeads
- minibeads-20: Deep verification with reference interpreter (CLOSED)
