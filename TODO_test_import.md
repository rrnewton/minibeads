# JSONL Import Test Integration - COMPLETE

## Status
✅ **COMPLETE** - Implementation finished, ready for testing

## Overview
Successfully integrated JSONL import testing into the random-actions stress test. When testing upstream, the test now exports the database to JSONL, verifies it can be parsed, and confirms minibeads can import it correctly.

## What Was Implemented

### 1. Command-Line Interface
- ✅ Added `--test-import` flag to `random-actions` command
- ✅ Flag defaults to `true` (import test runs by default when using upstream)
- ✅ Flag only affects upstream mode (ignored for minibeads)
- ✅ Help text explains when the import test applies

### 2. Parameter Threading
- ✅ `test_import` parameter threaded through entire call chain:
  - `main()` → `run_random_actions()`
  - `run_random_actions()` → `run_parallel_stress_test()`
  - `run_parallel_stress_test()` → `run_test()` (in worker threads)
  - `run_random_actions()` → `run_test()` (direct calls in time-based and iteration modes)

### 3. Import Validation Logic
- ✅ Implemented `test_jsonl_import()` function in `src/bin/test_minibeads.rs`
- ✅ Called from `run_test()` when `use_no_db && test_import`
- ✅ Full export → parse → import → verify cycle

### 4. test_jsonl_import() Implementation Details
Located at line ~1267 in `src/bin/test_minibeads.rs`, performs:

1. **Find minibeads binary** - Locates debug build in same directory as test_minibeads
2. **Export upstream database** - Uses `bd export -o export.jsonl` command
3. **Parse exported JSONL** - Reads and parses each line using existing `parse_jsonl_to_reference_issue()`
4. **Verify export** - Compares parsed issues with reference interpreter state using `compare_issue_states()`
5. **Create fresh import directory** - Uses tempfile::tempdir() for clean environment
6. **Initialize minibeads** - Runs `bd init --prefix <PREFIX>` with same prefix as reference
7. **Copy JSONL** - Copies export file to import directory
8. **Import to minibeads** - Runs `bd import -i export.jsonl`
9. **Verify import** - Uses existing `verify_minibeads_state()` to check markdown files match reference

### 5. Reused Existing Functions
- `parse_jsonl_to_reference_issue()` - Already existed for parsing JSONL lines
- `compare_issue_states()` - Already existed for comparing issue sets with colorful diffs
- `verify_minibeads_state()` - Already existed for verifying markdown file contents
- All reused functions were already thoroughly tested

### 6. Error Handling
- ✅ Checks minibeads binary exists before attempting import
- ✅ Validates export command succeeds
- ✅ Handles JSONL parse errors with line numbers
- ✅ Reports init failures with stderr
- ✅ Reports import failures with stderr
- ✅ Uses colorful diffs (similar-asserts) for issue mismatches

## Testing Scenarios

After implementation, test these scenarios:

1. **Default upstream** - `cargo run --bin test_minibeads -- random-actions --impl upstream`
   - Should run import test automatically
   - Should see "📦 Testing JSONL import capability..." message

2. **Disable import** - `cargo run --bin test_minibeads -- random-actions --impl upstream --test-import=false`
   - Should skip import test
   - No import messages

3. **Minibeads mode** - `cargo run --bin test_minibeads -- random-actions --impl minibeads --test-import=true`
   - Import flag ignored (only applies to upstream)
   - No import test runs

4. **Parallel mode** - `cargo run --bin test_minibeads -- random-actions --impl upstream --seconds 5 --parallel`
   - Import test runs for each worker iteration
   - May slow down throughput

5. **Verbose mode** - `cargo run --bin test_minibeads -- random-actions --impl upstream --verbose`
   - Shows detailed import progress:
     - Exporting, parsing, verifying export
     - Creating import directory, initializing, importing
     - Verifying import

## Files Modified

1. **src/bin/test_minibeads.rs**
   - Added `test_import` field to `RandomActions` command struct (line 136)
   - Added `test_import` parameter to:
     - `run_random_actions()` (line 187)
     - `run_parallel_stress_test()` (line 440)
     - `run_test()` (line 604)
   - Added import test call in `run_test()` (line 702-705)
   - Implemented `test_jsonl_import()` function (line 1267-1391)
   - Updated all 3 call sites to `run_test()` to pass parameter

2. **TODO_test_import.md** (this file)
   - Updated to reflect completion status

## Edge Cases Handled

- ✅ **No issues created** - Gracefully handles empty database (both export and import)
- ✅ **Minibeads binary missing** - Clear error with build instructions
- ✅ **Export fails** - Reports stderr and stops
- ✅ **JSONL parse error** - Shows line number where parse failed
- ✅ **Hash-based IDs** - Compares by content, not by ID string format
- ✅ **Mismatched issues** - Colorful diff shows exact field differences
- ✅ **Different dependency types** - Compares dependency types correctly

## Known Limitations

1. **Parallel mode performance** - Import test adds significant overhead to each iteration
   - Consider using `--test-import=false` for maximum throughput testing
   - Or only enable for final validation runs

2. **Temporary directory cleanup** - TempDirs are auto-cleaned, but failed tests may leave artifacts

3. **Binary discovery** - Assumes standard cargo build layout
   - minibeads binary must be in same directory as test_minibeads

## Potential Future Enhancements

- [ ] Add `--export-jsonl` flag to export without importing (for manual inspection)
- [ ] Add `--import-file <PATH>` flag to test importing a specific JSONL file
- [ ] Support bidirectional testing (minibeads → upstream)
- [ ] Add import performance metrics (time to export, parse, import)
- [ ] Skip import test for iterations with no issues created (optimization)
- [ ] Cache minibeads binary path to avoid repeated discovery

## Verification Checklist

Before considering this truly complete, verify:

- [x] Code compiles without warnings
- [ ] `make validate` passes (includes tests, fmt, clippy)
- [ ] Default upstream mode runs import test
- [ ] Import test can be disabled with `--test-import=false`
- [ ] Verbose mode shows import progress
- [ ] Parallel mode works with import test
- [ ] Import test passes with real upstream execution

## Notes

- Import test only runs for upstream (`use_no_db == true`)
- Reference interpreter uses "test" prefix, import test preserves this
- JSONL field is `issue_type` not `type` (matches upstream format)
- Import verification uses same logic as normal state verification
- All JSONL parsing reuses existing, tested functions
