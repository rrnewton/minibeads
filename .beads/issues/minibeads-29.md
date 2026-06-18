---
title: Implement issue claiming (mb claim / update --claim) for cross-machine coordination
status: closed
priority: 2
issue_type: feature
created_at: 2026-06-18T22:20:18.350984628+00:00
updated_at: 2026-06-18T22:20:22.212515899+00:00
closed_at: 2026-06-18T22:20:22.212515776+00:00
---

# Description

Add a claim/release protocol so multiple agent teams on different machines can coordinate through the shared beads markdown via git.

Implemented:
- `mb claim <id>` and `mb update <id> --claim`: atomic compare-and-swap claim. Sets assignee (default = hostname, or host/team via --team), status=in_progress, and stamps claimed_at + claimed_until into frontmatter. Fails if another worker holds an ACTIVE claim.
- `mb claim <id> --release` (with --force to override another holder): clears the claim and reopens the issue.
- Flags: --for <DURATION> (e.g. 48h/2d/90m, default 48h), --team <TEAM>, --as <ACTOR>.
- New optional frontmatter fields claimed_at / claimed_until (minibeads-specific, additive; upstream ignores them). assignee + status stay upstream-compatible.
- Stale recovery (beyond upstream bd): a claim past claimed_until is reclaimable, so a crashed agent never pins an issue forever.

Coordination model: the claim is a small git commit; cross-machine arbitration is the git push (losing push is rejected -> pull -> see it's taken). See mb quickstart 'CLAIMING WORK' section.

Tests: unit tests in types.rs (ClaimDuration parsing, is_actively_claimed), storage.rs (claim_tests: CAS reject, expired reclaim, same-actor refresh, closed guard, release, force-release, sibling updates), format.rs (claim-field round-trip), and e2e tests/claim.sh (full lifecycle + JSONL round-trip). Documented in CHANGELOG.md (0.18.0) and mb quickstart.
