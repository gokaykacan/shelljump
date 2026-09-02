#!/usr/bin/env bash
# Drives the ShellJump TUI binary inside a detached tmux session so an
# agent can send keystrokes and read screen contents programmatically.
# ShellJump takes over the terminal (raw mode + alternate screen) so it
# cannot be driven directly by a plain Bash tool call.
#
# Usage:
#   driver.sh launch [cols] [rows]      start the game in tmux (default 120x40)
#   driver.sh keys <tmux send-keys args...>   send input, e.g: driver.sh keys d
#   driver.sh capture [-e]              print the current screen (-e keeps colors)
#   driver.sh resize <cols> <rows>      resize the tmux window/pane
#   driver.sh quit                      send 'q' and give the app a moment to exit
#   driver.sh wait-exit [timeout_s]     block until the game process is gone
#   driver.sh pid                       print the game's actual PID (not tmux's)
#   driver.sh kill                      signal the game process, then tear
#                                       down the tmux session (last resort)
#
# IMPORTANT: prefer `quit` + `wait-exit` over `kill` to end a session.
# Killing the tmux session out from under the game can orphan the
# process on its pty without delivering the quit key first - it keeps
# running detached from any pane. `kill` therefore signals the game PID
# before removing the session, and `launch` quits any game still running
# in a stale session first. Always confirm `wait-exit` succeeded (or
# `pid` prints nothing) before assuming the game is gone.
#
# Environment:
#   SHELLJUMP_TMUX_SESSION  tmux session name (default: shelljump)
#   SHELLJUMP_BIN           command run inside the pane
#                           (default: ./target/release/shelljump).
#                           It is typed into the pane's shell and executed
#                           as a command line, so set it to trusted values
#                           only - never build it from untrusted input.

set -euo pipefail

SESSION="${SHELLJUMP_TMUX_SESSION:-shelljump}"
BIN="${SHELLJUMP_BIN:-./target/release/shelljump}"

game_pid() {
  local pane_pid
  # The driver only ever creates single-pane sessions; head -1 keeps this
  # correct if a pane is ever added by hand.
  pane_pid=$(tmux list-panes -t "$SESSION" -F '#{pane_pid}' 2>/dev/null | head -1) || return 0
  [ -n "$pane_pid" ] || return 0
  pgrep -P "$pane_pid" -f "$(basename "$BIN")" 2>/dev/null || true
}

# Graceful shutdown of a game left running in an existing session, so
# relaunching never orphans it. Escalates to signals only if 'q' is ignored.
stop_stale_game() {
  local pid i
  pid=$(game_pid)
  [ -n "$pid" ] || return 0
  tmux send-keys -t "$SESSION" 'q' 2>/dev/null || true
  for ((i = 0; i < 10; i++)); do
    sleep 0.2
    if ! kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
  done
  kill "$pid" 2>/dev/null || true
  sleep 0.3
  kill -9 "$pid" 2>/dev/null || true
}

cmd_launch() {
  local cols="${1:-120}" rows="${2:-40}"
  stop_stale_game
  tmux kill-session -t "$SESSION" 2>/dev/null || true
  tmux new-session -d -s "$SESSION" -x "$cols" -y "$rows"
  tmux send-keys -t "$SESSION" "$BIN" Enter
  local pid="" i
  for ((i = 0; i < 10; i++)); do
    sleep 0.2
    pid=$(game_pid)
    if [ -n "$pid" ]; then
      break
    fi
  done
  if [ -z "$pid" ]; then
    echo "launch failed - no game process found; pane content:" >&2
    tmux capture-pane -t "$SESSION" -p >&2
    return 1
  fi
  echo "launched: session=$SESSION pid=$pid size=${cols}x${rows}"
}

cmd_keys() {
  tmux send-keys -t "$SESSION" "$@"
}

cmd_capture() {
  tmux capture-pane -t "$SESSION" -p "$@"
}

cmd_resize() {
  tmux resize-window -t "$SESSION" -x "$1" -y "$2"
}

cmd_quit() {
  tmux send-keys -t "$SESSION" 'q'
  sleep 1
}

cmd_wait_exit() {
  local timeout="${1:-5}" waited=0 pid
  while (( waited < timeout * 2 )); do
    pid=$(game_pid)
    if [ -z "$pid" ]; then
      echo "exited"
      return 0
    fi
    sleep 0.5
    waited=$((waited + 1))
  done
  echo "still running after ${timeout}s (pid=$pid)" >&2
  return 1
}

cmd_pid() {
  game_pid
}

cmd_kill() {
  local pid
  pid=$(game_pid)
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null || true
    sleep 0.3
    kill -9 "$pid" 2>/dev/null || true
  fi
  tmux kill-session -t "$SESSION" 2>/dev/null || true
}

case "${1:-}" in
  launch) shift; cmd_launch "$@" ;;
  keys) shift; cmd_keys "$@" ;;
  capture) shift; cmd_capture "$@" ;;
  resize) shift; cmd_resize "$@" ;;
  quit) shift; cmd_quit "$@" ;;
  wait-exit) shift; cmd_wait_exit "$@" ;;
  pid) shift; cmd_pid "$@" ;;
  kill) shift; cmd_kill "$@" ;;
  *)
    echo "usage: $0 {launch [cols] [rows]|keys <keys...>|capture [-e]|resize <cols> <rows>|quit|wait-exit [timeout_s]|pid|kill}" >&2
    exit 1
    ;;
esac
