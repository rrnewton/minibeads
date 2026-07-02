#!/bin/bash
# E2E test for the claim/release workflow (cross-machine coordination)
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="claim"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BD_BIN="$WORKSPACE_ROOT/target/debug/mb"
TEST_DIR="$WORKSPACE_ROOT/scratch/e2e_${TEST_NAME}_$$"

# Counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

cleanup() {
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
}

error_handler() {
    echo -e "${RED}✗ Test failed at line $1${NC}" >&2
    echo -e "${RED}Test directory preserved: $TEST_DIR${NC}" >&2
    exit 1
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

fail() {
    echo -e "${RED}✗ $1${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

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

# Assert a command fails (non-zero exit)
assert_fails() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local message="${1:-Command should fail}"
    shift
    if "$@" >/dev/null 2>&1; then
        fail "$message (command unexpectedly succeeded)"
        return 1
    else
        success "$message"
    fi
}

trap 'error_handler $LINENO' ERR

echo "=========================================="
echo "Running E2E Test: $TEST_NAME"
echo "=========================================="
echo ""

if [ ! -f "$BD_BIN" ]; then
    echo "Building bd binary..."
    cd "$WORKSPACE_ROOT"
    cargo build
    echo ""
fi

echo "Creating test directory: $TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Setup: a repo with two open issues
"$BD_BIN" init --prefix test >/dev/null 2>&1
"$BD_BIN" create "First task" -p 1 -t task >/dev/null 2>&1
"$BD_BIN" create "Second task" -p 2 -t task >/dev/null 2>&1

# Test 1: claim sets assignee, status, and claim window
echo -e "\n${YELLOW}Test 1: Claim an issue${NC}"
OUTPUT=$("$BD_BIN" claim test-1 --as boxA 2>&1)
assert_contains "$OUTPUT" "Claimed test-1 as 'boxA'" "Claim should report holder"
assert_equals "in_progress" "$(grep '^status:' .minibeads/issues/test-1.md | awk '{print $2}')" "Status should be in_progress"
assert_equals "boxA" "$(grep '^assignee:' .minibeads/issues/test-1.md | awk '{print $2}')" "Assignee should be boxA"
assert_equals "true" "$(grep -q '^claimed_at:' .minibeads/issues/test-1.md && echo true || echo false)" "claimed_at should be recorded"
assert_equals "true" "$(grep -q '^claimed_until:' .minibeads/issues/test-1.md && echo true || echo false)" "claimed_until should be recorded"

# Test 2: a different worker cannot steal an active claim
echo -e "\n${YELLOW}Test 2: Active claim is protected${NC}"
assert_fails "Claim by another worker should fail" "$BD_BIN" claim test-1 --as boxB
assert_equals "boxA" "$(grep '^assignee:' .minibeads/issues/test-1.md | awk '{print $2}')" "Holder should be unchanged after failed claim"

# Test 3: claimed issue drops out of ready
echo -e "\n${YELLOW}Test 3: Claimed work leaves the ready queue${NC}"
OUTPUT=$("$BD_BIN" ready 2>&1)
if echo "$OUTPUT" | grep -qF "test-1:"; then
    fail "test-1 should not appear in ready while claimed"
else
    success "test-1 excluded from ready while claimed"
fi
TESTS_RUN=$((TESTS_RUN + 1))

# Test 4: host/team identity and custom duration via the update --claim long form
echo -e "\n${YELLOW}Test 4: host/team identity via 'mb update --claim'${NC}"
OUTPUT=$("$BD_BIN" update test-2 --claim --as boxA --team backend --for 4h 2>&1)
assert_contains "$OUTPUT" "Claimed test-2 as 'boxA/backend'" "update --claim should support host/team"
assert_equals "boxA/backend" "$(grep '^assignee:' .minibeads/issues/test-2.md | awk '{print $2}')" "Assignee should be boxA/backend"

# Test 5: release returns the issue to the backlog
echo -e "\n${YELLOW}Test 5: Release a claim${NC}"
OUTPUT=$("$BD_BIN" claim test-1 --release --as boxA 2>&1)
assert_contains "$OUTPUT" "Released test-1" "Release should be confirmed"
assert_equals "open" "$(grep '^status:' .minibeads/issues/test-1.md | awk '{print $2}')" "Status should be open after release"
assert_equals "false" "$(grep -q '^assignee:' .minibeads/issues/test-1.md && echo true || echo false)" "Assignee should be cleared after release"
assert_equals "false" "$(grep -q '^claimed_until:' .minibeads/issues/test-1.md && echo true || echo false)" "claimed_until should be cleared after release"
OUTPUT=$("$BD_BIN" ready 2>&1)
assert_contains "$OUTPUT" "test-1:" "test-1 should return to ready after release"

# Test 6: releasing someone else's claim requires --force
echo -e "\n${YELLOW}Test 6: Force-release another worker's claim${NC}"
"$BD_BIN" claim test-1 --as boxA >/dev/null 2>&1
assert_fails "Releasing another worker's claim should fail without --force" "$BD_BIN" claim test-1 --release --as boxB
OUTPUT=$("$BD_BIN" claim test-1 --release --as boxB --force 2>&1)
assert_contains "$OUTPUT" "Released test-1" "Force release should succeed"

# Test 7: claim window survives JSONL round-trip
echo -e "\n${YELLOW}Test 7: Claim survives JSONL sync round-trip${NC}"
"$BD_BIN" claim test-1 --as boxA --for 2d >/dev/null 2>&1
BEFORE=$(cat .minibeads/issues/test-1.md)
"$BD_BIN" export --mb-output-default >/dev/null 2>&1
rm .minibeads/issues/test-1.md
"$BD_BIN" sync >/dev/null 2>&1
AFTER=$(cat .minibeads/issues/test-1.md)
assert_equals "$BEFORE" "$AFTER" "Claimed issue should round-trip identically through JSONL"

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
