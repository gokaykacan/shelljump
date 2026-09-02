---
name: platformer-physics
description: Physics and control conventions for ShellJump's classic Mario-inspired platforming. Use for player movement, jumping, collisions, enemies, platforms and gameplay-feel changes.
user-invocable: false
---

# Platformer Physics

ShellJump targets responsive classic side-scrolling platformer movement.

Gameplay may closely reproduce classic movement concepts, but implementation must remain original.

## Core requirements

Use fixed-timestep simulation.

Rendering frequency must never affect physics.

Centralize tunable movement parameters:

- walk acceleration
- run acceleration
- ground deceleration
- air acceleration
- maximum walk speed
- maximum run speed
- gravity
- jump velocity
- maximum fall velocity
- coyote time
- jump buffer duration
- jump-cut multiplier

## Horizontal movement

Support:

- acceleration rather than instant velocity
- controlled deceleration
- directional reversal
- separate ground and air control
- momentum
- walk/run distinction when introduced

Movement should retain momentum without feeling excessively slippery.

## Jumping

Support:

- grounded jump
- coyote time
- jump buffering
- variable jump height
- jump-cut when jump input is released
- predictable apex behavior

## Collision

Collision resolution must be deterministic.

Handle:

- floor
- ceiling
- walls
- tile corners
- adjacent solid tiles
- high downward velocity
- walking off edges
- jumping into blocks from below

Prevent obvious tunneling.

## Testing

Physics must be testable without Crossterm or terminal initialization.

Important deterministic tests:

- gravity accumulation
- jump start
- jump release
- coyote window
- jump buffering
- landing
- ceiling impact
- wall collision
- terminal velocity
- collision at exact tile boundaries

Gameplay-feel regressions are bugs.

When changing tuning constants, document the behavioral reason.
