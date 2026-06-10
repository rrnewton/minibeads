---
title: 'Pre-existing: random_minibeads upstream test fails on config.yaml missing issue-prefix'
status: closed
priority: 2
issue_type: bug
created_at: 2026-06-10T16:04:12.178754905+00:00
updated_at: 2026-06-10T16:59:29.520266179+00:00
closed_at: 2026-06-10T16:59:29.520265984+00:00
---

# Description

test_random_actions_upstream (tests/random_minibeads.rs, seed 42, --impl upstream --ids hash --test-import=false) previously failed with:
  'Error: config.yaml is missing required issue-prefix field'

ROOT CAUSE (not an upstream change): the bundled bd-upstream binary is v0.24.2 and writes a config.yaml template where issue-prefix is COMMENTED OUT (the prefix lives in its SQLite DB). minibeads' Storage::open gained a strict check on 2025-10-30 (commit e930d73) that bailed when config.yaml lacked an active issue-prefix key, so it rejected upstream-created repos. The test had been silently failing make validate for ~7 months.

FIX (committed):
- src/storage.rs: Storage::open no longer bails when issue-prefix is absent; it only validates that config.yaml parses. get_prefix() falls back to inferring the prefix from issue filenames when the key is absent (proper drop-in behavior for upstream repos). Added regression test config_compat_tests::open_tolerates_commented_issue_prefix.
- src/bin/test_minibeads.rs: verify_config tolerates an undeterminable prefix for the upstream case (upstream stores issues+prefix in SQLite, not markdown; the prefix is verified for real via the JSONL dual-export step).

make validate now passes fully.
