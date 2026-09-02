cat > CLAUDE.md <<'EOF'
# ShellJump

ShellJump is a polished real-time side-scrolling platform game that runs entirely inside a terminal.

Gameplay may strongly reproduce the feel and mechanics of classic side-scrolling platform games such as Super Mario Bros, but all distributable characters, names, art, music, sounds, levels and story content must be original.

# Technology

Primary language:

Rust

Terminal:

Crossterm

Rendering:

Custom high-performance terminal framebuffer renderer.

Do not use a general TUI framework for the main gameplay viewport unless there is a proven reason.

# Runtime Architecture

Keep these systems separated:

- app/game state
- game loop/time
- input
- physics
- collisions
- entities
- player controller
- enemies
- tile map/world
- camera
- rendering
- animation
- particles
- HUD
- audio
- save/progression

Simulation must not depend on the terminal renderer.

Another renderer should theoretically be capable of consuming the same simulation.

# Game Loop

Use:

- real-time non-blocking input
- fixed-timestep simulation
- rendering up to 60 FPS
- buffered terminal output
- alternate terminal screen
- raw input
- hidden cursor
- safe terminal restoration

Never clear and redraw the entire terminal unnecessarily.

Maintain an in-memory framebuffer and diff against the previous rendered frame.

# Visual Technology

Use where useful:

- ANSI True Color
- Unicode
- block characters
- half blocks
- layered rendering

Examples:

█ ▀ ▄ ▌ ▐ ▓ ▒ ░

The target is a surprisingly polished terminal game, not a primitive ASCII demo.

# Core Gameplay Target

Eventually support:

- acceleration
- deceleration
- momentum
- walking
- running
- gravity
- variable jump height
- coyote time
- jump buffering
- solid tiles
- platforms
- breakable blocks
- question-style interactive blocks
- coins
- power-ups
- enemy stomping
- pipes
- pits
- checkpoints
- secrets
- scrolling camera
- lives
- score
- timer
- death
- respawn
- level completion
- multiple levels

# Required Agent Workflow

The main Claude session is ORCHESTRATOR.

The orchestrator owns task progression.

For every substantial feature:

Planner
→ Researcher when external/current research is needed
→ Developer
→ Tester
→ Reviewer
→ Committer

Do not stop after an intermediate agent completes.

The orchestrator must continue the workflow.

Failure loops:

Tester FAIL
→ Developer
→ Tester

Reviewer CHANGES_REQUESTED
→ Developer
→ Tester
→ Reviewer

Repeat until:

TEST_STATUS: PASS
REVIEW_STATUS: APPROVED

Only then invoke Committer.

Agents must not approve their own work.

# Model Responsibilities

Planner:
Claude Fable 5.1

Developer:
Claude Opus 5

Reviewer:
Claude Fable 5.1

Researcher:
Claude Sonnet 5

Tester:
Claude Sonnet 5

Committer:
Claude Haiku 4.5

Releaser:
Claude Sonnet 5

Do not silently replace these roles with the main model when their specialization applies.

# Research Rules

Never guess version-sensitive third-party APIs.

Use Context7 when implementation depends on:

- Rust crate APIs
- Crossterm behavior
- terminal APIs
- external libraries
- current GitHub Actions behavior
- current release tooling

Prefer authoritative documentation.

# GitHub

Official GitHub MCP is read-only.

Use it for:

- repository state
- issues
- pull requests
- Actions
- CI investigation

Do not depend on MCP for local git.

Use git locally through Bash.

Only Committer or Releaser may push or perform release-related Git operations.

Never:

- force push
- delete remote branches without explicit permission
- rewrite published history
- commit secrets

# Testing

Every feature must pass:

./scripts/quality-gate.sh

Never weaken tests simply to make them pass.

Test physics and simulation independently from terminal rendering whenever possible.

# Distribution

Players must not need:

- Rust
- Cargo
- Claude
- Node.js
- Python

Target standalone binaries:

- macOS arm64
- macOS x86_64
- Linux x86_64
- Linux arm64

Target installation UX:

curl -fsSL https://DOMAIN/install.sh | sh

Then:

shelljump

# Agent Teams

Agent Teams are available but should not be used for every simple feature.

Prefer normal subagents for sequential work.

Use an Agent Team when parallel independent reasoning materially improves the result, particularly:

- architectural exploration
- competing debugging hypotheses
- gameplay research
- performance investigation
- substantial milestone review

When an Agent Team is used, the main session remains team lead.

Project agent definitions should be reused as teammate types when appropriate.

Avoid multiple agents editing the same files concurrently.

# First Principle

Do not optimize for producing code quickly.

Optimize for building a maintainable, polished, responsive, portable game that remains easy to extend.
EOF