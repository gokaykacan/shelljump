//! Terminal lifecycle: raw mode, alternate screen, cursor, keyboard protocol.
//! Restoration is guaranteed on clean exit, on error and on panic.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute, style};

use crate::input::HoldMode;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static ENHANCED: AtomicBool = AtomicBool::new(false);

/// Keyboard protocol features requested from a Kitty-protocol terminal.
///
/// `REPORT_ALL_KEYS_AS_ESCAPE_CODES` is load-bearing, not a nicety:
/// `REPORT_EVENT_TYPES` alone only produces repeat/release events for keys that
/// already travel as escape sequences. Every key this game binds except the
/// arrows is plain text — Space, `a`, `d`, `j` — so without it the input
/// collector runs in [`HoldMode::Explicit`], where it trusts releases
/// completely, while no release for those keys ever arrives. The first press of
/// each would latch down for the whole session.
///
/// `examples/input_probe.rs` pushes this same constant, so what the probe
/// measures is what the game will actually see.
pub const KEYBOARD_ENHANCEMENT_FLAGS: KeyboardEnhancementFlags =
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        .union(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        .union(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES);

/// Owns the terminal's altered state. Dropping it restores the terminal.
pub struct TerminalGuard {
    hold_mode: HoldMode,
}

impl TerminalGuard {
    /// # Errors
    /// Returns the first failure while switching the terminal into game mode.
    /// The terminal is restored before the error propagates.
    pub fn enter() -> io::Result<Self> {
        // Must be queried before the event loop starts: this round-trips with
        // the terminal and would race a concurrent poll/read.
        let enhanced = matches!(terminal::supports_keyboard_enhancement(), Ok(true));

        enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);

        let guard = Self {
            hold_mode: if enhanced {
                HoldMode::Explicit
            } else {
                HoldMode::Timeout
            },
        };

        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, cursor::Hide)?;
        if enhanced {
            execute!(
                out,
                PushKeyboardEnhancementFlags(KEYBOARD_ENHANCEMENT_FLAGS)
            )?;
            ENHANCED.store(true, Ordering::SeqCst);
        }
        Ok(guard)
    }

    /// How key-release information will reach the input pump in this terminal.
    pub fn hold_mode(&self) -> HoldMode {
        self.hold_mode
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Err(error) = restore() {
            // Nothing can be propagated out of Drop, but the failure must not
            // vanish: the user's shell may need manual repair.
            let _ = writeln!(
                io::stderr(),
                "shelljump: failed to restore terminal: {error}"
            );
        }
    }
}

/// Returns the terminal to its original state. Idempotent, so the panic hook
/// and the guard's `Drop` can both call it.
///
/// # Errors
/// Returns the failure from leaving raw mode.
pub fn restore() -> io::Result<()> {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    let mut out = io::stdout();
    if ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, style::ResetColor, cursor::Show, LeaveAlternateScreen);
    disable_raw_mode()
}

/// Installs a panic hook that restores the terminal before the default hook
/// prints the panic message, so the report is readable on the normal screen.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));
}

/// Current terminal size in (columns, rows).
///
/// # Errors
/// Returns the failure from querying the terminal.
pub fn size() -> io::Result<(u16, u16)> {
    terminal::size()
}
