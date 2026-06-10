---
title: Cluster numeric IDs first in mb list output
status: closed
priority: 3
issue_type: feature
created_at: 2026-06-10T16:04:02.454595881+00:00
updated_at: 2026-06-10T16:04:12.176727143+00:00
closed_at: 2026-06-10T16:04:12.176726990+00:00
---

# Description

Changed 'mb list' sort order so numeric IDs (e.g. minibeads-42) are clustered first in ascending numeric order (1..N), placing the most recently created numeric issues at the end of that cluster, followed by hash-based IDs ordered by creation date. Previously all issues were sorted by creation date only, which interleaved numeric and hash IDs.

Implemented in src/storage.rs via numeric_id_suffix() helper and compare_for_list() comparator. Added unit tests (list_order_tests). Released in v0.17.0.
