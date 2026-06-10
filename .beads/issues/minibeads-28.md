---
title: 'Pre-existing: random_minibeads upstream test fails on config.yaml missing issue-prefix'
status: open
priority: 2
issue_type: bug
created_at: 2026-06-10T16:04:12.178754905+00:00
updated_at: 2026-06-10T16:04:12.178754905+00:00
---

# Description

test_random_actions_upstream (tests/random_minibeads.rs, seed 42, --impl upstream --ids hash) fails with:
  'Error: config.yaml is missing required issue-prefix field'

Confirmed pre-existing and unrelated to the mb list ordering change (reproduces on a clean tree with the change stashed). The failure originates from the upstream bd-upstream binary's config validation during the comparison test. This causes 'make validate' to fail at the test stage even though all minibeads-native tests (lib, e2e, bins, and the 6 minibeads-impl random tests) pass.

Reproduce:
  ./target/debug/test_minibeads random-actions --seed 42 --impl upstream --ids hash

Needs investigation: either the upstream bd-upstream binary/submodule is out of date with how minibeads writes config.yaml, or the test harness must seed an issue-prefix into the upstream config. Relates to minibeads-15 (feature completeness vs upstream).
