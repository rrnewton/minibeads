#!/bin/bash
# E2E test for bd export interoperability with upstream bd
set -euo pipefail

# Parse command line arguments
KEEP_TEMP=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --keep)
            KEEP_TEMP=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--keep]"
            echo "  --keep    Keep temporary test directories instead of cleaning up"
            exit 1
            ;;
    esac
done

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="export_interop"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BD_BIN="$WORKSPACE_ROOT/target/debug/mb"
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

# Check for upstream bd
UPSTREAM_BD=""
if [ -f "$WORKSPACE_ROOT/beads/bd-upstream" ]; then
    UPSTREAM_BD="$WORKSPACE_ROOT/beads/bd-upstream"
    echo -e "${BLUE}Found upstream bd: ./beads/bd-upstream${NC}"
elif command -v bd-upstream >/dev/null 2>&1; then
    UPSTREAM_BD="bd-upstream"
    echo -e "${BLUE}Found upstream bd: bd-upstream${NC}"
elif [ -f "$HOME/.cargo/bin/bd-upstream" ]; then
    UPSTREAM_BD="$HOME/.cargo/bin/bd-upstream"
    echo -e "${BLUE}Found upstream bd: $UPSTREAM_BD${NC}"
elif command -v beads >/dev/null 2>&1; then
    UPSTREAM_BD="beads"
    echo -e "${BLUE}Found upstream bd: beads${NC}"
else
    echo -e "${YELLOW}Upstream bd not found - will test export only${NC}"
    echo -e "${YELLOW}To build upstream bd: make upstream${NC}"
    echo -e "${YELLOW}Or install from source: cargo install --git https://github.com/steveyegge/beads bd${NC}"
    echo ""
fi

# Create test directory
echo "Creating test directory: $TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Test 1: Initialize minibeads database
echo -e "\n${YELLOW}Test 1: Initialize minibeads database${NC}"
OUTPUT=$("$BD_BIN" init --prefix exp 2>&1)
assert_contains "$OUTPUT" "Initialized beads database with prefix: exp" "Initialize should report prefix"

# Test 2: Create test issues with various types and states
echo -e "\n${YELLOW}Test 2: Create test issues${NC}"
"$BD_BIN" create "Bug Fix A" -p 0 -t bug -d "Fix critical authentication bug" >/dev/null 2>&1
"$BD_BIN" create "Feature B" -p 1 -t feature -d "Add export functionality" >/dev/null 2>&1
"$BD_BIN" create "Task C" -p 2 -t task --deps exp-1 >/dev/null 2>&1
"$BD_BIN" update exp-2 --status in_progress >/dev/null 2>&1
"$BD_BIN" close exp-1 --reason "Fixed" >/dev/null 2>&1

# Verify issues were created
assert_file_exists ".beads/issues/exp-1.md" "exp-1 file should exist"
assert_file_exists ".beads/issues/exp-2.md" "exp-2 file should exist"
assert_file_exists ".beads/issues/exp-3.md" "exp-3 file should exist"

# Test 3: Export to stdout
echo -e "\n${YELLOW}Test 3: Export to stdout (default)${NC}"
STDOUT_EXPORT=$("$BD_BIN" export 2>/dev/null)
LINE_COUNT=$(echo "$STDOUT_EXPORT" | wc -l | tr -d ' ')
assert_equals "3" "$LINE_COUNT" "Should export 3 issues to stdout"

# Verify it's valid JSON
assert_contains "$STDOUT_EXPORT" '"id":"exp-1"' "Should contain exp-1 in JSON"
assert_contains "$STDOUT_EXPORT" '"id":"exp-2"' "Should contain exp-2 in JSON"
assert_contains "$STDOUT_EXPORT" '"id":"exp-3"' "Should contain exp-3 in JSON"

# Test 4: Export to file with -o flag
echo -e "\n${YELLOW}Test 4: Export to file with -o flag${NC}"
OUTPUT=$("$BD_BIN" export -o custom_export.jsonl 2>&1)
assert_contains "$OUTPUT" "Exported 3 issues to custom_export.jsonl" "Should report export to custom file"
assert_file_exists "custom_export.jsonl" "Custom export file should exist"

# Verify file contents
FILE_LINES=$(wc -l < custom_export.jsonl | tr -d ' ')
assert_equals "3" "$FILE_LINES" "Custom export should have 3 lines"

# Test 5: Export with --mb-output-default flag
echo -e "\n${YELLOW}Test 5: Export with --mb-output-default flag${NC}"
OUTPUT=$("$BD_BIN" export --mb-output-default 2>&1)
assert_contains "$OUTPUT" "Exported 3 issues to" "Should report export"
assert_file_exists ".beads/issues.jsonl" "Default export file should exist"

JSONL_FILE="$TEST_DIR/.beads/issues.jsonl"
JSONL_LINES=$(wc -l < "$JSONL_FILE" | tr -d ' ')
assert_equals "3" "$JSONL_LINES" "Export should have 3 lines"

# Test 6: Verify JSONL format is valid
echo -e "\n${YELLOW}Test 6: Verify JSONL format${NC}"
# Each line should be valid JSON
TESTS_RUN=$((TESTS_RUN + 1))
if jq -e '.' "$JSONL_FILE" >/dev/null 2>&1; then
    success "All lines are valid JSON"
else
    fail "JSONL contains invalid JSON"
    return 1
fi

# Verify required fields exist
FIRST_ISSUE=$(head -n 1 "$JSONL_FILE")
assert_contains "$FIRST_ISSUE" '"id":' "Issue should have id field"
assert_contains "$FIRST_ISSUE" '"title":' "Issue should have title field"
assert_contains "$FIRST_ISSUE" '"status":' "Issue should have status field"
assert_contains "$FIRST_ISSUE" '"priority":' "Issue should have priority field"
assert_contains "$FIRST_ISSUE" '"issue_type":' "Issue should have issue_type field"
assert_contains "$FIRST_ISSUE" '"created_at":' "Issue should have created_at field"
assert_contains "$FIRST_ISSUE" '"updated_at":' "Issue should have updated_at field"

# Test 7: Verify dependencies/dependents format (MCP compatible)
echo -e "\n${YELLOW}Test 7: Verify dependencies/dependents format${NC}"
# exp-3 depends on exp-1
EXP3_JSON=$(grep '"id":"exp-3"' "$JSONL_FILE")
assert_contains "$EXP3_JSON" '"dependencies":[' "Should have dependencies array"
assert_contains "$EXP3_JSON" '"id":"exp-1"' "Should have exp-1 dependency"

# exp-1 should have exp-3 as dependent
EXP1_JSON=$(grep '"id":"exp-1"' "$JSONL_FILE")
assert_contains "$EXP1_JSON" '"dependents":[' "Should have dependents array"
assert_contains "$EXP1_JSON" '"id":"exp-3"' "Should have exp-3 as dependent"

# Test 8: Export with filters
echo -e "\n${YELLOW}Test 8: Export with status filter${NC}"
OPEN_EXPORT=$("$BD_BIN" export --status open 2>/dev/null)
OPEN_COUNT=$(echo "$OPEN_EXPORT" | wc -l | tr -d ' ')
assert_equals "1" "$OPEN_COUNT" "Should export 1 open issue (exp-3)"

# Also test closed filter
CLOSED_EXPORT=$("$BD_BIN" export --status closed 2>/dev/null)
CLOSED_COUNT=$(echo "$CLOSED_EXPORT" | wc -l | tr -d ' ')
assert_equals "1" "$CLOSED_COUNT" "Should export 1 closed issue (exp-1)"

# Test 9: Upstream bd interoperability (if available)
if [ -n "$UPSTREAM_BD" ]; then
    echo -e "\n${YELLOW}Test 9: Upstream bd interoperability${NC}"

    # Create a clean directory for upstream bd testing
    UPSTREAM_DIR="$TEST_DIR/upstream_test"
    echo -e "${BLUE}→ mkdir -p $UPSTREAM_DIR${NC}"
    mkdir -p "$UPSTREAM_DIR"
    cd "$UPSTREAM_DIR"

    # Initialize upstream bd to create .beads directory
    echo -e "${BLUE}→ echo n | $UPSTREAM_BD init --prefix exp${NC}"
    echo n | "$UPSTREAM_BD" init --prefix exp >/dev/null 2>&1
    echo -e "${BLUE}  (init completed)${NC}"

    # Copy exported issues.jsonl to .beads directory
    echo -e "${BLUE}→ cp $JSONL_FILE .beads/issues.jsonl${NC}"
    cp "$JSONL_FILE" .beads/issues.jsonl

    # Test upstream bd list (latest upstream uses database mode, not --no-db)
    echo -e "${BLUE}→ $UPSTREAM_BD list${NC}"
    UPSTREAM_LIST=$("$UPSTREAM_BD" list 2>&1)
    assert_contains "$UPSTREAM_LIST" "exp-1" "Upstream bd should list exp-1"
    assert_contains "$UPSTREAM_LIST" "exp-2" "Upstream bd should list exp-2"
    assert_contains "$UPSTREAM_LIST" "exp-3" "Upstream bd should list exp-3"

    # Test upstream bd show
    echo -e "${BLUE}→ $UPSTREAM_BD show exp-1${NC}"
    UPSTREAM_SHOW=$("$UPSTREAM_BD" show exp-1 2>&1)
    assert_contains "$UPSTREAM_SHOW" "exp-1" "Upstream bd should show exp-1"
    assert_contains "$UPSTREAM_SHOW" "Bug Fix A" "Upstream bd should show title"

    # Test upstream bd stats
    echo -e "${BLUE}→ $UPSTREAM_BD stats${NC}"
    UPSTREAM_STATS=$("$UPSTREAM_BD" stats 2>&1)
    assert_contains "$UPSTREAM_STATS" "Total Issues:" "Upstream bd should show total issues"
    assert_contains "$UPSTREAM_STATS" "3" "Upstream bd should show 3 as total"

    cd "$TEST_DIR"
else
    echo -e "\n${YELLOW}Test 9: Upstream bd interoperability - SKIPPED${NC}"
    echo -e "${YELLOW}Install upstream bd to enable full interoperability testing${NC}"
fi

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
    if [ "$KEEP_TEMP" = true ]; then
        echo -e "${BLUE}Test directory preserved (--keep): $TEST_DIR${NC}"
    else
        cleanup
    fi
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    echo -e "${RED}Test directory preserved: $TEST_DIR${NC}"
    exit 1
fi
