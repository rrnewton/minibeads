# AI-generated design and analysis

This directory holds research, design proposals, and implementation handoffs.
These documents can become outdated; they are not a substitute for the source,
tests, or the project's [user-facing README](../README.md).

## Documents

| Document | Scope | Status |
| --- | --- | --- |
| [GitHub sync plan for review](github-sync-plan.md) | Proposed scope, merge behavior, ancestor layout, and acceptance gates | Awaiting human review; existing prototype frozen and unapproved |
| [GitHub sync and prose merging](github-sync-design.md) | Ancestor-based field reconciliation, deterministic prose merging, the Unison comparison, local persistence, and recovery | Historical prototype evidence; frozen and unapproved pending plan review |
| [Cached PR landing triage](pr-landing-triage.md) | Historical cached branch ancestry and risks | Superseded by live #15–#24 stack review; see its session-update banner |

The sync design's source snapshot is
`2026-09-07_#215(9437f50c1a)`. Its integration direction includes the parent's
same-day implementation handoff. Offline integration passes 208 tests, including
24 prose tests and 27 mocked-GitHub sync tests; local formatting and strict Clippy
also pass. One additional standalone migration test passes separately.
Read its status
distinctions before treating a proposal as implemented. External literature pointers are explicitly marked
unverified when their contents could not be retrieved.

## Maintenance

- Keep analysis here, rather than adding speculative material to the root README.
- Separate inspected behavior, implementation direction, and further proposals.
- Date transient claims with `YYYY-MM-DD_#DEPTH(COMMIT)`, using the local date,
  `git rev-list --count HEAD`, and the corresponding commit identifier.
- After integration, reconcile claims with the actual source and tests before
  changing their status. Local implementation does not establish remote CI or
  landing status.
- Keep live ancestry records, issue contents, and recovery artifacts out of this
  directory. Runtime sync state belongs under the resolved project database root.
