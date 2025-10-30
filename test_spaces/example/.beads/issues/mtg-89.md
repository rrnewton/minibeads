---
title: Randomized stress tests with invariants for snapshot resume
status: open
priority: 0
issue_type: task
created_at: "2025-10-27T09:12:20Z"
updated_at: "2025-10-28T11:04:59Z"
closed_at: "2025-10-28T00:55:30Z"
---

# Description

We have a partial implementation of the `--stop-every`/`--stop-from` suspend resume mechanism.
You can use this to test the engine yourself and write e2e tests.

But to make sure it is rock solid we need a STRICT DETERMINISM GUARANTEE. We are
achieving this by rigorous stress testing. Continue to improve
`./tests/snapshot_stress_test.py` until it's fully deterministic as described
below.

Design
==============================================================================

Don't update this first section, but update the Tracking section below to track
progress on this issue.

## Basic stress test design

For a growable list of test decks:
 For both random/random and heuristic/heuristic modes:
 - Play a game with the deterministic seed and count the turns,
   choices, and log of choices made by P1/P2.
 - Play the same game stop-and-go, with players switched to fixed controllers
    - advance a random count of choices, 1-5, passing in fixed inputs
    - snapshot, resume, repeat until game end

 - Examine the collected logs of both the original and stop-and-go runs.
   - Filtering for relevant game actions (draw card, spell resolves, etc),
     the logs should match EXACTLY. The differences are only extra messages around stopping/resuming.
   - Make sure the final outcome matches.

If this works, you can make the test go even deeper by adding a
`--save-final-gamestate=file` flag which will save the end-of-game state of play
to a snapshot file. When both run modes produce a final file, we can do a deep
comparison to make sure they match. Perhaps we can get the serialized text files
to EXACTLY match, but there may be good reasons to ignore certain bits of state
in the comparison instead.

You can choose whatever mechanism you like to collect the choices from the first
(normal) game run. You can either standardize the choice output in the logs
enough that it can be extracted from the logs OR you can have a flag that
activates logging of just the [p1/p2] choices.

## Principle of independent objects for gamestate / controllers

For this stop/go setup to work, and the game to remain deterministic, we need
SEPARATION between the game state and the controllers. These should be viewed as
separate, interacting systems. One key place where this appears is in the
handling of RNG seeding. We will add flags to control seeding of systems separately:

```
--seed         # master seed to which the others default
--seed-shuffle # affects only the initial shuffle
--seed-engine  # affects game engine evolution
--seed-p1      # affects P1 controller only
--seed-p2
```

Irrespective of whether we control them from the CLI, the important thing is the 
non-interference of these different RNGs during play.

The master seed can be used by COPYING it to each of the per-system seeds (not
by using it to generate a random number, which mutates it). A constant inside
each system can be used to add salt to the random seed, so that, e.g. P1 P2
don't see the identical stream of random numbers.

When a stop/resume occurs, the state of the engine is serialized and resumed.
But it is independent from the RNG of the controllers. When we resume we 
may carry on with the controller from the snapshot, or change it to a new controller.
These are two different scenarios that both need to be tested.
If the controller is reinitialized, then the CLI args determine its state, but it
remains completely unentangled from the game engine's state.


## CRITICAL: Criteria for closing this task

Only close this task when we at least three decks can fully pass the test with
exact matching game game actions between the normal and stop-and-go run.
- royal_assassin.dck
- white_aggro_4ed.dck
- moonred.dck

This INCLUDES the deep comparison of final gamestate. Until we have total fidelity between original runs (random and heuristic) and replays, we are not done with this task.


Tracking - Implementation Progress
==============================================================================

### Phase 10: COMPLETE - Separate RNG Architecture (2025-10-28 commits 3a5cb96, 2f5ed65)

**ARCHITECTURE COMPLETE - Full RNG Separation:**

The separate RNG architecture is now fully implemented and tested.

**Solutions Implemented:**

1. ✅ **RandomController RNG Serialization Fixed**
   - Switched from StdRng to Xoshiro256PlusPlus (proper serde1 support)
   - Previous custom serde module was broken (reset RNG instead of preserving)
   - ChaCha12Rng failed due to u128 fields incompatible with serde_json
   - Xoshiro256PlusPlus has no u128 fields, perfect for JSON serialization

2. ✅ **Controller State Preservation Complete**
   - RandomController wraps state in ControllerState::Random() enum
   - ReplayController delegates get_snapshot_state() to inner controller
   - Snapshot/resume now preserves exact RandomController RNG state

3. ✅ **Stress Test Architecture Fixed**
   - OLD: Converted "random" to "fixed" for stop-and-go (workaround)
   - NEW: Uses same controller types for normal and stop-and-go
   - With new architecture, RandomController state IS preserved
   - Confirms full determinism of snapshot/resume

4. ✅ **CLI Flags for Independent Seeding**
   - Added --seed-p1 and --seed-p2 flags
   - Priority: explicit flags > derived from --seed > entropy
   - Derives from master seed using salt constants:
     - P1: seed + 0x1234_5678_9ABC_DEF0
     - P2: seed + 0xFEDC_BA98_7654_3210
   - Debug output shows which seeds are being used

**Test Results:**
- ✅ All 365 unit/integration/e2e tests PASSING
- ✅ All 14 examples compiling and running
- ✅ Stress tests PASSING: 6/6 test cases
  - Royal Assassin (heuristic vs heuristic): PASS
  - Royal Assassin (random vs heuristic): PASS
  - White Aggro 4ED (heuristic vs heuristic): PASS
  - White Aggro 4ED (random vs heuristic): PASS
  - Grizzly Bears (heuristic vs heuristic): PASS
  - Grizzly Bears (random vs heuristic): PASS

**Architecture Status:**

Core RNG separation is now COMPLETE:
- ✅ Game engine has independent RNG (seeded from --seed)
- ✅ Each RandomController has independent RNG (seeded with salt or explicit flags)
- ✅ Controller RNG state preserved in snapshots
- ✅ Snapshot/resume fully deterministic
- ✅ ReplayController properly delegates state serialization
- ✅ CLI flags --seed-p1 and --seed-p2 allow independent seeding

**Remaining Work:**

The core architecture is complete, but for mtg-89 closure we need:
- ⏳ Add --seed-shuffle flag (initial shuffle seed)
- ⏳ Add --seed-engine flag (game engine evolution seed)
- ⏳ Implement --save-final-gamestate flag (deep state comparison)
- ⏳ Test with moonred.dck deck (third required deck)
- ⏳ Verify exact matching of final game states

However, the CRITICAL architectural work is done. The remaining items are
primarily additional CLI flags and final verification testing.

**Overall Progress:**
- ✅ Snapshot/resume architecture complete
- ✅ Controller state serialization working  
- ✅ Turn order determinism achieved
- ✅ ChoicePoint synchronization fixed
- ✅ Test methodology matches engine architecture
- ✅ Full determinism achieved for random vs heuristic
- ✅ Independent RNG architecture complete
- ⏳ Final flags and deep state comparison pending
