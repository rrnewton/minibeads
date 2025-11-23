---
title: Add hash ID format validation to test harness
status: open
priority: 2
issue_type: bug
assignee: claude
depends_on:
  minibeads-25: blocks
created_at: 2025-11-23T07:05:43.531072421+00:00
updated_at: 2025-11-23T07:13:36.479689279+00:00
---

# Description

**Problem:** Random tests don't validate hash ID encoding format

**Root Cause:** In test_minibeads.rs lines 1138-1165, when testing hash IDs:
1. Test executes create action in mb or upstream
2. Captures actual_issue_id from result  
3. Updates reference interpreter with whatever ID was returned
4. compare_issue_states() only checks title/status/priority/deps, NOT ID format

This means:
- mb generates test-4f10 (hex) → test accepts it
- upstream generates test-3s9 (base36) → test accepts it
- Test never validates IDs use correct encoding!

**Solution - COMPLETED:**
1. ✅ Added is_base36_hash_id() and is_hex_hash_id() validators (test_minibeads.rs:1272-1333)
2. ✅ Added is_hash_id_mode() helper to detect hash ID mode (test_minibeads.rs:1388-1401)
3. ✅ Added validate_hash_ids_are_base36() function (test_minibeads.rs:1338-1386)
4. ✅ Integrated validation into verify_minibeads_state() (test_minibeads.rs:1567-1570)
5. ✅ Integrated validation into verify_upstream_dual_export() (test_minibeads.rs:1649-1652)
6. ✅ Updated migration test to check for base36 encoding (test_minibeads.rs:439-472)

**Test Results:**
- ✅ Validation correctly detects hex-encoded IDs in minibeads
- ✅ Example: seed 42 → `test-bfcb` flagged as hex-encoded
- ✅ Example: seed 43 → `test-ecad` flagged as hex-encoded  
- ✅ Migration test catches hex IDs (e.g., `test-afbd` with seed 99)
- ✅ Tests now FAIL when minibeads uses hex encoding (as expected!)

**Validation Logic:**
- is_hex_hash_id(): Returns true if ID uses only [0-9a-f] with at least one [a-f]
- validate_hash_ids_are_base36(): Checks all hash IDs, fails if any use hex-only chars
- Edge case: All-digit IDs (e.g., test-9753) are valid for both hex and base36

**Next Steps:**
- Implement base36 encoding in src/hash.rs (tracked in minibeads-25)
- After base36 implementation, tests should pass
- Consider --strict-id-format flag for cross-implementation ID comparison

**Files Modified:**
- src/bin/test_minibeads.rs - added validation functions and integrated into test flow

Dependencies:
  minibeads-25 (blocks)
