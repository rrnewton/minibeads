---
title: Separate minibeads config into config-minibeads.yaml
status: closed
priority: 2
issue_type: task
created_at: 2025-10-31T19:40:41.280959314+00:00
updated_at: 2025-10-31T19:47:43.106968812+00:00
closed_at: 2025-10-31T19:47:43.106968371+00:00
---

# Description

Separate minibeads-specific configuration options from config.yaml into config-minibeads.yaml to prevent confusing or breaking upstream bd when sharing .beads directories.

## Current State

Currently, Storage::init() writes both upstream and minibeads-specific options to config.yaml:
- `issue-prefix` (upstream compatible) ✅
- `mb-hash-ids` (minibeads-specific) ❌ Needs to move

This can cause issues when sharing .beads directories between minibeads and upstream bd.

## Proposed Changes

### 1. Create config-minibeads.yaml

New file with minibeads-specific options, with commented defaults:
```yaml
## Minibeads-specific configuration
## This file is separate from config.yaml to maintain upstream bd compatibility

## Use hash-based IDs instead of sequential numbering
## Default: false (use sequential: project-1, project-2, ...)
## When true: use hash-based IDs (project-a1b2, project-c3d4, ...)
mb-hash-ids: false

## Disable command history logging
## Default: false (enable logging to .beads/command_history.log)
## mb-no-cmd-logging: false
```

### 2. Update Storage::init()

- Create config-minibeads.yaml with mb-hash-ids option
- Remove mb-hash-ids from config.yaml
- Don't clobber existing config-minibeads.yaml if it exists
- Add --mb-hash-ids flag to `bd init` command

### 3. Update Storage::open()

- Validate both config.yaml and config-minibeads.yaml exist
- Create config-minibeads.yaml with defaults if missing
- Read mb-hash-ids from config-minibeads.yaml

### 4. Update use_hash_ids()

- Read from config-minibeads.yaml instead of config.yaml
- Default to false if config-minibeads.yaml doesn't exist

## Files to Modify

- `src/storage.rs` - init(), open(), use_hash_ids()
- `src/main.rs` - Add --mb-hash-ids flag to Init command
- `.beads/.gitignore` - No changes needed (already ignoring log files)

## Testing

- `cargo test` - All unit tests pass
- `bd init --mb-hash-ids` - Creates config-minibeads.yaml with mb-hash-ids: true
- `bd init` - Creates config-minibeads.yaml with mb-hash-ids: false (default)
- Existing .beads directories should auto-create config-minibeads.yaml on open
- Verify upstream bd can still read config.yaml without errors

## Priority

P2 - Important for upstream compatibility but not blocking current work
