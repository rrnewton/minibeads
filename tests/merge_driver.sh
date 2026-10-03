#!/bin/bash
# E2E test for `mb merge-driver`: real `git merge`s of diverged issue and comment files
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_NAME="merge_driver"
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

export GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=tester@example.com
export GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=tester@example.com
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
MB() { "$BD_BIN" --mb-no-cmd-logging "$@"; }
ISSUE=.minibeads/issues/test-1.md
COMMENTS=.minibeads/comments/test-1.json

git init -q -b main
MB init --prefix test >/dev/null
MB create "Shared title" -p 2 -t task -d "First paragraph.

Second paragraph.

Third paragraph." >/dev/null
MB comments add test-1 -b "Base comment" >/dev/null
git add -A
git commit -qm base

# Test 1: install registers the driver and routes minibeads files to it
echo -e "\n${YELLOW}Test 1: mb merge-driver install${NC}"
OUTPUT=$(MB merge-driver install --command "$BD_BIN" 2>&1)
assert_contains "$OUTPUT" "merge.mb.driver" "Install should report the git config it wrote"
assert_contains "$(git config merge.mb.driver)" "merge-driver run %O %A %B --marker-size %L" "merge.mb.driver should pass git's placeholders"
assert_equals "mb" "$(git check-attr merge -- "$ISSUE" | awk '{print $3}')" "Issue files should use the mb driver"
assert_equals "mb" "$(git check-attr merge -- "$COMMENTS" | awk '{print $3}')" "Comment files should use the mb driver"
MB merge-driver install --command "$BD_BIN" >/dev/null 2>&1
assert_equals "1" "$(grep -c '^.minibeads/issues/\*\*/\*.md merge=mb$' .gitattributes)" "Re-running install should not duplicate .gitattributes lines"
git add .gitattributes
git commit -qm "Use the mb merge driver"

# Test 2: disjoint edits plus comments on both branches merge cleanly
echo -e "\n${YELLOW}Test 2: Clean merge of diverged edits and comments${NC}"
git checkout -qb feature
MB update test-1 --title "Feature title" >/dev/null
MB update test-1 --search "Third paragraph." --replace "Third paragraph, edited on feature." >/dev/null
MB label add test-1 feature-label >/dev/null
MB comments add test-1 -b "Comment from feature" >/dev/null
git commit -qam "feature edits"
git checkout -q main
sleep 1
MB update test-1 -p 0 >/dev/null
MB update test-1 --search "First paragraph." --replace "First paragraph, edited on main." >/dev/null
MB label add test-1 main-label >/dev/null
MB comments add test-1 -b "Comment from main" >/dev/null
git commit -qam "main edits"
MERGE_OUTPUT=$(git merge --no-edit feature 2>&1)
assert_equals "" "$(git diff --name-only --diff-filter=U)" "No files should be left conflicted"
assert_equals "title: Feature title" "$(grep '^title:' "$ISSUE")" "Feature's title should be merged in"
assert_equals "priority: 0" "$(grep '^priority:' "$ISSUE")" "Main's priority should be kept"
assert_contains "$(cat "$ISSUE")" "First paragraph, edited on main." "Main's paragraph edit should be kept"
assert_contains "$(cat "$ISSUE")" "Third paragraph, edited on feature." "Feature's paragraph edit should be merged in"
assert_contains "$(MB label list test-1)" "feature-label" "Feature's label should be merged in"
assert_contains "$(MB label list test-1)" "main-label" "Main's label should be kept"
COMMENT_LIST=$(MB comments list test-1)
for body in "Base comment" "Comment from feature" "Comment from main"; do
    assert_equals "1" "$(echo "$COMMENT_LIST" | grep -cF "$body")" "Comment '$body' should appear exactly once"
done
assert_equals "" "$(grep -E '^(<<<<<<<|=======|>>>>>>>)' "$ISSUE" "$COMMENTS" || true)" "Clean merge should leave no markers"
MB show test-1 >/dev/null
success "Merged issue should still parse"
TESTS_RUN=$((TESTS_RUN + 1))

# Test 3: a comment deleted on one branch survives the merge (append-only set)
echo -e "\n${YELLOW}Test 3: One-sided comment deletion is not propagated${NC}"
git checkout -qb deleter
BASE_COMMENT_ID=$(MB --json comments list test-1 | sed -n 's/.*"id": *"\([^"]*\)".*/\1/p' | head -1)
MB comments delete test-1 "$BASE_COMMENT_ID" >/dev/null
git commit -qam "delete a comment"
git checkout -q main
MB comments add test-1 -b "Comment after the split" >/dev/null
git commit -qam "another comment on main"
MERGE_OUTPUT=$(git merge --no-edit deleter 2>&1)
assert_contains "$MERGE_OUTPUT" "NOTICE $COMMENTS: kept comment $BASE_COMMENT_ID" "Driver should report the kept comment"
assert_equals "1" "$(MB comments list test-1 | grep -cF "Base comment")" "Deleted-on-one-side comment should be kept"
assert_equals "1" "$(MB comments list test-1 | grep -cF "Comment after the split")" "New comment should be kept"

# Test 4: competing title edits conflict on exactly the title line
echo -e "\n${YELLOW}Test 4: Precise conflict hunk for competing title edits${NC}"
git checkout -qb rival
MB update test-1 --title "Rival title" >/dev/null
MB update test-1 --search "Second paragraph." --replace "Second paragraph, rival." >/dev/null
git commit -qam "rival title"
git checkout -q main
MB update test-1 --title "Main title" >/dev/null
git commit -qam "main title"
MERGE_STATUS=0
MERGE_OUTPUT=$(git merge --no-edit rival 2>&1) || MERGE_STATUS=$?
assert_equals "1" "$MERGE_STATUS" "git merge should stop on the conflict"
assert_contains "$MERGE_OUTPUT" "CONFLICT $ISSUE: field title: both sides changed it" "Driver should name the conflicting field"
assert_equals "$ISSUE" "$(git diff --name-only --diff-filter=U)" "Only the issue file should be conflicted"
EXPECTED_HUNK=$(printf '%s\n' "<<<<<<< ours" "title: Main title" "||||||| base" "title: Feature title" "=======" "title: Rival title" ">>>>>>> theirs")
assert_equals "$EXPECTED_HUNK" "$(sed -n '/^<<<<<<< ours$/,/^>>>>>>> theirs$/p' "$ISSUE")" "The only hunk should hold the three titles"
assert_equals "1" "$(grep -c '^<<<<<<< ours$' "$ISSUE")" "There should be exactly one hunk"
assert_contains "$(cat "$ISSUE")" "Second paragraph, rival." "Rival's non-conflicting edit should already be merged"
SHOW_OUTPUT=$(MB show test-1 2>&1 || true)
assert_contains "$SHOW_OUTPUT" "unresolved merge conflict" "mb should refuse to read a file with an unresolved hunk"
# Resolve mechanically by taking theirs, as a tool would
awk 'BEGIN{state=0} /^<<<<<<< ours$/{state=1; next} /^\|\|\|\|\|\|\| base$/{state=2; next} /^=======$/{state=3; next} /^>>>>>>> theirs$/{state=0; next} state==0 || state==3' "$ISSUE" > "$ISSUE.resolved"
mv "$ISSUE.resolved" "$ISSUE"
assert_equals "Rival title" "$(MB --json show test-1 | sed -n 's/.*"title": *"\([^"]*\)".*/\1/p' | head -1)" "Resolved file should parse with theirs' title"
git add "$ISSUE"
git commit -qm "resolve"

# Test 5: run --stdout previews a merge without touching files
echo -e "\n${YELLOW}Test 5: merge-driver run --stdout${NC}"
printf 'a\nb\nc\n' > base.txt
printf 'A\nb\nc\n' > ours.txt
printf 'a\nb\nC\n' > theirs.txt
assert_equals "$(printf 'A\nb\nC')" "$(MB merge-driver run base.txt ours.txt theirs.txt --stdout)" "Plain text should merge line by line"
assert_equals "$(printf 'A\nb\nc')" "$(cat ours.txt)" "--stdout should leave ours untouched"
rm base.txt ours.txt theirs.txt

echo ""
echo "=========================================="
echo "Test Summary: $TEST_NAME"
echo "=========================================="
echo "Tests run:    $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
if [ $TESTS_FAILED -gt 0 ]; then
    echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
    exit 1
fi
echo -e "${GREEN}All tests passed!${NC}"
cleanup
