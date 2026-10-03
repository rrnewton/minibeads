---
title: 'Prose merger: reduce conservative refusals over repeated text'
status: open
priority: 4
issue_type: task
created_at: 2026-10-03T08:44:19.228893825+00:00
updated_at: 2026-10-03T14:20:49.263114600+00:00
---

# Description

The prose merger (src/prose_merge.rs) is sound on everything the generated properties produce, but it is conservative. At 300,000 seeds per shape in both roles (2026-10-03_#233(de008f76e3) plus the uncommitted review fixes for minibeads-38), 98.9% of subsumed merges and only 25.7% of disjoint merges came out clean; the rest were refused. The vocabulary has six words, so nearly every alignment is ambiguous. Real prose repeats far less, but refusals there still become conflict hunks a person has to resolve.

Ideas:
- Measure refusal rates on realistic prose. For example, replay pairs of edits from git history of .beads issue files.
- Try refining refusals with a unique-anchor alignment (patience diff), and accept the result when the merge-containment and alignment-agreement checks pass.
- Same-gap concurrent insertions are ordered canonically. Consider whether reporting them as a conflict would be better.
- A single paragraph of about 500 words or more exhausts the piecewise word-level alignment budget (AlignmentWork), so two disjoint edits to it are refused. This was already true at de008f7, before this work. Token distances, used by containment and merge containment, use Myers O((N+M)D) and stay cheap. Making the piecewise alignment linear-ish too (Myers or patience instead of the full LCS table) would lift this limit.

Not a correctness bug: every refusal is reported as a conflict, and nothing is merged silently.
