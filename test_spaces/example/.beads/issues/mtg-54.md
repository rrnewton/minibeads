---
title: Improve error messages in card loader
status: closed
priority: 4
issue_type: task
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-27T11:05:00Z"
closed_at: "2025-10-27T11:05:00Z"
---

# Description

When cards fail to load from the cardsfolder, the error messages should be more descriptive.

Current behavior: Generic "failed to load card" message
Desired behavior: Show which field failed to parse, line number, and suggested fix

This will save debugging time when adding new cards.

# Resolution

Implemented comprehensive error message improvements in card loader.

**Changes made:**

1. **src/loader/card.rs** - Enhanced `parse()` method:
   - Added line number tracking (enumerate lines)
   - Improved PT field parsing with specific error messages
   - Shows expected format examples in error messages
   - Better error message for missing Name field
   - Format: "Line N: Failed to parse <field> '<value>': <error> (expected format: ...)"

2. **src/loader/card.rs** - Enhanced `load_from_file()`:
   - Wraps parse errors with file path context
   - Format: "Failed to parse card file '<path>': <original error>"

3. **src/loader/database_async.rs** - Enhanced `load_card_async()`:
   - Wraps parse errors with file path context
   - Same format as synchronous loader for consistency

**Example improved error messages:**

Before:
```
Invalid card format: Missing card name
```

After:
```
Failed to parse card file 'cardsfolder/m/mox_pearl.txt': 
Missing required 'Name:' field (add 'Name: <card name>' to the card file)
```

Before:
```
Invalid card format: <generic error>
```

After:
```
Failed to parse card file 'cardsfolder/g/grizzly_bears.txt':
Line 4: Invalid PT format 'X' (expected format: 'N/N', e.g., 'PT:2/2')
```

**Testing:**
- All 360 tests pass
- Handles variable P/T (*, 1+*, etc.) gracefully
- Error messages now include file path, line number, field name, and expected format
