---
name: quality-gate
description: Independently validate a completed ShellJump implementation. Use after Developer reports DEV_STATUS READY_FOR_TEST and before code review.
context: fork
agent: tester
background: false
---

Validate the current implementation as the independent Tester.

First inspect:

- current git diff
- Planner acceptance criteria when available
- changed tests
- surrounding affected code

Run:

./scripts/quality-gate.sh

Do not stop at command success.

Also validate behavior implied by the change.

For gameplay changes examine relevant regression scenarios.

For physics changes use deterministic simulation tests.

For renderer changes inspect terminal lifecycle, resize behavior and hot-path allocations.

If coverage is inadequate, add appropriate tests.

Return exactly:

TEST_STATUS: PASS

or:

TEST_STATUS: FAIL

If FAIL, include actionable reproduction and expected behavior.
