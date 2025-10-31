#!/bin/bash
# Purge upstream bd processes and database files
#
# This script cleans up any lingering upstream bd processes and removes
# SQLite database files that upstream bd creates, ensuring a clean test
# environment for minibeads validation.

set -e

# Kill any running bd-upstream processes
if command -v killall &> /dev/null; then
    killall bd-upstream 2>/dev/null || true
else
    # Fallback for systems without killall
    pkill -f bd-upstream 2>/dev/null || true
fi

# Find and clean all .beads directories in the repo
# Look for:
# - *.db (SQLite database files from upstream)
# - *.db-shm (SQLite shared memory files)
# - *.db-wal (SQLite write-ahead log files)
# - .lock files (upstream bd lock files)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Cleaning upstream bd artifacts from .beads directories..."

# Find all .beads directories and clean them
find "$REPO_ROOT" -type d -name ".beads" | while read -r beads_dir; do
    # Remove SQLite database files
    find "$beads_dir" -type f \( -name "*.db" -o -name "*.db-shm" -o -name "*.db-wal" \) -delete 2>/dev/null || true

    # Remove lock files (but not our .gitignore)
    find "$beads_dir" -type f -name ".lock" -delete 2>/dev/null || true

    # Count what we found (for reporting)
    if [ -n "$(find "$beads_dir" -type f \( -name "*.db*" -o -name ".lock" \) 2>/dev/null)" ]; then
        echo "  Cleaned: $beads_dir"
    fi
done

echo "Purge complete."
