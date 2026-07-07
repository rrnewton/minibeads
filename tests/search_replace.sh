#!/bin/bash
# E2E test for `mb update --search/--replace` (targeted, aider-style field edits)
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="search_replace"
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

# Setup: a repo with one issue whose description has a repeated token.
"$BD_BIN" init --prefix test >/dev/null 2>&1
"$BD_BIN" create "A task" -d "I have a CLI that lets agents file tasks. The CLI is clumsy to edit." >/dev/null 2>&1

# Read back the description body of test-1 (everything after the "# Description" heading).
desc_body() {
    sed -n '/^# Description/,/^# /p' .minibeads/issues/test-1.md | grep -v '^# ' | sed '/^$/d'
}

# Test 1: a uniquely-matching search is replaced in place
echo -e "\n${YELLOW}Test 1: Unique search/replace edits the description${NC}"
OUTPUT=$("$BD_BIN" update test-1 --search "clumsy to edit" --replace "edited via search/replace" 2>&1)
assert_contains "$OUTPUT" "Updated issue: test-1 (description field)" "Should confirm targeted edit"
assert_contains "$(desc_body)" "edited via search/replace" "Replacement text should be present"
assert_fails "Old text should be gone" grep -qF "clumsy to edit" .minibeads/issues/test-1.md

# Test 2: a search that does not match fails and changes nothing
echo -e "\n${YELLOW}Test 2: Missing search text is an error${NC}"
BEFORE=$(cat .minibeads/issues/test-1.md)
assert_fails "Non-matching search should fail" "$BD_BIN" update test-1 --search "no such text" --replace "x"
assert_equals "$BEFORE" "$(cat .minibeads/issues/test-1.md)" "File must be untouched after a failed search"

# Test 3: an ambiguous (multi-match) search is rejected by default
echo -e "\n${YELLOW}Test 3: Ambiguous search is rejected without --replace-all${NC}"
assert_fails "Ambiguous search should fail" "$BD_BIN" update test-1 --search "CLI" --replace "tool"
OUTPUT=$("$BD_BIN" update test-1 --search "CLI" --replace "tool" 2>&1 || true)
assert_contains "$OUTPUT" "2 times" "Error should report the match count"

# Test 4: --replace-all rewrites every occurrence
echo -e "\n${YELLOW}Test 4: --replace-all rewrites every occurrence${NC}"
"$BD_BIN" update test-1 --search "CLI" --replace "tool" --replace-all >/dev/null 2>&1
assert_fails "No CLI should remain" grep -qF "CLI" .minibeads/issues/test-1.md
assert_contains "$(desc_body)" "tool" "Replacement should be present after replace-all"

# Test 5: --field targets a non-default field
echo -e "\n${YELLOW}Test 5: --field targets another text field${NC}"
"$BD_BIN" update test-1 --design "alpha beta gamma" >/dev/null 2>&1
OUTPUT=$("$BD_BIN" update test-1 --field design --search "beta" --replace "BETA" 2>&1)
assert_contains "$OUTPUT" "Updated issue: test-1 (design field)" "Should report the design field"
assert_contains "$(sed -n '/^# Design/,/^# /p' .minibeads/issues/test-1.md)" "alpha BETA gamma" "Design field should be edited"

# Test 6: --search requires --replace, and conflicts with wholesale --description
echo -e "\n${YELLOW}Test 6: Argument guards${NC}"
assert_fails "--search without --replace should fail" "$BD_BIN" update test-1 --search "tool"
assert_fails "--search with --description should conflict" "$BD_BIN" update test-1 --search "tool" --replace "x" --description "whole new body"

# Test 7: --append tacks a new paragraph onto a field
echo -e "\n${YELLOW}Test 7: --append adds to the end of a field${NC}"
OUTPUT=$("$BD_BIN" update test-1 --append "An appended closing paragraph." 2>&1)
assert_contains "$OUTPUT" "Appended to issue: test-1 (description field)" "Should confirm the append"
assert_contains "$(desc_body)" "An appended closing paragraph." "Appended text should be present"
# The pre-existing description content must still be there.
assert_contains "$(desc_body)" "edited via search/replace" "Original description must be preserved"

# Test 8: --append honors --field and conflicts with the wholesale setters
echo -e "\n${YELLOW}Test 8: --append field selection and guards${NC}"
"$BD_BIN" update test-1 --field design --append "An appended design note." >/dev/null 2>&1
assert_contains "$(sed -n '/^# Design/,/^# /p' .minibeads/issues/test-1.md)" "An appended design note." "Append should target the design field"
assert_fails "--append with --description should conflict" "$BD_BIN" update test-1 --append "x" --description "whole new body"
assert_fails "--append with --search should conflict" "$BD_BIN" update test-1 --append "x" --search "y" --replace "z"

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
