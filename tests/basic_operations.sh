#!/bin/bash
# E2E test for basic bd operations
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="basic_operations"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BD_BIN="$WORKSPACE_ROOT/target/debug/bd"
TEST_DIR="$WORKSPACE_ROOT/test_spaces/e2e_${TEST_NAME}_$$"

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

    if echo "$haystack" | grep -q "$needle"; then
        success "$message"
    else
        fail "$message (expected to find: '$needle' in output)"
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

# Test 1: Initialize beads database
echo -e "\n${YELLOW}Test 1: Initialize database${NC}"
OUTPUT=$("$BD_BIN" init --prefix test 2>&1)
assert_contains "$OUTPUT" "Initialized beads database with prefix: test" "Initialize should report prefix"
assert_equals "true" "$([ -d .beads/issues ] && echo true || echo false)" "Issues directory should exist"
assert_equals "true" "$([ -f .beads/config.yaml ] && echo true || echo false)" "Config file should exist"

# Test 2: Create an issue
echo -e "\n${YELLOW}Test 2: Create an issue${NC}"
OUTPUT=$("$BD_BIN" create "Test issue 1" -p 1 -t task -d "Test description" 2>&1)
assert_contains "$OUTPUT" "Created issue: test-1" "Should create test-1"
assert_equals "true" "$([ -f .beads/issues/test-1.md ] && echo true || echo false)" "Issue file should exist"

# Test 3: Create another issue with dependency
echo -e "\n${YELLOW}Test 3: Create issue with dependency${NC}"
OUTPUT=$("$BD_BIN" create "Test issue 2" -p 2 -t bug --deps test-1 2>&1)
assert_contains "$OUTPUT" "Created issue: test-2" "Should create test-2"

# Verify dependency in file
DEP_COUNT=$(grep -c "test-1: blocks" .beads/issues/test-2.md || true)
assert_equals "1" "$DEP_COUNT" "Dependency should be recorded"

# Test 4: List issues
echo -e "\n${YELLOW}Test 4: List issues${NC}"
OUTPUT=$("$BD_BIN" list 2>&1)
assert_contains "$OUTPUT" "test-1: Test issue 1" "Should list test-1"
assert_contains "$OUTPUT" "test-2: Test issue 2" "Should list test-2"

# Test 5: Show issue details
echo -e "\n${YELLOW}Test 5: Show issue details${NC}"
OUTPUT=$("$BD_BIN" show test-1 2>&1)
assert_contains "$OUTPUT" "ID: test-1" "Should show issue ID"
assert_contains "$OUTPUT" "Title: Test issue 1" "Should show title"
assert_contains "$OUTPUT" "Description:" "Should show description section"
assert_contains "$OUTPUT" "Test description" "Should show description content"

# Test 6: Update issue status
echo -e "\n${YELLOW}Test 6: Update issue status${NC}"
OUTPUT=$("$BD_BIN" update test-1 --status in_progress 2>&1)
assert_contains "$OUTPUT" "Updated issue: test-1" "Should confirm update"

# Verify status in file
STATUS=$(grep "^status:" .beads/issues/test-1.md | awk '{print $2}')
assert_equals "in_progress" "$STATUS" "Status should be in_progress"

# Test 7: Get statistics
echo -e "\n${YELLOW}Test 7: Get statistics${NC}"
OUTPUT=$("$BD_BIN" stats 2>&1)
assert_contains "$OUTPUT" "Total issues: 2" "Should show 2 total issues"
assert_contains "$OUTPUT" "Open: 1" "Should show 1 open issue"
assert_contains "$OUTPUT" "In Progress: 1" "Should show 1 in_progress issue"

# Test 8: Get ready work
echo -e "\n${YELLOW}Test 8: Get ready work${NC}"
OUTPUT=$("$BD_BIN" ready 2>&1)
# test-2 is blocked by test-1, so it shouldn't appear in ready
OUTPUT_LINES=$(echo "$OUTPUT" | wc -l)
# Should have few lines since test-2 is blocked
assert_equals "true" "$([ $OUTPUT_LINES -le 3 ] && echo true || echo false)" "Should have limited ready work"

# Test 9: Get blocked issues
echo -e "\n${YELLOW}Test 9: Get blocked issues${NC}"
OUTPUT=$("$BD_BIN" blocked 2>&1)
assert_contains "$OUTPUT" "test-2: Test issue 2 - blocked by: test-1" "test-2 should be blocked by test-1"

# Test 10: Close an issue
echo -e "\n${YELLOW}Test 10: Close an issue${NC}"
OUTPUT=$("$BD_BIN" close test-1 --reason "Test completed" 2>&1)
assert_contains "$OUTPUT" "Closed issue: test-1" "Should confirm closure"

# Verify closed status
STATUS=$(grep "^status:" .beads/issues/test-1.md | awk '{print $2}')
assert_equals "closed" "$STATUS" "Status should be closed"

# Test 11: Reopen an issue
echo -e "\n${YELLOW}Test 11: Reopen an issue${NC}"
OUTPUT=$("$BD_BIN" reopen test-1 2>&1)
assert_contains "$OUTPUT" "Reopened issue: test-1" "Should confirm reopen"

STATUS=$(grep "^status:" .beads/issues/test-1.md | awk '{print $2}')
assert_equals "open" "$STATUS" "Status should be open"

# Test 12: JSON output
echo -e "\n${YELLOW}Test 12: JSON output${NC}"
OUTPUT=$("$BD_BIN" list --json 2>&1)
# Verify it's valid JSON by checking structure
assert_contains "$OUTPUT" "\"id\":" "JSON should have id field"
assert_contains "$OUTPUT" "\"title\":" "JSON should have title field"
assert_contains "$OUTPUT" "test-1" "JSON should include test-1"

# Test 13: Add dependency manually
echo -e "\n${YELLOW}Test 13: Add dependency${NC}"
"$BD_BIN" create "Test issue 3" -p 1 -t task >/dev/null 2>&1
OUTPUT=$("$BD_BIN" dep add test-3 test-2 --type related 2>&1)
assert_contains "$OUTPUT" "Added dependency: test-3 depends on test-2" "Should add dependency"

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
