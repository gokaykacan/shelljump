---
name: review-change
description: Perform independent senior review of a ShellJump change after the Tester has passed it.
context: fork
agent: reviewer
background: false
---

Review the current implementation independently.

Precondition:

TEST_STATUS must already be PASS.

Inspect:

- git diff
- surrounding implementation
- Planner acceptance criteria
- tests
- architecture impact

Evaluate:

- correctness
- architectural integrity
- Rust quality
- physics correctness
- rendering performance
- portability
- error handling
- regression risk
- test adequacy
- unnecessary complexity

Do not modify production files.

Use Context7 when current external API behavior needs verification.

Return:

REVIEW_STATUS: APPROVED

or:

REVIEW_STATUS: CHANGES_REQUESTED

For blocking findings provide:

- severity
- file/location
- problem
- reason
- recommended correction
