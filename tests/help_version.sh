#!/bin/bash
# E2E test for bd help and version commands
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="help_version"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BD_BIN="$WORKSPACE_ROOT/target/debug/bd"

# Counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

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

# Test 1: bd --help
echo -e "\n${YELLOW}Test 1: bd --help${NC}"
OUTPUT=$("$BD_BIN" --help 2>&1)
assert_contains "$OUTPUT" "Minibeads - A minimal issue tracker" "Help should show program description"
assert_contains "$OUTPUT" "Commands:" "Help should list commands"
assert_contains "$OUTPUT" "init" "Help should list init command"
assert_contains "$OUTPUT" "create" "Help should list create command"
assert_contains "$OUTPUT" "list" "Help should list list command"
assert_contains "$OUTPUT" "show" "Help should list show command"

# Test 2: bd -h (short form)
echo -e "\n${YELLOW}Test 2: bd -h (short form)${NC}"
OUTPUT=$("$BD_BIN" -h 2>&1)
assert_contains "$OUTPUT" "Minibeads" "Short help should work"

# Test 3: bd help (subcommand)
echo -e "\n${YELLOW}Test 3: bd help${NC}"
OUTPUT=$("$BD_BIN" help 2>&1)
assert_contains "$OUTPUT" "Minibeads" "bd help subcommand should work"

# Test 4: bd init --help
echo -e "\n${YELLOW}Test 4: bd init --help${NC}"
OUTPUT=$("$BD_BIN" init --help 2>&1)
assert_contains "$OUTPUT" "Initialize beads in current directory" "Init help should show description"
assert_contains "$OUTPUT" "--prefix" "Init help should show --prefix flag"

# Test 5: bd create --help
echo -e "\n${YELLOW}Test 5: bd create --help${NC}"
OUTPUT=$("$BD_BIN" create --help 2>&1)
assert_contains "$OUTPUT" "Create a new issue" "Create help should show description"
assert_contains "$OUTPUT" "--priority" "Create help should show --priority flag"
assert_contains "$OUTPUT" "--issue-type" "Create help should show --issue-type flag"

# Test 6: bd list --help
echo -e "\n${YELLOW}Test 6: bd list --help${NC}"
OUTPUT=$("$BD_BIN" list --help 2>&1)
assert_contains "$OUTPUT" "List issues" "List help should show description"
assert_contains "$OUTPUT" "-s, --status" "List help should show -s, --status flag"
assert_contains "$OUTPUT" "-p, --priority" "List help should show -p, --priority flag"

# Test 7: bd version
echo -e "\n${YELLOW}Test 7: bd version${NC}"
OUTPUT=$("$BD_BIN" version 2>&1)
assert_contains "$OUTPUT" "bd version" "Version should show version string"
assert_contains "$OUTPUT" "0.9.0" "Version should show version number"

# Test 8: bd --version
echo -e "\n${YELLOW}Test 8: bd --version${NC}"
OUTPUT=$("$BD_BIN" --version 2>&1)
assert_contains "$OUTPUT" "bd" "--version should show program name"

# Test 9: bd quickstart
echo -e "\n${YELLOW}Test 9: bd quickstart${NC}"
OUTPUT=$("$BD_BIN" quickstart 2>&1)
assert_contains "$OUTPUT" "GETTING STARTED" "Quickstart should show getting started section"
assert_contains "$OUTPUT" "bd init" "Quickstart should mention bd init"
assert_contains "$OUTPUT" "bd create" "Quickstart should mention bd create"
assert_contains "$OUTPUT" "bd list" "Quickstart should mention bd list"
assert_contains "$OUTPUT" "DEPENDENCY TYPES" "Quickstart should explain dependency types"

# Test 10: bd dep --help
echo -e "\n${YELLOW}Test 10: bd dep --help${NC}"
OUTPUT=$("$BD_BIN" dep --help 2>&1)
assert_contains "$OUTPUT" "Manage dependencies" "Dep help should show description"
assert_contains "$OUTPUT" "add" "Dep help should show add subcommand"

# Test 11: bd dep add --help
echo -e "\n${YELLOW}Test 11: bd dep add --help${NC}"
OUTPUT=$("$BD_BIN" dep add --help 2>&1)
assert_contains "$OUTPUT" "Add a dependency" "Dep add help should show description"
assert_contains "$OUTPUT" "--type" "Dep add help should show --type flag"

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
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
