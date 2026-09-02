---
name: run-shelljump
description: Build, run, and drive ShellJump, the terminal side-scrolling platformer. Use when asked to run, start, launch, playtest, screenshot, or interact with the ShellJump game/binary, or to confirm a gameplay/rendering/input change actually works in the real terminal.
---

ShellJump is a real-time TUI game (Crossterm raw mode + alternate screen) —
it takes over the terminal, so it must be driven through the tmux wrapper
`.claude/skills/run-shelljump/driver.sh`, not called directly from a plain
Bash tool call. All paths below are relative to the repo root.

## Prerequisites

Only if `tmux -V` fails (this repo's dev container had it missing):

```bash
brew install tmux              # macOS
apt-get install -y tmux        # Debian/Ubuntu
```

Rust/Cargo are required to build (see `CLAUDE.md`) — already present in this
environment (`cargo --version` / `rustc --version`).

## Build

```bash
cargo build --release
```

Binary lands at `./target/release/shelljump`.

## Run (agent path)

Use the driver — it wraps tmux launch/input/capture and, critically, gives
you a reliable way to confirm the process actually exited (see Gotchas).

```bash
D=.claude/skills/run-shelljump/driver.sh
$D launch              # starts the game in tmux, 120x40 by default
$D launch 80 24        # or a specific size
```

```bash
$D capture              # plain text screen dump (colors stripped)
$D capture -e            # with ANSI color codes preserved (for verifying
                          # true-color output, e.g. `grep` for a known RGB
                          # sequence like the grass/dirt colors)
```

Send input with `keys` (passes straight through to `tmux send-keys`, so
tmux key names like `Space`, `Escape`, `C-c`, `Left`, `Right` work):

```bash
$D keys d              # hold-equivalent tap: move right
$D keys a               # move left
$D keys j                # run (held for one input window, like the other keys)
$D keys Space            # jump
```

Resize mid-session (tests the resize/reflow path):

```bash
$D resize 80 24
$D capture | wc -l      # should now be 24 lines
```

Quit and confirm the process is really gone (don't skip `wait-exit` — see
Gotchas):

```bash
$D quit
$D wait-exit 5           # blocks until the game process exits, or fails after 5s
$D pid                   # should print nothing
```

Force-cleanup if something went wrong (last resort — see Gotchas before using):

```bash
$D kill
```

| driver command | what it does |
|---|---|
| `launch [cols] [rows]` | quit any game still running in a stale session, then start `target/release/shelljump` in a fresh tmux session (default 120x40) |
| `keys <tmux send-keys args>` | send keystrokes to the running game |
| `capture [-e]` | dump the current screen (`-e` keeps ANSI color codes) |
| `resize <cols> <rows>` | resize the tmux window the game is running in |
| `quit` | send `q` and pause briefly |
| `wait-exit [timeout_s]` | poll until the actual game process (not the tmux pane) exits |
| `pid` | print the game's real PID, empty if not running |
| `kill` | signal the game process directly, then tear down the tmux session |

### Environment variables

| variable | default | purpose |
|---|---|---|
| `SHELLJUMP_TMUX_SESSION` | `shelljump` | tmux session name — override to run a second, independent session in parallel without the two clobbering each other |
| `SHELLJUMP_BIN` | `./target/release/shelljump` | the command run inside the pane — override to drive a different build, e.g. `SHELLJUMP_BIN=./target/debug/shelljump $D launch` |

`SHELLJUMP_BIN` is typed into the tmux pane's shell and executed as a command
line, not merely used as a path string. Set it only to trusted values; never
build it from untrusted input.

## Run (human path)

```bash
cargo run --release
```

Controls: `A`/`Left` move left, `D`/`Right` move right, `J` hold to run,
`Space` jump, `Q`/`Esc`/`Ctrl+C` quit. Terminal must be at least 20x10 or the
game shows a "too small" message instead of the viewport.

## Test

```bash
./scripts/quality-gate.sh
```

Runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D
warnings`, `cargo test --all` (99 tests, headless, no TTY needed), and
`cargo build --release`. All currently pass.

## Gotchas

- **Killing the tmux session does not reliably quit the game.** The game
  process attaches directly to the pty device (`/dev/ttysNNN`), not to the
  tmux session object. If you `tmux kill-session` while the game is still
  running, the process can be reparented and keep running in the
  background, invisible to `tmux list-sessions` and still burning CPU. It
  happened during development of this driver: killing the session left an
  orphaned `shelljump` process consuming CPU with no owning tmux session at
  all. Always send `q` (or `Escape`/`C-c`) and confirm exit with
  `driver.sh wait-exit` — check `driver.sh pid` is empty — **before**
  calling `driver.sh kill` or `tmux kill-session`. `driver.sh kill` and
  `driver.sh launch` both signal/quit the game process before touching the
  session for exactly this reason, but a bare `tmux kill-session` still
  orphans it.
- **`tmux capture-pane -p` (no `-e`) strips colors**, and ShellJump renders
  almost everything with the half-block glyph `▀` — so a plain-text capture
  of a fully-rendered frame looks like a wall of identical `▀` characters;
  that is expected, not a bug. To actually verify rendering (sky vs. dirt
  vs. grass vs. player), capture with `-e` and check for the distinct
  24-bit color escape sequences, or diff two captures across an input to
  confirm the frame actually changed.
- **The game doesn't echo a "ready" banner.** It goes straight into the
  alternate screen and starts drawing the level. There's no text marker to
  poll for like a server's "listening on port" line, so `launch` polls for
  the process itself (every 0.2s, up to 2s) instead of waiting a fixed
  interval.
- **`Event::Resize` needs an actual tmux resize, not just capture at a
  different size.** Use `driver.sh resize`; the pane must really change
  size for the game to receive and handle the resize event.

## Troubleshooting

- **`tmux: command not found`**: `brew install tmux` on macOS, or
  `apt-get install -y tmux` on Debian/Ubuntu (this container didn't have it
  preinstalled).
- **`driver.sh launch` prints "launch failed - no game process found"**:
  the binary likely isn't built yet, or crashed instantly — run
  `cargo build --release` and check `./target/release/shelljump` runs
  directly in a real terminal to see the error before retrying through the
  driver.
