---
title: update --append/--search silently drop other flags in the same call
status: closed
priority: 2
issue_type: bug
created_at: 2026-10-28T15:22:23.697113068+00:00
updated_at: 2026-10-28T15:22:23.709428133+00:00
closed_at: 2026-10-28T15:22:23.709427822+00:00
---

# Description

`mb update --append` and `--search/--replace` each return before the general
field update, so other flags in the same call (`--add-label`, `--remove-label`,
`--set-labels`, `--status`, `--priority`, `--assignee`, `--unassign`,
`--external-ref`, `--description-file`) were silently discarded with exit 0.
A downstream project lost 1,652 triage labels in one pass and 22 more on
2026-10-28.

Fix: add those flags to both modes' clap `conflicts_with_all`, so the
combination is refused loudly. Regression coverage in tests/search_replace.sh
tests 9 and 10 (refusal for each flag with each mode, file untouched; each mode
alone still works; the refused flags work in their own update).
