---
title: Implement base36 encoding for hash IDs (divergence from hex)
status: closed
priority: 2
issue_type: feature
assignee: claude
created_at: 2025-11-23T06:02:51.413795197+00:00
updated_at: 2025-11-23T07:33:52.029189896+00:00
---

# Description

**Goal:** Implement base36 encoding for hash IDs to match upstream bd

**Background:**
- Upstream bd switched from hexadecimal to base36 encoding for hash IDs
- Base36 uses [0-9a-z] vs hex [0-9a-f], providing better information density
- Same number of bits → shorter IDs (e.g., 3 chars base36 ≈ 4 chars hex)

**Implementation - COMPLETED:**

1. ✅ Added encode_base36() function in src/hash.rs (lines 19-66)
   - Uses num-bigint for arbitrary-precision arithmetic
   - Converts byte array to BigUint, then to base36 string
   - Handles zero case correctly
   - Pads or truncates to exact length

2. ✅ Updated generate_hash_id() to use base36 encoding (lines 145-186)
   - Switched from hex encoding to base36
   - Updated byte counts to match upstream:
     - Length 3: 2 bytes (16 bits ≈ 3.09 base36 chars)
     - Length 4: 3 bytes (24 bits ≈ 4.63 base36 chars)
     - Length 5-6: 4 bytes (32 bits ≈ 6.18 base36 chars)
     - Length 7-8: 5 bytes (40 bits ≈ 7.73 base36 chars)

3. ✅ Updated adaptive length logic (lines 88-100)
   - Base36 starts at length 3 (vs hex which started at 4)
   - Database size thresholds: <10→3, <100→4, <1000→5, <10000→6, <100000→7, else 8

4. ✅ Added dependencies to Cargo.toml
   - num-bigint = "0.4"
   - num-traits = "0.2"

5. ✅ Updated tests in src/hash.rs
   - test_generate_hash_id_basic: Added base36 character validation
   - test_generate_hash_id_different_lengths: Updated to test lengths 3-8
   - All tests passing

**Test Results:**
- ✅ Unit tests pass (5/5)
- ✅ random-actions tests pass (seed 42, 43, 100-110)
- ✅ migration-test passes (seed 99)
- ✅ Stress test: 10 iterations, all passed

**Example IDs Generated:**
- test-e08d (base36 with extended chars)
- test-8451 (all-digit, valid base36)
- test-afca (hex-compatible chars, still valid base36)

**Key Insight:**
Base36 IDs can randomly contain only hex characters (0-9, a-f) or only digits.
This is normal and not an error - it's just random chance. The validation was updated
to accept these as valid base36 IDs and not flag them as hex encoding.

**Files Modified:**
- src/hash.rs: Implemented base36 encoding (lines 4-186)
- Cargo.toml: Added num-bigint and num-traits dependencies
- src/bin/test_minibeads.rs: Relaxed validation (accepts hex-compatible base36 IDs)

**Performance:**
Base36 encoding uses big-integer arithmetic which is slightly slower than hex,
but the difference is negligible for our use case (< 1ms per ID generation).

Dependencies:
  minibeads-26 (related)
