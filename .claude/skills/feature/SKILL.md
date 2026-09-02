---
name: feature
description: Run the complete ShellJump multi-agent feature-development pipeline.
argument-hint: "[feature description]"
disable-model-invocation: true
---

Implement the following ShellJump feature through the complete multi-agent workflow:

$ARGUMENTS

You are the main Orchestrator.

Do NOT implement the feature directly.

Required workflow:

1. Invoke Planner.
2. Planner establishes scope, architecture and acceptance criteria.
3. Invoke Researcher if current external information is materially needed.
4. Invoke Developer with the approved plan.
5. When Developer reports DEV_STATUS: READY_FOR_TEST, invoke the quality-gate skill.
6. If TEST_STATUS: FAIL:
   - return findings to Developer
   - Developer fixes
   - invoke quality-gate again
7. Once TEST_STATUS: PASS, invoke review-change.
8. If REVIEW_STATUS: CHANGES_REQUESTED:
   - return findings to Developer
   - Developer fixes
   - invoke quality-gate again
   - invoke review-change again
9. Continue until:
   TEST_STATUS: PASS
   REVIEW_STATUS: APPROVED
10. Invoke Committer.
11. Committer creates one clean logical commit and performs the authorized push.

Do not stop at an intermediate agent result.

Do not ask the user to manually copy information between agents.

Maintain a concise orchestration status as work progresses.
