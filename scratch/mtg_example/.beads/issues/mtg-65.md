---
title: Need ask the choice oracle ALL questions
status: closed
priority: 0
issue_type: task
labels:
  - human
created_at: "2025-10-26T21:06:34Z"
updated_at: "2025-10-27T05:53:47-04:00"
closed_at: "2025-10-27T05:53:47-04:00"
---

# Description

Right now, all the code that follows this line in actions.rs is misguided:

    // Fill in missing targets for effects

The idea is that the choice agent, such as the random agent, should be
making ALL choices.  There should be no auto-targetting of spells or
placeholder targets.  When we need targets, we need to ask the agent
to produce them. We do, however, need to filter the valid targets to
present valid options to the agent.

This will be a large task, but you can get started by incrementally
shifting choices from this "auto-targetting" code over to correct
choices by the agent.  We can run games with the random agent to test
changes and observe which choices are asked of the agent.
