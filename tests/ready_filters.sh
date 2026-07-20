#!/bin/bash
# E2E test for `mb ready` filter options (parity with `mb list`)
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="ready_filters"
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

assert_not_contains() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local haystack="$1"
    local needle="$2"
    local message="${3:-Assertion failed}"

    if echo "$haystack" | grep -qF -- "$needle"; then
        fail "$message (unexpectedly found: '$needle' in output)"
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

"$BD_BIN" init --prefix test >/dev/null 2>&1

# Fixture issues covering the various filter dimensions.
"$BD_BIN" create "Fix login bug" -t bug -p 1 -l backend -l urgent >/dev/null
"$BD_BIN" create "Add dark mode" -t feature -p 2 -l frontend >/dev/null
"$BD_BIN" create "Write docs" -t task -p 3 -l docs >/dev/null
"$BD_BIN" create "Refactor auth" -t task -p 1 -l backend >/dev/null

# test-4 becomes a child of test-1; test-3 becomes blocked by test-1.
"$BD_BIN" dep add test-4 test-1 -t parent-child >/dev/null
"$BD_BIN" dep add test-3 test-1 -t blocks >/dev/null

# Ready still excludes blocked issues.
echo -e "\n${YELLOW}Blocked issues stay out of ready${NC}"
OUTPUT=$("$BD_BIN" ready 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "test-1 is ready"
assert_not_contains "$OUTPUT" "test-3: Write docs" "test-3 is blocked, not ready"

# --label (must have ALL specified labels).
echo -e "\n${YELLOW}--label filter${NC}"
OUTPUT=$("$BD_BIN" ready --label backend 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "backend label includes test-1"
assert_contains "$OUTPUT" "test-4: Refactor auth" "backend label includes test-4"
assert_not_contains "$OUTPUT" "test-2: Add dark mode" "frontend issue excluded by backend label"

OUTPUT=$("$BD_BIN" ready --label backend --label urgent 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "ALL labels: test-1 has backend+urgent"
assert_not_contains "$OUTPUT" "test-4: Refactor auth" "ALL labels: test-4 lacks urgent"

# --type filter.
echo -e "\n${YELLOW}--type filter${NC}"
OUTPUT=$("$BD_BIN" ready --type feature 2>&1)
assert_contains "$OUTPUT" "test-2: Add dark mode" "type feature includes test-2"
assert_not_contains "$OUTPUT" "test-1: Fix login bug" "type feature excludes bug"

# --priority as comma-separated list.
echo -e "\n${YELLOW}--priority comma-separated list${NC}"
OUTPUT=$("$BD_BIN" ready --priority 1,2 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "priority 1,2 includes p1"
assert_contains "$OUTPUT" "test-2: Add dark mode" "priority 1,2 includes p2"

# --priority repeated (must union with comma-separated behaviour).
echo -e "\n${YELLOW}--priority repeated flag${NC}"
OUTPUT=$("$BD_BIN" ready -p 1 -p 2 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "-p 1 -p 2 includes p1"
assert_contains "$OUTPUT" "test-2: Add dark mode" "-p 1 -p 2 includes p2"
assert_not_contains "$OUTPUT" "test-3: Write docs" "-p 1 -p 2 excludes p3 (also blocked)"

# --title substring (case-insensitive).
echo -e "\n${YELLOW}--title filter${NC}"
OUTPUT=$("$BD_BIN" ready --title DARK 2>&1)
assert_contains "$OUTPUT" "test-2: Add dark mode" "title DARK matches case-insensitively"
assert_not_contains "$OUTPUT" "test-1: Fix login bug" "title DARK excludes others"

# --id (comma-separated exact IDs).
echo -e "\n${YELLOW}--id filter${NC}"
OUTPUT=$("$BD_BIN" ready --id test-1,test-2 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "id filter includes test-1"
assert_contains "$OUTPUT" "test-2: Add dark mode" "id filter includes test-2"
assert_not_contains "$OUTPUT" "test-4: Refactor auth" "id filter excludes test-4"

# --parent (direct children only).
echo -e "\n${YELLOW}--parent filter${NC}"
OUTPUT=$("$BD_BIN" ready --parent test-1 2>&1)
assert_contains "$OUTPUT" "test-4: Refactor auth" "parent test-1 includes child test-4"
assert_not_contains "$OUTPUT" "test-1: Fix login bug" "parent test-1 excludes the parent itself"

# --limit applies after filtering (test-1 and test-4 both match backend).
echo -e "\n${YELLOW}--limit filter${NC}"
TESTS_RUN=$((TESTS_RUN + 1))
OUTPUT=$("$BD_BIN" ready --label backend --limit 1 2>&1)
LINES=$(echo "$OUTPUT" | grep -c "^test-" || true)
if [ "$LINES" = "1" ]; then
    success "limit truncates filtered results to 1"
else
    fail "limit should truncate to 1 (got $LINES lines)"
fi

# --group-priority display.
echo -e "\n${YELLOW}--group-priority display${NC}"
OUTPUT=$("$BD_BIN" ready --group-priority 2>&1)
assert_contains "$OUTPUT" "Priority 1" "group-priority prints a Priority 1 header"
assert_contains "$OUTPUT" "Priority 2" "group-priority prints a Priority 2 header"

# --github (no linked issues here, so it should be empty).
echo -e "\n${YELLOW}--github filter${NC}"
OUTPUT=$("$BD_BIN" ready --github 2>&1)
assert_not_contains "$OUTPUT" "test-1: Fix login bug" "github filter excludes non-linked issues"

# --sort random preserves the full ready set (just reorders it).
echo -e "\n${YELLOW}--sort random preserves the set${NC}"
TESTS_RUN=$((TESTS_RUN + 1))
COUNT=$("$BD_BIN" ready --sort random 2>&1 | grep -c "^test-" || true)
# Ready set is test-1, test-2, test-4 (test-3 is blocked).
if [ "$COUNT" = "3" ]; then
    success "random sort returns all 3 ready issues"
else
    fail "random sort should return 3 issues (got $COUNT)"
fi

# --sort random with -n 1 picks from the whole filtered set, not just its head.
# test-1 and test-4 are the two ready priority-1 issues; over many draws we
# should observe both (P(all-same) over 40 draws is ~2^-39).
echo -e "\n${YELLOW}--sort random -n 1 picks uniformly${NC}"
TESTS_RUN=$((TESTS_RUN + 1))
DISTINCT=$(for _ in $(seq 40); do
    "$BD_BIN" ready -p 1 -s random -n 1 2>/dev/null | awk -F: '{print $1}'
done | sort -u | wc -l | tr -d ' ')
if [ "$DISTINCT" -ge 2 ]; then
    success "random -n 1 yielded $DISTINCT distinct picks across 40 draws"
else
    fail "random -n 1 should vary its pick (only $DISTINCT distinct over 40 draws)"
fi

# Combined filters compose.
echo -e "\n${YELLOW}Combined filters${NC}"
OUTPUT=$("$BD_BIN" ready --label backend --priority 1 2>&1)
assert_contains "$OUTPUT" "test-1: Fix login bug" "combined: backend + p1 includes test-1"
assert_contains "$OUTPUT" "test-4: Refactor auth" "combined: backend + p1 includes test-4"

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
