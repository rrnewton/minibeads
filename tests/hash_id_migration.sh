#!/bin/bash
# E2E test for hash ID migration
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="hash_id_migration"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BD_BIN="$WORKSPACE_ROOT/target/debug/bd"
TEST_DIR="$WORKSPACE_ROOT/scratch/e2e_${TEST_NAME}_$$"

# Counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Cleanup function
cleanup() {
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
}

# Error handler
error_handler() {
    echo -e "${RED}✗ Test failed at line $1${NC}" >&2
    echo -e "${RED}Test directory preserved: $TEST_DIR${NC}" >&2
    exit 1
}

# Success message
success() {
    echo -e "${GREEN}✓ $1${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

# Fail message
fail() {
    echo -e "${RED}✗ $1${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

# Assert equals
assert_equals() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local expected="$1"
    local actual="$2"
    local message="${3:-Assertion failed}"

    if [ "$expected" = "$actual" ]; then
        success "$message"
    else
        fail "$message (expected: '$expected', got: '$actual')"
        return 1
    fi
}

# Assert contains
assert_contains() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local haystack="$1"
    local needle="$2"
    local message="${3:-Assertion failed}"

    if echo "$haystack" | grep -qF -- "$needle"; then
        success "$message"
    else
        fail "$message (expected to find: '$needle' in output)"
        return 1
    fi
}

# Assert file exists
assert_file_exists() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local file="$1"
    local message="${2:-File should exist}"

    if [ -f "$file" ]; then
        success "$message"
    else
        fail "$message (file not found: $file)"
        return 1
    fi
}

# Assert file does not exist
assert_file_not_exists() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local file="$1"
    local message="${2:-File should not exist}"

    if [ ! -f "$file" ]; then
        success "$message"
    else
        fail "$message (file exists: $file)"
        return 1
    fi
}

# Set up error handling
trap 'error_handler $LINENO' ERR

echo "=========================================="
echo "Running E2E Test: $TEST_NAME"
echo "=========================================="
echo ""

# Ensure bd binary exists
if [ ! -f "$BD_BIN" ]; then
    echo "Building bd binary..."
    cd "$WORKSPACE_ROOT"
    cargo build
    echo ""
fi

# Create test directory
echo "Creating test directory: $TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Test 1: Initialize database with numeric IDs
echo -e "\n${YELLOW}Test 1: Initialize database (default: numeric IDs)${NC}"
OUTPUT=$("$BD_BIN" init --prefix test 2>&1)
assert_contains "$OUTPUT" "Initialized beads database with prefix: test" "Initialize should succeed"
# Check config-minibeads.yaml has mb-hash-ids: false
CONFIG_VALUE=$(grep "mb-hash-ids:" .beads/config-minibeads.yaml | awk '{print $2}' | tr -d "'\"")
assert_equals "false" "$CONFIG_VALUE" "mb-hash-ids should default to false"

# Test 2: Create issues with numeric IDs
echo -e "\n${YELLOW}Test 2: Create issues with numeric IDs${NC}"
"$BD_BIN" create "First issue" -d "This is the first test issue" >/dev/null 2>&1
"$BD_BIN" create "Second issue" -d "This depends on the first" --deps test-1 >/dev/null 2>&1
"$BD_BIN" create "Third issue" -d "This also depends on the first" --deps test-1 >/dev/null 2>&1

# Verify numeric IDs were created
assert_file_exists ".beads/issues/test-1.md" "test-1.md should exist"
assert_file_exists ".beads/issues/test-2.md" "test-2.md should exist"
assert_file_exists ".beads/issues/test-3.md" "test-3.md should exist"

# Test 3: Dry-run migration
echo -e "\n${YELLOW}Test 3: Dry-run migration${NC}"
OUTPUT=$("$BD_BIN" mb-migrate --dry-run 2>&1)
assert_contains "$OUTPUT" "Dry run - would make the following changes:" "Should show dry-run header"
assert_contains "$OUTPUT" "Update config-minibeads.yaml: mb-hash-ids: false -> true" "Should update config"
assert_contains "$OUTPUT" "Rename file: test-1.md ->" "Should rename test-1"
assert_contains "$OUTPUT" "Rename file: test-2.md ->" "Should rename test-2"
assert_contains "$OUTPUT" "Rename file: test-3.md ->" "Should rename test-3"
assert_contains "$OUTPUT" "Update dependency" "Should update dependencies"

# Verify files haven't changed (dry-run)
assert_file_exists ".beads/issues/test-1.md" "test-1.md should still exist after dry-run"
assert_file_exists ".beads/issues/test-2.md" "test-2.md should still exist after dry-run"
assert_file_exists ".beads/issues/test-3.md" "test-3.md should still exist after dry-run"

# Test 4: Actual migration
echo -e "\n${YELLOW}Test 4: Actual migration${NC}"
OUTPUT=$("$BD_BIN" mb-migrate 2>&1)
assert_contains "$OUTPUT" "Successfully migrated 3 issue(s) to hash-based IDs" "Should report success"
assert_contains "$OUTPUT" "Updated config-minibeads.yaml: mb-hash-ids: true" "Should confirm config update"

# Verify old files are gone
assert_file_not_exists ".beads/issues/test-1.md" "test-1.md should be removed"
assert_file_not_exists ".beads/issues/test-2.md" "test-2.md should be removed"
assert_file_not_exists ".beads/issues/test-3.md" "test-3.md should be removed"

# Verify config was updated
CONFIG_VALUE=$(grep "mb-hash-ids:" .beads/config-minibeads.yaml | awk '{print $2}' | tr -d "'\"")
assert_equals "true" "$CONFIG_VALUE" "mb-hash-ids should now be true"

# Test 5: List issues after migration
echo -e "\n${YELLOW}Test 5: List issues after migration${NC}"
OUTPUT=$("$BD_BIN" list 2>&1)
assert_contains "$OUTPUT" "First issue" "Should list first issue"
assert_contains "$OUTPUT" "Second issue" "Should list second issue"
assert_contains "$OUTPUT" "Third issue" "Should list third issue"

# Get the new hash IDs from the list
ISSUE_1_ID=$(echo "$OUTPUT" | grep "First issue" | awk -F: '{print $1}')
ISSUE_2_ID=$(echo "$OUTPUT" | grep "Second issue" | awk -F: '{print $1}')
ISSUE_3_ID=$(echo "$OUTPUT" | grep "Third issue" | awk -F: '{print $1}')

echo "New hash IDs: $ISSUE_1_ID, $ISSUE_2_ID, $ISSUE_3_ID"

# Verify they're hash IDs (contain at least one hex digit a-f)
if echo "$ISSUE_1_ID" | grep -q "[a-f]"; then
    success "First issue has hash-based ID"
    TESTS_RUN=$((TESTS_RUN + 1))
else
    fail "First issue does not have hash-based ID"
    TESTS_RUN=$((TESTS_RUN + 1))
fi

# Test 6: Verify dependencies were updated
echo -e "\n${YELLOW}Test 6: Verify dependencies were updated${NC}"
OUTPUT=$("$BD_BIN" show "$ISSUE_2_ID" 2>&1)
assert_contains "$OUTPUT" "Dependencies:" "Should have dependencies section"
assert_contains "$OUTPUT" "$ISSUE_1_ID" "Should depend on migrated issue 1"

# Test 7: Create new issue with hash ID
echo -e "\n${YELLOW}Test 7: Create new issue with hash ID${NC}"
OUTPUT=$("$BD_BIN" create "Fourth issue" -d "Created after migration" 2>&1)
assert_contains "$OUTPUT" "Created issue: test-" "Should create new issue"

# Extract the new issue ID
NEW_ISSUE_ID=$(echo "$OUTPUT" | grep "Created issue:" | awk '{print $3}')
echo "New issue ID: $NEW_ISSUE_ID"

# Verify it's a hash ID (contains hex digits a-f)
if echo "$NEW_ISSUE_ID" | grep -q "[a-f]"; then
    success "New issue has hash-based ID"
    TESTS_RUN=$((TESTS_RUN + 1))
else
    fail "New issue does not have hash-based ID"
    TESTS_RUN=$((TESTS_RUN + 1))
fi

# Test 8: Attempt to migrate again (should fail)
echo -e "\n${YELLOW}Test 8: Attempt to migrate again (should fail)${NC}"
OUTPUT=$("$BD_BIN" mb-migrate 2>&1 || true)
assert_contains "$OUTPUT" "Database is already using hash-based IDs" "Should reject re-migration"

# Print summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo "Tests run:    $TESTS_RUN"
echo "Tests passed: $TESTS_PASSED"
echo "Tests failed: $TESTS_FAILED"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    cleanup
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    echo -e "${RED}Test directory preserved: $TEST_DIR${NC}"
    exit 1
fi
