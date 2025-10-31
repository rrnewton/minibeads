#!/bin/bash
# Simple benchmark script for minibeads operations

set -e

echo "=== Minibeads Benchmark ==="
echo "Testing with large issue dataset"
echo

# Get the project root directory (where the script is run from)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BD_BIN="$PROJECT_ROOT/target/release/bd"

# Create temporary test directory
BENCH_DIR=$(mktemp -d)
echo "Benchmark directory: $BENCH_DIR"

cd "$BENCH_DIR"

# Initialize beads
"$BD_BIN" init --prefix bench

# Create 100 issues with dependencies to test blocking dependency checks
echo "Creating 100 test issues..."
for i in $(seq 1 100); do
    if [ $i -gt 1 ]; then
        # Every issue depends on the previous one (creates blocking chain)
        prev=$((i-1))
        "$BD_BIN" create "Task $i" -p 2 --deps bench-$prev > /dev/null
    else
        "$BD_BIN" create "Task $i" -p 2 > /dev/null
    fi
done

echo "Running benchmarks..."
echo

# Benchmark: bd stats (exercises has_blocking_dependencies)
echo "Benchmark: bd stats"
time for i in {1..10}; do
    "$BD_BIN" stats > /dev/null
done

# Benchmark: bd blocked (exercises get_blocking_dependencies)
echo
echo "Benchmark: bd blocked"
time for i in {1..10}; do
    "$BD_BIN" blocked > /dev/null
done

# Benchmark: bd ready (exercises has_blocking_dependencies)
echo
echo "Benchmark: bd ready"
time for i in {1..10}; do
    "$BD_BIN" ready > /dev/null
done

# Benchmark: bd list (general performance)
echo
echo "Benchmark: bd list"
time for i in {1..10}; do
    "$BD_BIN" list > /dev/null
done

# Cleanup
cd -
rm -rf "$BENCH_DIR"

echo
echo "=== Benchmark Complete ==="
