---
name: terminal-rendering
description: Rendering conventions for ShellJump's high-performance Unicode and True Color terminal renderer. Use for framebuffer, camera rendering, Crossterm output, terminal resize and visual effects.
user-invocable: false
---

# Terminal Rendering

ShellJump is a real-time game running inside a terminal, not a conventional TUI.

## Pipeline

Preferred architecture:

Simulation
  ↓
Render Snapshot
  ↓
Camera Transform
  ↓
Framebuffer
  ↓
Diff Previous Frame
  ↓
Terminal Output

Rendering must never become authoritative game state.

## Framebuffer

Maintain:

- current framebuffer
- previous framebuffer

Each logical cell may contain:

- glyph
- foreground color
- background color
- style flags when necessary

Only changed cells should be emitted when practical.

## Terminal behavior

Use:

- alternate screen
- raw mode
- hidden cursor
- Crossterm queueing
- batched writes
- one flush per completed frame where practical

Never flush once per cell.

Do not clear and repaint the entire terminal unless a full redraw is actually necessary.

## Visual capabilities

Use where useful:

█ ▀ ▄ ▌ ▐ ▓ ▒ ░

Support ANSI True Color where available.

Gameplay readability has priority over decorative complexity.

## Resize

Terminal resizing must never:

- panic
- corrupt framebuffer memory
- invalidate camera state
- alter simulation state incorrectly

Reallocate viewport buffers safely.

## Lifecycle

Restore:

- cursor
- terminal mode
- normal screen buffer

after clean termination and error paths.

Do not leave the user's shell in raw mode.

## Performance

Watch for:

- per-frame allocation
- per-cell String allocation
- repeated formatting
- excessive cloning
- repeated terminal queries
- unnecessary full redraws

Optimize measured bottlenecks rather than guessing.
