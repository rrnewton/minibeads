---
title: Add targeted search/replace editing for mb update (--search/--replace)
status: closed
priority: 3
issue_type: feature
created_at: 2026-06-19T03:41:02.032552672+00:00
updated_at: 2026-06-19T03:56:13.353585620+00:00
closed_at: 2026-06-19T03:56:13.353585004+00:00
---

# Description

Agents are tempted to hand-edit .beads/*.md files because the only way to revise a description via the CLI was --description, which overwrites the WHOLE field (error-prone, drops content).

Add an aider-style search/replace edit mode to `mb update`:
- `--search TEXT --replace TEXT` swaps an exact substring instead of overwriting.
- `--field {title,description,design,notes,acceptance}` selects the field (default description).
- `--replace-all` to rewrite every occurrence.
- Default requires exactly one match; missing/ambiguous match is an error and the file is left untouched.
- `--search` conflicts with the wholesale field setters and --claim; requires --replace.

minibeads-specific (upstream bd up to 0.49 has no equivalent; additive extension).

Implemented: EditField enum + Issue::text_field_mut (types.rs), Storage::search_replace_issue (storage.rs), CLI wiring + handler (main.rs). Docs: README, CHANGELOG, and `mb quickstart` now steer agents to this method instead of hand-editing files. Tests: 6 unit tests (storage::search_replace_tests) + tests/search_replace.sh (13 assertions).
