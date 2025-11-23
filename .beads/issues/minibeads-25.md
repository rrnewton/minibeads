---
title: Implement base36 encoding for hash IDs (divergence from hex)
status: open
priority: 2
issue_type: feature
assignee: claude
created_at: 2025-11-23T06:02:51.413795197+00:00
updated_at: 2025-11-23T06:02:51.413795197+00:00
---

# Description

**Current state:** minibeads uses hexadecimal (base16) encoding for hash IDs (0-9, a-f), generating IDs like minibeads-4f10 or minibeads-b127a5.

**Upstream bd:** Uses base36 encoding (0-9, a-z) for better information density, generating IDs like bd-3s9 or bd-0qeg.

**Impact:**
- Base36 allows shorter IDs for same collision resistance
- Hex 4 chars = 16 bits, base36 3 chars ≈ 15.5 bits (similar space)
- Better human readability with full alphabet

**Implementation needed:**
1. Add base36 encoding function (convert bytes to base36 string)
2. Update hash.rs generate_hash_id() to use base36 instead of hex
3. Adjust adaptive length thresholds (upstream uses 3-8 chars, we use 4-8)
4. Update tests to expect base36 format
5. Document migration path for existing databases

**Files to modify:**
- src/hash.rs - Add encodeBase36() and update generate_hash_id()
- tests/* - Update expected hash formats

**Upstream reference:**
- beads/internal/storage/sqlite/ids.go - encodeBase36() implementation
- Uses math/big to convert bytes to base36 representation

**Priority:** P2 - Compatibility improvement, not breaking current functionality
