# TODO: Complete JSONL Import Test Integration

## Overview
This document tracks the remaining work to integrate JSONL import testing into the random-actions stress test.

## Current Status (Commit: fbb35c6)
- ✅ Added `--test-import` flag to `random-actions` command
- ✅ Flag defaults to `true` (import test runs by default when using upstream)
- ✅ Parameter threaded through to `run_random_actions()`
- ✅ Code compiles and basic infrastructure in place

## Remaining Work

### 1. Implement Import Validation in `run_test()` Function
**Location**: `src/bin/test_minibeads.rs`, `run_test()` function (around line 530-633)

**What needs to be done**:
- Add `test_import: bool` parameter to `run_test()` signature
- Add logic after executor.execute_sequence() when `test_import == true && implementation == upstream`
- Export upstream database to JSONL
- Parse JSONL into minibeads reference format
- Verify parsed issues match reference interpreter state

**Pseudocode**:
```rust
fn run_test(
    seed: u64,
    actions_per_iter: usize,
    implementation: &str,
    binary_path: &str,
    work_dir: &Path,
    logger: &Logger,
    test_import: bool,  // NEW PARAMETER
) -> Result<()> {
    // ... existing code ...

    // After running actions against upstream
    let results = executor.execute_sequence(&actions)?;

    // NEW: If testing import and using upstream
    if test_import && implementation == "upstream" {
        logger.log("🔄 Testing JSONL import...".to_string());

        // 1. Export upstream database to JSONL
        let export_output = std::process::Command::new(binary_path)
            .current_dir(work_dir)
            .args(["export", "-o", "issues.jsonl"])
            .output()?;

        if !export_output.status.success() {
            // Handle export failure
        }

        // 2. Read and parse JSONL
        let jsonl_path = work_dir.join("issues.jsonl");
        let jsonl_content = fs::read_to_string(&jsonl_path)?;
        let parsed_issues = parse_jsonl_to_reference_issues(&jsonl_content)?;

        // 3. Compare with reference interpreter
        verify_issues_match(&reference.issues, &parsed_issues, logger)?;

        logger.log("✅ JSONL import verified".to_string());
    }

    Ok(())
}
```

### 2. Add Helper Functions

#### `parse_jsonl_to_reference_issues()`
**Purpose**: Parse JSONL string into HashMap of ReferenceIssue structs

**Can reuse from**: `tests/random_import_upstream_json.rs`, function `parse_jsonl_to_reference_issue()`

**Location**: Add to `src/bin/test_minibeads.rs` as a helper function

```rust
fn parse_jsonl_to_reference_issues(
    jsonl: &str
) -> Result<HashMap<String, ReferenceIssue>> {
    // Reuse logic from tests/random_import_upstream_json.rs
}
```

#### `verify_issues_match()`
**Purpose**: Compare two sets of issues and report differences

**Should check**:
- Same number of issues
- Same issue IDs (may differ if hash-based)
- Same titles, descriptions, priorities, types, statuses
- Same dependency relationships
- Use colorful diff output (similar-asserts) if differences found

```rust
fn verify_issues_match(
    reference: &HashMap<String, ReferenceIssue>,
    imported: &HashMap<String, ReferenceIssue>,
    logger: &Logger,
) -> Result<()> {
    // Compare and report differences
}
```

### 3. Update All Call Sites

**Functions that call `run_test()` need to pass `test_import` parameter**:

1. **`run_random_actions()`** - Single iteration mode (line ~240)
   ```rust
   run_test(seed, actions_per_iter, &implementation_str, &binary_path, &work_dir, &logger, test_import)?;
   ```

2. **Time-based stress test** (line ~300)
   ```rust
   run_test(seed, actions_per_iter, &implementation_str, &binary_path, &work_dir, &logger, test_import)?;
   ```

3. **Parallel stress test workers** (line ~450)
   ```rust
   run_test(seed, actions_per_iter, &implementation_str, &binary_path, &work_dir, &buffering_logger, test_import)?;
   ```

### 4. Handle Edge Cases

- **Upstream binary not found**: Skip import test gracefully
- **Export fails**: Report clear error message
- **JSONL parse fails**: Show which line/field failed
- **Mismatched issues**: Use colorful diff to show differences
- **Hash-based IDs**: Don't directly compare IDs, compare by content

### 5. Update Documentation

- Update `--help` text to clearly explain when import test runs
- Update README.md with examples of using `--test-import` flag
- Document expected behavior in various scenarios

## Testing Plan

After implementation, test these scenarios:

1. **Default behavior**: `test_minibeads random-actions --impl upstream`
   - Should run import test automatically

2. **Disable import**: `test_minibeads random-actions --impl upstream --test-import=false`
   - Should skip import test

3. **Minibeads mode**: `test_minibeads random-actions --impl minibeads --test-import=true`
   - Import flag should have no effect (only applies to upstream)

4. **Parallel mode**: `test_minibeads random-actions --impl upstream --seconds 5 --parallel`
   - Import test should run for each iteration

5. **Failure cases**:
   - Upstream binary missing
   - Export fails
   - JSONL parse error
   - Issue mismatch

## Files to Modify

1. `src/bin/test_minibeads.rs` - Main implementation
2. Consider moving JSONL parsing to `src/lib.rs` for reuse
3. Update `tests/random_import_upstream_json.rs` - May become redundant

## Potential Future Enhancements

- Add `--export-jsonl` flag to export even when not testing import
- Add `--import-file` flag to test importing a specific JSONL file
- Support testing minibeads→upstream direction (export from minibeads, import to upstream)
- Add performance metrics for import operations

## Notes

- The reference interpreter currently assumes sequential IDs, but upstream uses hash-based IDs
- Need to be careful about ID comparison - compare by content, not by ID string
- JSONL field is `issue_type` not `type` (discovered in previous testing)
- Import test should not affect reference interpreter state (read-only verification)
