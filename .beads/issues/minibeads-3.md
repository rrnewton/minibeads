---
title: Testing and validation tracking
status: in_progress
priority: 1
issue_type: epic
created_at: 2025-10-30T13:22:12.880731360+00:00
updated_at: 2026-09-07T20:37:27.453290803+00:00
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
- [x] Upstream JSONL import compatibility test (tests/random_import_upstream_json.rs)
  - Verifies minibeads can correctly parse upstream beads JSONL format
  - Tests parsing of all fields: id, title, description, status, priority, type, dependencies
  - Validates dependency relationships and types
  - Uses sample JSONL matching upstream format

## Related Issues
- minibeads-5: Fixed serialization bug and validation (CLOSED)
- minibeads-14: Test porting plan - adapting upstream beads tests for minibeads
- minibeads-20: Deep verification with reference interpreter (CLOSED)

## minibeads-35: bidirectional upstream sync stress test failing (pre-existing)
make stress-test's test_sync_stress fails deterministically (seed 12345) with an issue-count mismatch (expected 45, got 30). Confirmed pre-existing as of af00710. Not caught by make validate. See minibeads-35.


## Update 2026-07-11_#197(293601f)
- Merged main (0.13.2, crates.io-publishing prep) into integration to unblock
  promoting integration -> main; resolved conflicts in Cargo.toml/lock,
  src/main.rs (CLI help text), and README.md (mb/bd naming consistency).
- Fixed `mb ready` to have no default result limit (previously capped at 10,
  which had confused agents relying on `mb ready` to see the full ready set).
  `-n`/`--limit` still works when explicitly passed. Added
  storage::ready_tests (no_limit_returns_all_ready_issues,
  explicit_limit_truncates) as regression coverage.
- Found flaky test: minibeads-36
  (github::tests::github_import_creates_only_unlinked_issues intermittently
  fails with "Text file busy" under parallel `cargo test`).

2026-09-07_#215(9437f50c1a): minibeads-37 sync draft adds 24 prose tests, 27 mocked-GitHub sync tests, and 1 CLI selection test. Debug suite totals: 64 library + 137 binary + 7 shell e2e = 208 passing tests. Coverage includes missing/corrupt/legacy ancestors, conflicts without mutation, label/ID/since intersections, partial batches, pre/post-write remote changes, and stale-ancestor paragraph replay. All new tests run through the existing validation target and CI. Full validation is NOT green: existing format differences at src/format.rs and tests/migration_description_truncation.rs, plus existing strict-Clippy too_many_arguments and useless_vec warnings. New/changed-file formatting passes; Clippy passes with only those two existing categories allowed. Live GitHub CI remains unavailable. No duplicate tracking issues created.

2026-09-07_#215(9437f50c1a), continuation update superseding prior formatting/lint failures: make -o purge validate now passes all build/test/fmt/Clippy gates (208 routine tests), and strict all-target/all-feature Clippy passes without lint allowances. One standalone migration test additionally passes. Gate fixes are formatting-only plus an array instead of an unnecessary test Vec and named CommentDeletionOptions replacing existing positional flags. The private-database purge remains intentionally skipped. GitHub live CI remains unverified; minibeads-37 tracks the remaining landing work.

2026-09-07_#215(9437f50c1a): Remote PR15–24 stack landing verified at merge 19fc71cfc1. Corrected stack totals: 203 tests on each of stable and Rust 1.87, strict Clippy and audit clean; all seven final-head CI jobs green. Earlier 208+1 results here describe only the frozen/unapproved prototype, not the landed stack. See minibeads-37 for the human plan-review hold.

2026-09-07_#215(9437f50c1a): Final remote main 6e30bdc7a8 passes all seven CI jobs (34159742387). A post-merge macOS ENOENT stress-harness race was corrected by building shared debug tools once; five deterministic initialization regressions now run in make validate and CI. Routine suite 208 passed; native harness 10 passed on stable and Rust 1.87 (five overlap routine checks). This supersedes the earlier post-landing CI failure; no release iteration or sync-overhaul implementation was performed.
