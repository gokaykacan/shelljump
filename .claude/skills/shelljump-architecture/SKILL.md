---
name: shelljump-architecture
description: ShellJump engine architecture, module boundaries, dependency rules and engineering invariants. Use whenever planning, implementing or reviewing structural game-engine changes.
user-invocable: false
---

# ShellJump Architecture

Preserve strict separation between game simulation and presentation.

## Dependency direction

Preferred dependency flow:

Input
  ↓
Game Commands
  ↓
Simulation
  ├── Physics
  ├── Collision
  ├── Entities
  ├── World
  └── Game State
        ↓
   Render Snapshot
        ↓
     Renderer

The simulation must never depend on terminal APIs.

Crossterm must remain at the application/input/render boundary.

## Core modules

Keep responsibilities separated:

- app: application lifecycle
- time: fixed timestep and frame timing
- input: terminal events → game commands
- world: map and tile data
- entities: player, enemies, objects
- physics: velocities, gravity and motion
- collision: geometry and collision resolution
- camera: world → viewport transform
- rendering: framebuffer and terminal output
- gameplay: rules and interactions
- hud: presentation-only status information

Avoid god modules.

## Simulation rules

Simulation code should:

- be deterministic where practical
- be testable without a terminal
- avoid wall-clock access inside gameplay systems
- receive delta/fixed timestep explicitly
- avoid rendering knowledge
- avoid direct keyboard knowledge

## Rendering rules

Renderer consumes state.

Renderer must never become authoritative game state.

Game behavior must not depend on terminal dimensions except through explicit viewport/camera information.

## Performance

The frame loop is hot-path code.

Avoid:

- repeated heap allocation per cell
- full-screen clearing every frame
- excessive string formatting
- unnecessary cloning of world state
- blocking I/O

Prefer reusable buffers.

## Error handling

Terminal restoration is mandatory after:

- clean exit
- ordinary errors
- panic where technically possible

Never leave the user's terminal in raw mode intentionally.

## Portability

Core simulation must remain platform-independent.

Platform-specific behavior belongs at boundaries.

Initial platforms:

- macOS arm64
- macOS x86_64
- Linux x86_64
- Linux arm64
