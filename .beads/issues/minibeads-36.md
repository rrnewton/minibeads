---
title: 'Flaky test: github_import_creates_only_unlinked_issues fails with ''Text file busy'' under parallel test execution'
status: open
priority: 4
issue_type: bug
created_at: 2026-07-11T10:34:45.605426266+00:00
updated_at: 2026-07-11T10:35:06.275409869+00:00
---

# Description

Observed 2026-07-11_#197(293601f) while running `make validate`.

thread 'github::tests::github_import_creates_only_unlinked_issues' panicked at src/github.rs:3105:92:
called `Result::unwrap()` on an `Err` value: Failed to run /tmp/.tmpXXXXXX/gh-import-fake issue list ...
Caused by: Text file busy (os error 26)

The test writes a fake `gh` script to a temp path and executes it; when
cargo test runs suites in parallel, something races on writing vs exec'ing
a temp binary path, hitting ETXTBSY. Passes reliably when run in isolation
(`cargo test github_import_creates_only_unlinked_issues`).

Likely fix: confirm the fake-gh script path is unique per test invocation
(should already live under a per-test tempdir -- double check it isn't
written to a shared/predictable path), or ensure the writer closes and
syncs the file before making it executable / before exec.

Not consistently reproduced; only seen once locally under `make validate`.
Low priority: test-infra flake, not a product bug.
